//! Local filesystem storage backend with SQLite metadata.
//!
//! Storage layout per spec:
//! ```text
//! /var/lib/strix/
//!   objects/
//!     ab/cd/<object_id>.blob   (encrypted if SSE enabled)
//!   tmp/
//!   multipart/
//!   meta/
//!     strix.db
//!     encryption.key           (master key for SSE-S3)
//! ```

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::StreamExt;
use rusqlite::{OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_rusqlite::Connection;
use tracing::{debug, info, instrument, warn};
use ulid::Ulid;

use strix_core::{
    AuditLogEntry, AuditQueryOpts, BucketInfo, CompletePart, CopyObjectOpts, CopyObjectResponse,
    CorsConfiguration, CreateBucketOpts, DeleteObjectResponse, EncryptionInfo, Error,
    GetObjectOpts, GetObjectResponse, LegalHoldStatus, LifecycleConfiguration, ListObjectsOpts,
    ListObjectsResponse, ListPartsOpts, ListPartsResponse, ListUploadsOpts, ListUploadsResponse,
    ListVersionsOpts, ListVersionsResponse, MetadataDirective, MultipartUpload,
    NotificationConfiguration, ObjectBody, ObjectInfo, ObjectLockConfiguration, ObjectRetention,
    ObjectStore, ObjectVersion, PartInfo, PutObjectOpts, PutObjectResponse, Result, RetentionMode,
    ServerSideEncryption, StorageClass, TaggingConfiguration, TenantInfo,
};
use strix_crypto::{
    KEY_SIZE, decrypt_aes256_gcm, derive_key, encrypt_aes256_gcm, format_etag,
    format_multipart_etag, generate_encryption_key, validate_sse_c_key,
};

use crate::db::{self, BucketRow, ObjectRow, PartRow, TenantRow, UploadRow, VersioningStatus};

/// Size threshold for warning about large encrypted objects.
/// Encryption currently requires loading the object into memory.
/// Objects larger than this will log a warning but still work.
const LARGE_ENCRYPTED_OBJECT_THRESHOLD: u64 = 5 * 1024 * 1024 * 1024;

/// Local filesystem storage backend with SQLite metadata.
pub struct LocalFsStore {
    root: PathBuf,
    db: Connection,
    /// Master encryption key for SSE-S3 (AES-256)
    master_key: [u8; KEY_SIZE],
}

/// Convert tokio-rusqlite errors to our error type
fn db_err(e: tokio_rusqlite::Error) -> Error {
    Error::Internal(format!("Database error: {}", e))
}

impl LocalFsStore {
    /// Create a new LocalFsStore with the given root directory.
    pub async fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();

        // Create directory structure
        let objects_dir = root.join("objects");
        let tmp_dir = root.join("tmp");
        let multipart_dir = root.join("multipart");
        let meta_dir = root.join("meta");

        fs::create_dir_all(&objects_dir).await?;
        fs::create_dir_all(&tmp_dir).await?;
        fs::create_dir_all(&multipart_dir).await?;
        fs::create_dir_all(&meta_dir).await?;

        // Open SQLite database
        let db_path = meta_dir.join("strix.db");
        let db = Connection::open(&db_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to open database: {}", e)))?;

        // Initialize schema and run migrations
        db.call(|conn| {
            db::init_schema(conn)?;
            crate::migrations::run_migrations(conn)?;
            Ok(())
        })
        .await
        .map_err(db_err)?;

        // Load or create master encryption key for SSE-S3
        let key_path = meta_dir.join("encryption.key");
        let master_key = Self::load_or_create_master_key(&key_path).await?;

        Ok(Self {
            root,
            db,
            master_key,
        })
    }

    /// Load or create the master encryption key for SSE-S3.
    ///
    /// Security: The key file is created with 0o600 permissions (owner read/write only).
    /// If an existing file has insecure permissions, a warning is logged.
    async fn load_or_create_master_key(key_path: &Path) -> Result<[u8; KEY_SIZE]> {
        if key_path.exists() {
            // Check file permissions on Unix systems
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(key_path).await {
                    let mode = metadata.permissions().mode() & 0o777;
                    if mode != 0o600 {
                        warn!(
                            "Encryption key file has insecure permissions {:o}, should be 600",
                            mode
                        );
                        // Attempt to fix permissions
                        if let Err(e) =
                            fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))
                                .await
                        {
                            warn!("Failed to fix key file permissions: {}", e);
                        } else {
                            info!("Fixed encryption key file permissions to 600");
                        }
                    }
                }
            }

            // Load existing key
            let key_data = fs::read(key_path).await?;
            if key_data.len() != KEY_SIZE {
                return Err(Error::Internal(
                    "Invalid encryption key file size".to_string(),
                ));
            }
            let mut key = [0u8; KEY_SIZE];
            key.copy_from_slice(&key_data);
            debug!("Loaded encryption master key");
            Ok(key)
        } else {
            // Generate new key
            let key = generate_encryption_key();

            // Write key file with secure permissions on Unix
            #[cfg(unix)]
            {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600) // Owner read/write only
                    .open(key_path)
                    .await?;
                file.write_all(&key).await?;
                file.sync_all().await?;
            }

            // On non-Unix systems, use regular write
            #[cfg(not(unix))]
            {
                fs::write(key_path, &key).await?;
            }

            info!("Generated new encryption master key");
            Ok(key)
        }
    }

    /// Get the blob path for an object ID (sharded by first 4 chars).
    fn blob_path(&self, object_id: &str) -> PathBuf {
        let prefix1 = &object_id[0..2];
        let prefix2 = &object_id[2..4];
        self.root
            .join("objects")
            .join(prefix1)
            .join(prefix2)
            .join(format!("{}.blob", object_id))
    }

    /// Get the tmp path for an in-progress write.
    fn tmp_path(&self, id: &str) -> PathBuf {
        self.root.join("tmp").join(format!("{}.tmp", id))
    }

    /// Get the multipart part path.
    fn part_path(&self, upload_id: &str, part_number: u16) -> PathBuf {
        self.root
            .join("multipart")
            .join(upload_id)
            .join(format!("{:05}.part", part_number))
    }

    /// Validate bucket name per S3 rules.
    fn validate_bucket_name(name: &str) -> Result<()> {
        if name.len() < 3 || name.len() > 63 {
            return Err(Error::InvalidBucketName(
                "Bucket name must be 3-63 characters".to_string(),
            ));
        }
        if name.starts_with('-') || name.ends_with('-') {
            return Err(Error::InvalidBucketName(
                "Bucket name cannot start or end with hyphen".to_string(),
            ));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        {
            return Err(Error::InvalidBucketName(
                "Bucket name can only contain lowercase letters, numbers, hyphens, and periods"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Parse datetime string from SQLite.
    fn parse_datetime(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|dt| dt.and_utc())
            })
            .unwrap_or_else(|_| Utc::now())
    }

    async fn object_delete_locked(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<bool> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        let version_id = version_id.map(|v| v.to_string());

        let state = self
            .db
            .call(move |conn| {
                let sql = if version_id.is_some() {
                    "SELECT retention_mode, retain_until_date, legal_hold FROM objects WHERE bucket = ?1 AND key = ?2 AND version_id = ?3"
                } else {
                    "SELECT retention_mode, retain_until_date, legal_hold FROM objects WHERE bucket = ?1 AND key = ?2 AND is_latest = 1"
                };

                let result: Option<(Option<String>, Option<String>, i64)> = if let Some(vid) = version_id {
                    conn.query_row(sql, params![&bucket, &key, &vid], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .optional()?
                } else {
                    conn.query_row(sql, params![&bucket, &key], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .optional()?
                };

                Ok(result)
            })
            .await
            .map_err(db_err)?;

        let Some((retention_mode, retain_until, legal_hold)) = state else {
            return Ok(false);
        };

        if legal_hold != 0 {
            return Ok(true);
        }

        if retention_mode.is_some() {
            if let Some(until) = retain_until {
                let until_dt = Self::parse_datetime(&until);
                if Utc::now() < until_dt {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Parse storage class from string.
    fn parse_storage_class(s: &str) -> StorageClass {
        match s {
            "REDUCED_REDUNDANCY" => StorageClass::ReducedRedundancy,
            "GLACIER" => StorageClass::Glacier,
            "DEEP_ARCHIVE" => StorageClass::DeepArchive,
            _ => StorageClass::Standard,
        }
    }

    /// Get bucket usage statistics (object count and total size).
    pub async fn get_bucket_usage(&self, bucket: &str) -> Result<(u64, u64)> {
        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                let (count, size): (i64, i64) = conn
                    .query_row(
                        "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM objects WHERE bucket = ?1",
                        params![&bucket],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap_or((0, 0));
                Ok((count as u64, size as u64))
            })
            .await
            .map_err(db_err)
    }

    /// Derive an object-specific encryption key for SSE-S3.
    fn derive_object_key(&self, bucket: &str, key: &str, version_id: &str) -> [u8; KEY_SIZE] {
        let context = format!("object:{}:{}:{}", bucket, key, version_id);
        derive_key(&self.master_key, context.as_bytes())
    }

    /// Encrypt data for SSE-S3 using the server's master key.
    fn encrypt_sse_s3(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
        data: &[u8],
    ) -> Result<Vec<u8>> {
        let object_key = self.derive_object_key(bucket, key, version_id);
        encrypt_aes256_gcm(&object_key, data)
            .map_err(|e| Error::Internal(format!("Encryption failed: {}", e)))
    }

    /// Decrypt data encrypted with SSE-S3.
    fn decrypt_sse_s3(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>> {
        let object_key = self.derive_object_key(bucket, key, version_id);
        decrypt_aes256_gcm(&object_key, ciphertext)
            .map_err(|e| Error::Internal(format!("Decryption failed: {}", e)))
    }

    /// Encrypt data with a customer-provided key (SSE-C).
    fn encrypt_sse_c(key: &[u8; KEY_SIZE], data: &[u8]) -> Result<Vec<u8>> {
        encrypt_aes256_gcm(key, data)
            .map_err(|e| Error::Internal(format!("Encryption failed: {}", e)))
    }

    /// Decrypt data with a customer-provided key (SSE-C).
    fn decrypt_sse_c(key: &[u8; KEY_SIZE], ciphertext: &[u8]) -> Result<Vec<u8>> {
        decrypt_aes256_gcm(key, ciphertext)
            .map_err(|e| Error::Internal(format!("Decryption failed: {}", e)))
    }

    /// Parse encryption info from database row.
    fn parse_encryption_info(
        algorithm: &Option<String>,
        key_md5: &Option<String>,
    ) -> Option<EncryptionInfo> {
        algorithm.as_ref().map(|alg| EncryptionInfo {
            algorithm: match alg.as_str() {
                "AES256" => ServerSideEncryption::Aes256,
                "SSE-C" => ServerSideEncryption::SseC,
                _ => ServerSideEncryption::Aes256,
            },
            sse_customer_key_md5: key_md5.clone(),
        })
    }

    /// List stale multipart uploads (older than the specified hours).
    /// Used by the background cleanup task.
    pub async fn list_stale_uploads(&self, hours: u32) -> Result<Vec<MultipartUpload>> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        let uploads = self
            .db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r#"SELECT upload_id, bucket, key, initiated_at
                       FROM multipart_uploads
                       WHERE initiated_at < ?1
                       ORDER BY initiated_at"#,
                )?;

                let rows: Vec<(String, String, String, String)> = stmt
                    .query_map(params![&cutoff_str], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(rows)
            })
            .await
            .map_err(db_err)?;

        Ok(uploads
            .into_iter()
            .map(|(upload_id, bucket, key, initiated_at)| MultipartUpload {
                upload_id,
                bucket,
                key,
                initiated: Self::parse_datetime(&initiated_at),
            })
            .collect())
    }
}

#[async_trait]
impl ObjectStore for LocalFsStore {
    // === Bucket Operations ===

    #[instrument(skip(self))]
    async fn create_bucket(&self, bucket: &str, opts: CreateBucketOpts) -> Result<()> {
        Self::validate_bucket_name(bucket)?;

        let bucket = bucket.to_string();
        let bucket_for_error = bucket.clone();
        let region = opts.region;
        let tenant_slug = opts.tenant_slug;

        let result = self
            .db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO buckets (name, region, tenant_slug) VALUES (?1, ?2, ?3)",
                    params![&bucket, &region, &tenant_slug],
                )?;
                Ok(())
            })
            .await;

        match result {
            Ok(_) => {
                debug!("Created bucket");
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed") {
                    Err(Error::BucketAlreadyExists(bucket_for_error))
                } else {
                    Err(db_err(e))
                }
            }
        }
    }

    #[instrument(skip(self))]
    async fn delete_bucket(&self, bucket: &str) -> Result<()> {
        let bucket_name = bucket.to_string();

        // Check existence and emptiness
        let (exists, has_objects) = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    let exists: bool = conn
                        .query_row(
                            "SELECT 1 FROM buckets WHERE name = ?1",
                            params![&bucket],
                            |_| Ok(true),
                        )
                        .unwrap_or(false);

                    let has_objects: bool = conn
                        .query_row(
                            "SELECT 1 FROM objects WHERE bucket = ?1 LIMIT 1",
                            params![&bucket],
                            |_| Ok(true),
                        )
                        .unwrap_or(false);

                    Ok((exists, has_objects))
                }
            })
            .await
            .map_err(db_err)?;

        if !exists {
            return Err(Error::BucketNotFound(bucket_name));
        }
        if has_objects {
            return Err(Error::BucketNotEmpty(bucket_name));
        }

        self.db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    conn.execute("DELETE FROM buckets WHERE name = ?1", params![&bucket])?;
                    Ok(())
                }
            })
            .await
            .map_err(db_err)?;

        debug!("Deleted bucket: {}", bucket_name);
        Ok(())
    }

    async fn list_buckets(&self) -> Result<Vec<BucketInfo>> {
        let buckets = self
            .db
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, tenant_slug, region, versioning_status, object_lock_enabled, created_at FROM buckets ORDER BY name",
                )?;

                let rows = stmt.query_map([], |row| {
                    Ok(BucketRow {
                        name: row.get(0)?,
                        tenant_slug: row.get(1)?,
                        region: row.get(2)?,
                        versioning_status: VersioningStatus::from_db_str(
                            &row.get::<_, String>(3).unwrap_or_default(),
                        ),
                        object_lock_enabled: row.get::<_, i64>(4).unwrap_or(0) != 0,
                        created_at: row.get(5)?,
                    })
                })?;

                let mut buckets = Vec::new();
                for row in rows {
                    buckets.push(row?);
                }
                Ok(buckets)
            })
            .await
            .map_err(db_err)?;

        Ok(buckets
            .into_iter()
            .map(|b| BucketInfo {
                name: b.name,
                tenant_slug: b.tenant_slug,
                created_at: Self::parse_datetime(&b.created_at),
                versioning_enabled: match b.versioning_status {
                    VersioningStatus::Unversioned => None,
                    VersioningStatus::Enabled => Some(true),
                    VersioningStatus::Suspended => Some(false),
                },
                object_locking_enabled: b.object_lock_enabled,
            })
            .collect())
    }

    async fn bucket_exists(&self, bucket: &str) -> Result<bool> {
        let bucket = bucket.to_string();

        self.db
            .call(move |conn| {
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM buckets WHERE name = ?1",
                        params![&bucket],
                        |_| Ok(true),
                    )
                    .unwrap_or(false);
                Ok(exists)
            })
            .await
            .map_err(db_err)
    }

    async fn head_bucket(&self, bucket: &str) -> Result<BucketInfo> {
        let bucket_name = bucket.to_string();

        let row = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    Ok(conn.query_row(
                        "SELECT name, tenant_slug, region, versioning_status, object_lock_enabled, created_at FROM buckets WHERE name = ?1",
                        params![&bucket],
                        |row| {
                            Ok(BucketRow {
                                name: row.get(0)?,
                                tenant_slug: row.get(1)?,
                                region: row.get(2)?,
                                versioning_status: VersioningStatus::from_db_str(
                                    &row.get::<_, String>(3).unwrap_or_default(),
                                ),
                                object_lock_enabled: row.get::<_, i64>(4).unwrap_or(0) != 0,
                                created_at: row.get(5)?,
                            })
                        },
                    )?)
                }
            })
            .await;

        match row {
            Ok(r) => Ok(BucketInfo {
                name: r.name,
                tenant_slug: r.tenant_slug,
                created_at: Self::parse_datetime(&r.created_at),
                versioning_enabled: match r.versioning_status {
                    VersioningStatus::Unversioned => None,
                    VersioningStatus::Enabled => Some(true),
                    VersioningStatus::Suspended => Some(false),
                },
                object_locking_enabled: r.object_lock_enabled,
            }),
            Err(_) => Err(Error::BucketNotFound(bucket_name)),
        }
    }

    async fn list_tenants(&self) -> Result<Vec<TenantInfo>> {
        let rows = self
            .db
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, slug, owner, notes, created_at FROM tenants ORDER BY slug",
                )?;
                let mapped = stmt.query_map([], |row| {
                    Ok(TenantRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        slug: row.get(2)?,
                        owner: row.get(3)?,
                        notes: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                })?;

                let mut out = Vec::new();
                for row in mapped {
                    out.push(row?);
                }
                Ok(out)
            })
            .await
            .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| TenantInfo {
                id: r.id,
                name: r.name,
                slug: r.slug,
                owner: r.owner,
                notes: r.notes,
                created_at: Self::parse_datetime(&r.created_at),
            })
            .collect())
    }

    async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        owner: &str,
        notes: Option<&str>,
    ) -> Result<TenantInfo> {
        let id = Ulid::new().to_string();
        let name_s = name.to_string();
        let slug_s = slug.to_string();
        let owner_s = owner.to_string();
        let notes_s = notes.map(ToString::to_string);

        let res = self
            .db
            .call({
                let id = id.clone();
                let name = name_s.clone();
                let slug = slug_s.clone();
                let owner = owner_s.clone();
                let notes = notes_s.clone();
                move |conn| {
                    conn.execute(
                        "INSERT INTO tenants (id, name, slug, owner, notes) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![id, name, slug, owner, notes],
                    )?;
                    Ok(())
                }
            })
            .await;

        match res {
            Ok(_) => self.get_tenant(slug).await,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed") {
                    Err(Error::TenantAlreadyExists(slug.to_string()))
                } else {
                    Err(db_err(e))
                }
            }
        }
    }

    async fn get_tenant(&self, slug: &str) -> Result<TenantInfo> {
        let slug_s = slug.to_string();
        let row = self
            .db
            .call({
                let slug = slug_s.clone();
                move |conn| {
                    Ok(conn.query_row(
                        "SELECT id, name, slug, owner, notes, created_at FROM tenants WHERE slug = ?1",
                        params![slug],
                        |row| {
                            Ok(TenantRow {
                                id: row.get(0)?,
                                name: row.get(1)?,
                                slug: row.get(2)?,
                                owner: row.get(3)?,
                                notes: row.get(4)?,
                                created_at: row.get(5)?,
                            })
                        },
                    )?)
                }
            })
            .await;

        match row {
            Ok(r) => Ok(TenantInfo {
                id: r.id,
                name: r.name,
                slug: r.slug,
                owner: r.owner,
                notes: r.notes,
                created_at: Self::parse_datetime(&r.created_at),
            }),
            Err(_) => Err(Error::TenantNotFound(slug_s)),
        }
    }

    async fn delete_tenant(&self, slug: &str) -> Result<()> {
        let slug_s = slug.to_string();

        let bucket_count = self
            .db
            .call({
                let slug = slug_s.clone();
                move |conn| {
                    let count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM buckets WHERE tenant_slug = ?1",
                        params![slug],
                        |row| row.get(0),
                    )?;
                    Ok(count)
                }
            })
            .await
            .map_err(db_err)?;

        if bucket_count > 0 {
            return Err(Error::InvalidArgument(
                "cannot delete tenant with existing buckets".to_string(),
            ));
        }

        let deleted = self
            .db
            .call({
                let slug = slug_s.clone();
                move |conn| {
                    let rows =
                        conn.execute("DELETE FROM tenants WHERE slug = ?1", params![slug])?;
                    Ok(rows)
                }
            })
            .await
            .map_err(db_err)?;

        if deleted == 0 {
            return Err(Error::TenantNotFound(slug_s));
        }

        Ok(())
    }

    // === Object Operations ===

    #[instrument(skip(self))]
    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        opts: GetObjectOpts,
    ) -> Result<GetObjectResponse> {
        let bucket_str = bucket.to_string();
        let key_str = key.to_string();
        let version_id = opts.version_id.clone();

        // Get object metadata from database (including encryption info)
        let obj_row = self
            .db
            .call({
                let bucket = bucket_str.clone();
                let key = key_str.clone();
                let version_id = version_id.clone();
                move |conn| {
                    // If version_id specified, get that specific version; otherwise get latest
                    let sql = if version_id.is_some() {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           is_multipart, part_count, is_latest, is_delete_marker,
                           encryption_algorithm, encryption_key_md5, encryption_nonce,
                           retention_mode, retain_until_date, legal_hold,
                           created_at, last_modified
                           FROM objects WHERE bucket = ?1 AND key = ?2 AND version_id = ?3"#
                    } else {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           is_multipart, part_count, is_latest, is_delete_marker,
                           encryption_algorithm, encryption_key_md5, encryption_nonce,
                           retention_mode, retain_until_date, legal_hold,
                           created_at, last_modified
                           FROM objects WHERE bucket = ?1 AND key = ?2 AND is_latest = 1"#
                    };

                    if let Some(vid) = version_id {
                        Ok(conn.query_row(sql, params![&bucket, &key, &vid], |row| {
                            Ok(ObjectRow {
                                id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                version_id: row.get(3)?,
                                size: row.get(4)?,
                                etag: row.get(5)?,
                                content_type: row.get(6)?,
                                content_encoding: row.get(7)?,
                                content_disposition: row.get(8)?,
                                cache_control: row.get(9)?,
                                storage_class: row.get(10)?,
                                user_metadata: row.get(11)?,
                                is_multipart: row.get::<_, i64>(12)? != 0,
                                part_count: row.get(13)?,
                                is_latest: row.get::<_, i64>(14)? != 0,
                                is_delete_marker: row.get::<_, i64>(15)? != 0,
                                encryption_algorithm: row.get(16)?,
                                encryption_key_md5: row.get(17)?,
                                encryption_nonce: row.get(18)?,
                                retention_mode: row.get(19)?,
                                retain_until_date: row.get(20)?,
                                legal_hold: row.get::<_, i64>(21).unwrap_or(0) != 0,
                                created_at: row.get(22)?,
                                last_modified: row.get(23)?,
                            })
                        })?)
                    } else {
                        Ok(conn.query_row(sql, params![&bucket, &key], |row| {
                            Ok(ObjectRow {
                                id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                version_id: row.get(3)?,
                                size: row.get(4)?,
                                etag: row.get(5)?,
                                content_type: row.get(6)?,
                                content_encoding: row.get(7)?,
                                content_disposition: row.get(8)?,
                                cache_control: row.get(9)?,
                                storage_class: row.get(10)?,
                                user_metadata: row.get(11)?,
                                is_multipart: row.get::<_, i64>(12)? != 0,
                                part_count: row.get(13)?,
                                is_latest: row.get::<_, i64>(14)? != 0,
                                is_delete_marker: row.get::<_, i64>(15)? != 0,
                                encryption_algorithm: row.get(16)?,
                                encryption_key_md5: row.get(17)?,
                                encryption_nonce: row.get(18)?,
                                retention_mode: row.get(19)?,
                                retain_until_date: row.get(20)?,
                                legal_hold: row.get::<_, i64>(21).unwrap_or(0) != 0,
                                created_at: row.get(22)?,
                                last_modified: row.get(23)?,
                            })
                        })?)
                    }
                }
            })
            .await
            .map_err(|_| Error::ObjectNotFound {
                bucket: bucket_str.clone(),
                key: key_str.clone(),
            })?;

        // If this is a delete marker, return not found
        if obj_row.is_delete_marker {
            return Err(Error::ObjectNotFound {
                bucket: bucket_str,
                key: key_str,
            });
        }

        // Handle conditional requests
        if let Some(ref if_match) = opts.if_match {
            // Strip quotes from ETag for comparison
            let etag = obj_row.etag.trim_matches('"');
            let match_etag = if_match.trim_matches('"');
            if etag != match_etag && if_match != "*" {
                return Err(Error::PreconditionFailed);
            }
        }
        if let Some(ref if_none_match) = opts.if_none_match {
            let etag = obj_row.etag.trim_matches('"');
            let none_match_etag = if_none_match.trim_matches('"');
            if etag == none_match_etag || if_none_match == "*" {
                return Err(Error::NotModified);
            }
        }

        // Handle conditional time-based requests
        let last_modified = Self::parse_datetime(&obj_row.last_modified);
        if let Some(if_modified_since) = opts.if_modified_since {
            // Return 304 Not Modified if object hasn't been modified since the specified time
            if last_modified <= if_modified_since {
                return Err(Error::NotModified);
            }
        }
        if let Some(if_unmodified_since) = opts.if_unmodified_since {
            // Return 412 Precondition Failed if object has been modified since the specified time
            if last_modified > if_unmodified_since {
                return Err(Error::PreconditionFailed);
            }
        }

        let metadata: HashMap<String, String> = obj_row
            .user_metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // Return version_id only if it's not "null" (unversioned marker)
        let version_id = if obj_row.version_id == "null" {
            None
        } else {
            Some(obj_row.version_id.clone())
        };

        // Parse encryption info
        let encryption =
            Self::parse_encryption_info(&obj_row.encryption_algorithm, &obj_row.encryption_key_md5);

        let info = ObjectInfo {
            key: obj_row.key.clone(),
            size: obj_row.size as u64,
            etag: obj_row.etag.clone(),
            content_type: obj_row.content_type.clone(),
            last_modified: Self::parse_datetime(&obj_row.last_modified),
            metadata,
            storage_class: Self::parse_storage_class(&obj_row.storage_class),
            version_id: version_id.clone(),
            is_latest: obj_row.is_latest,
            is_delete_marker: obj_row.is_delete_marker,
            encryption: encryption.clone(),
        };

        // Open blob file and read data
        let blob_path = self.blob_path(&obj_row.id);
        let ciphertext = fs::read(&blob_path).await.map_err(|e| {
            warn!("Failed to read blob {}: {}", blob_path.display(), e);
            Error::ObjectNotFound {
                bucket: bucket.to_string(),
                key: key.to_string(),
            }
        })?;

        // Decrypt if encrypted
        let plaintext = if let Some(ref enc) = encryption {
            match enc.algorithm {
                ServerSideEncryption::Aes256 => {
                    // SSE-S3: Decrypt with server key
                    let vid = version_id.as_deref().unwrap_or("null");
                    self.decrypt_sse_s3(bucket, key, vid, &ciphertext)?
                }
                ServerSideEncryption::SseC => {
                    // SSE-C: Require customer key
                    let key_b64 = opts.sse_customer_key.as_ref().ok_or_else(|| {
                        Error::InvalidArgument(
                            "Object is encrypted with SSE-C, customer key required".to_string(),
                        )
                    })?;
                    let sse_key = validate_sse_c_key(key_b64, opts.sse_customer_key_md5.as_deref())
                        .map_err(|e| Error::InvalidArgument(format!("Invalid SSE-C key: {}", e)))?;
                    Self::decrypt_sse_c(&sse_key, &ciphertext)?
                }
            }
        } else {
            ciphertext
        };

        // Handle range requests on decrypted data
        let total_size = plaintext.len();
        let (start, length) = if let Some((req_start, req_end)) = opts.range {
            if total_size == 0 {
                return Err(Error::InvalidRange(
                    "Range requested on empty object".to_string(),
                ));
            }

            let start = req_start as usize;
            if start >= total_size {
                return Err(Error::InvalidRange(format!(
                    "Range start {} beyond object size {}",
                    req_start, total_size
                )));
            }

            let end = req_end
                .map(|e| e as usize)
                .unwrap_or(total_size.saturating_sub(1));

            if end < start {
                return Err(Error::InvalidRange(format!(
                    "Range end {} before start {}",
                    end, start
                )));
            }

            let clamped_end = end.min(total_size.saturating_sub(1));
            (start, clamped_end.saturating_sub(start) + 1)
        } else {
            (0, total_size)
        };

        // Create stream from the (possibly partial) plaintext
        let data = plaintext[start..start + length.min(total_size - start)].to_vec();
        let stream = async_stream::try_stream! {
            yield Bytes::from(data);
        };

        Ok(GetObjectResponse {
            info,
            body: Box::pin(stream),
        })
    }

    #[instrument(skip(self, body))]
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: ObjectBody,
        _size: u64,
        opts: PutObjectOpts,
    ) -> Result<PutObjectResponse> {
        // Get bucket info including versioning status
        let bucket_name = bucket.to_string();
        let versioning_status = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT versioning_status FROM buckets WHERE name = ?1",
                            params![&bucket],
                            |row| row.get::<_, String>(0),
                        )
                        .ok())
                }
            })
            .await
            .map_err(db_err)?;

        let versioning_status =
            versioning_status.ok_or_else(|| Error::BucketNotFound(bucket_name.clone()))?;
        let is_versioned = versioning_status == "Enabled";

        // Generate object ID (ULID)
        let object_id = Ulid::new().to_string();

        // version_id is the object_id for versioned buckets, "null" for non-versioned
        let version_id = if is_versioned {
            object_id.clone()
        } else {
            "null".to_string()
        };

        // Validate SSE-C key if provided
        let sse_c_key: Option<[u8; KEY_SIZE]> = if let Some(ref key_b64) = opts.sse_customer_key {
            Some(
                validate_sse_c_key(key_b64, opts.sse_customer_key_md5.as_deref())
                    .map_err(|e| Error::InvalidArgument(format!("Invalid SSE-C key: {}", e)))?,
            )
        } else {
            None
        };

        // Determine if encryption is needed upfront
        let needs_encryption = opts.server_side_encryption.is_some();

        // Stream body data to temp file while computing MD5
        // This is memory-efficient for non-encrypted objects
        let tmp_path = self.tmp_path(&format!("{}-upload", object_id));
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
            .await?;

        let mut total_size: u64 = 0;
        let mut hasher = md5::Context::new();
        let mut body = body;
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            total_size += chunk.len() as u64;

            // Compute MD5 while streaming
            hasher.consume(&chunk);
            tmp_file.write_all(&chunk).await?;
        }
        tmp_file.flush().await?;
        tmp_file.sync_all().await?;
        drop(tmp_file);

        // Warn about large encrypted objects (they need to be loaded into memory)
        if needs_encryption && total_size > LARGE_ENCRYPTED_OBJECT_THRESHOLD {
            tracing::warn!(
                "Encrypting large object ({} bytes). This requires loading the object into memory. \
                Consider using multipart upload for objects larger than {} bytes.",
                total_size,
                LARGE_ENCRYPTED_OBJECT_THRESHOLD
            );
        }

        // For encrypted objects, we need to read the data back into memory
        // For non-encrypted objects, we can use the temp file directly
        let plaintext = if needs_encryption {
            let mut data = Vec::with_capacity(total_size as usize);
            let read_file = File::open(&tmp_path).await?;
            let mut reader = BufReader::new(read_file);
            let mut buf = vec![0u8; 64 * 1024]; // 64KB read buffer
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..n]);
            }
            drop(reader);
            data
        } else {
            Vec::new() // Not used for non-encrypted objects
        };

        let etag = format_etag(&format!("{:x}", hasher.compute()));
        let original_size = total_size;

        // Determine encryption settings and handle file appropriately
        let (encryption_algorithm, encryption_key_md5) = if let Some(sse) =
            &opts.server_side_encryption
        {
            match sse {
                ServerSideEncryption::Aes256 => {
                    // SSE-S3: Use server-managed key
                    let encrypted = self.encrypt_sse_s3(bucket, key, &version_id, &plaintext)?;

                    // Write encrypted data to final location
                    let final_tmp_path = self.tmp_path(&object_id);
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&final_tmp_path)
                        .await?;
                    file.write_all(&encrypted).await?;
                    file.flush().await?;
                    file.sync_all().await?;
                    drop(file);

                    // Remove the plaintext temp file
                    let _ = fs::remove_file(&tmp_path).await;

                    // Move encrypted file to final location
                    let blob_path = self.blob_path(&object_id);
                    if let Some(parent) = blob_path.parent() {
                        fs::create_dir_all(parent).await?;
                    }
                    fs::rename(&final_tmp_path, &blob_path).await?;

                    (Some("AES256".to_string()), None)
                }
                ServerSideEncryption::SseC => {
                    // SSE-C: Use customer-provided key
                    let sse_key = sse_c_key.ok_or_else(|| {
                        Error::InvalidArgument("SSE-C requires customer key".to_string())
                    })?;
                    let encrypted = Self::encrypt_sse_c(&sse_key, &plaintext)?;

                    // Write encrypted data to final location
                    let final_tmp_path = self.tmp_path(&object_id);
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&final_tmp_path)
                        .await?;
                    file.write_all(&encrypted).await?;
                    file.flush().await?;
                    file.sync_all().await?;
                    drop(file);

                    // Remove the plaintext temp file
                    let _ = fs::remove_file(&tmp_path).await;

                    // Move encrypted file to final location
                    let blob_path = self.blob_path(&object_id);
                    if let Some(parent) = blob_path.parent() {
                        fs::create_dir_all(parent).await?;
                    }
                    fs::rename(&final_tmp_path, &blob_path).await?;

                    let key_md5 = opts.sse_customer_key_md5.clone();
                    (Some("SSE-C".to_string()), key_md5)
                }
            }
        } else {
            // No encryption - just rename temp file to final location
            let blob_path = self.blob_path(&object_id);
            if let Some(parent) = blob_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::rename(&tmp_path, &blob_path).await?;

            (None, None)
        };

        // Insert/update metadata in database
        let bucket = bucket.to_string();
        let key = key.to_string();
        let etag_clone = etag.clone();
        let content_type = opts.content_type;
        let content_encoding = opts.content_encoding;
        let content_disposition = opts.content_disposition;
        let cache_control = opts.cache_control;
        let storage_class = opts.storage_class.unwrap_or_default().to_string();
        let user_metadata = serde_json::to_string(&opts.metadata).ok();
        let object_id_for_log = object_id.clone();
        let version_id_clone = version_id.clone();

        self.db
            .call(move |conn| {
                if is_versioned {
                    // Mark all existing versions as not latest
                    conn.execute(
                        "UPDATE objects SET is_latest = 0 WHERE bucket = ?1 AND key = ?2",
                        params![&bucket, &key],
                    )?;
                } else {
                    // For unversioned buckets, delete existing object
                    conn.execute(
                        "DELETE FROM objects WHERE bucket = ?1 AND key = ?2",
                        params![&bucket, &key],
                    )?;
                }

                // Insert new object with encryption metadata
                conn.execute(
                    r#"INSERT INTO objects (id, bucket, key, version_id, size, etag, content_type,
                       content_encoding, content_disposition, cache_control, storage_class,
                       user_metadata, is_latest, is_delete_marker, encryption_algorithm, encryption_key_md5)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, 0, ?13, ?14)"#,
                    params![
                        &object_id,
                        &bucket,
                        &key,
                        &version_id_clone,
                        original_size as i64,
                        &etag_clone,
                        &content_type,
                        &content_encoding,
                        &content_disposition,
                        &cache_control,
                        &storage_class,
                        &user_metadata,
                        &encryption_algorithm,
                        &encryption_key_md5,
                    ],
                )?;

                Ok(())
            })
            .await
            .map_err(db_err)?;

        debug!(
            "Put object: {} bytes -> {}",
            original_size, object_id_for_log
        );

        // Return version_id only for versioned buckets
        let response_version_id = if is_versioned { Some(version_id) } else { None };

        Ok(PutObjectResponse {
            etag,
            version_id: response_version_id,
        })
    }

    #[instrument(skip(self))]
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        // Use delete_object_version with no version_id (soft delete)
        let _ = self.delete_object_version(bucket, key, None).await?;
        Ok(())
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectInfo> {
        let bucket_str = bucket.to_string();
        let key_str = key.to_string();

        let obj = self
            .db
            .call({
                let bucket = bucket_str.clone();
                let key = key_str.clone();
                move |conn| {
                    Ok(conn.query_row(
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           is_multipart, part_count, is_latest, is_delete_marker,
                           encryption_algorithm, encryption_key_md5, encryption_nonce,
                           retention_mode, retain_until_date, legal_hold,
                           created_at, last_modified
                           FROM objects WHERE bucket = ?1 AND key = ?2 AND is_latest = 1"#,
                        params![&bucket, &key],
                        |row| {
                            Ok(ObjectRow {
                                id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                version_id: row.get(3)?,
                                size: row.get(4)?,
                                etag: row.get(5)?,
                                content_type: row.get(6)?,
                                content_encoding: row.get(7)?,
                                content_disposition: row.get(8)?,
                                cache_control: row.get(9)?,
                                storage_class: row.get(10)?,
                                user_metadata: row.get(11)?,
                                is_multipart: row.get::<_, i64>(12)? != 0,
                                part_count: row.get(13)?,
                                is_latest: row.get::<_, i64>(14)? != 0,
                                is_delete_marker: row.get::<_, i64>(15)? != 0,
                                encryption_algorithm: row.get(16)?,
                                encryption_key_md5: row.get(17)?,
                                encryption_nonce: row.get(18)?,
                                retention_mode: row.get(19)?,
                                retain_until_date: row.get(20)?,
                                legal_hold: row.get::<_, i64>(21).unwrap_or(0) != 0,
                                created_at: row.get(22)?,
                                last_modified: row.get(23)?,
                            })
                        },
                    )?)
                }
            })
            .await
            .map_err(|_| Error::ObjectNotFound {
                bucket: bucket_str.clone(),
                key: key_str.clone(),
            })?;

        // If this is a delete marker, return not found
        if obj.is_delete_marker {
            return Err(Error::ObjectNotFound {
                bucket: bucket_str,
                key: key_str,
            });
        }

        let metadata: HashMap<String, String> = obj
            .user_metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // Return version_id only if it's not "null" (unversioned marker)
        let version_id = if obj.version_id == "null" {
            None
        } else {
            Some(obj.version_id)
        };

        // Parse encryption info
        let encryption =
            Self::parse_encryption_info(&obj.encryption_algorithm, &obj.encryption_key_md5);

        Ok(ObjectInfo {
            key: obj.key,
            size: obj.size as u64,
            etag: obj.etag,
            content_type: obj.content_type,
            last_modified: Self::parse_datetime(&obj.last_modified),
            metadata,
            storage_class: Self::parse_storage_class(&obj.storage_class),
            version_id,
            is_latest: obj.is_latest,
            is_delete_marker: obj.is_delete_marker,
            encryption,
        })
    }

    async fn list_objects(
        &self,
        bucket: &str,
        opts: ListObjectsOpts,
    ) -> Result<ListObjectsResponse> {
        // Verify bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let prefix = opts.prefix.clone().unwrap_or_default();
        let delimiter = opts.delimiter.clone();
        let max_keys = opts.max_keys.unwrap_or(1000) as usize;
        let start_after = opts.start_after.clone().or(opts.continuation_token.clone());

        let objects = self
            .db
            .call({
                let bucket = bucket.clone();
                let prefix = prefix.clone();
                let start_after = start_after.clone();
                move |conn| {
                    let prefix_pattern = format!("{}%", prefix.replace('%', "\\%"));

                    // Only return latest versions that are not delete markers
                    let sql = if start_after.is_some() {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           is_multipart, part_count, is_latest, is_delete_marker,
                           encryption_algorithm, encryption_key_md5, encryption_nonce,
                           retention_mode, retain_until_date, legal_hold,
                           created_at, last_modified
                           FROM objects WHERE bucket = ?1 AND key LIKE ?2 AND key > ?3
                           AND is_latest = 1 AND is_delete_marker = 0
                           ORDER BY key"#
                    } else {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           is_multipart, part_count, is_latest, is_delete_marker,
                           encryption_algorithm, encryption_key_md5, encryption_nonce,
                           retention_mode, retain_until_date, legal_hold,
                           created_at, last_modified
                           FROM objects WHERE bucket = ?1 AND key LIKE ?2
                           AND is_latest = 1 AND is_delete_marker = 0
                           ORDER BY key"#
                    };

                    let mut stmt = conn.prepare(sql)?;

                    let rows: Vec<ObjectRow> = if let Some(start) = start_after {
                        stmt.query_map(params![&bucket, &prefix_pattern, &start], |row| {
                            Ok(ObjectRow {
                                id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                version_id: row.get(3)?,
                                size: row.get(4)?,
                                etag: row.get(5)?,
                                content_type: row.get(6)?,
                                content_encoding: row.get(7)?,
                                content_disposition: row.get(8)?,
                                cache_control: row.get(9)?,
                                storage_class: row.get(10)?,
                                user_metadata: row.get(11)?,
                                is_multipart: row.get::<_, i64>(12)? != 0,
                                part_count: row.get(13)?,
                                is_latest: row.get::<_, i64>(14)? != 0,
                                is_delete_marker: row.get::<_, i64>(15)? != 0,
                                encryption_algorithm: row.get(16)?,
                                encryption_key_md5: row.get(17)?,
                                encryption_nonce: row.get(18)?,
                                retention_mode: row.get(19)?,
                                retain_until_date: row.get(20)?,
                                legal_hold: row.get::<_, i64>(21).unwrap_or(0) != 0,
                                created_at: row.get(22)?,
                                last_modified: row.get(23)?,
                            })
                        })?
                        .filter_map(|r| r.ok())
                        .collect()
                    } else {
                        stmt.query_map(params![&bucket, &prefix_pattern], |row| {
                            Ok(ObjectRow {
                                id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                version_id: row.get(3)?,
                                size: row.get(4)?,
                                etag: row.get(5)?,
                                content_type: row.get(6)?,
                                content_encoding: row.get(7)?,
                                content_disposition: row.get(8)?,
                                cache_control: row.get(9)?,
                                storage_class: row.get(10)?,
                                user_metadata: row.get(11)?,
                                is_multipart: row.get::<_, i64>(12)? != 0,
                                part_count: row.get(13)?,
                                is_latest: row.get::<_, i64>(14)? != 0,
                                is_delete_marker: row.get::<_, i64>(15)? != 0,
                                encryption_algorithm: row.get(16)?,
                                encryption_key_md5: row.get(17)?,
                                encryption_nonce: row.get(18)?,
                                retention_mode: row.get(19)?,
                                retain_until_date: row.get(20)?,
                                legal_hold: row.get::<_, i64>(21).unwrap_or(0) != 0,
                                created_at: row.get(22)?,
                                last_modified: row.get(23)?,
                            })
                        })?
                        .filter_map(|r| r.ok())
                        .collect()
                    };

                    Ok(rows)
                }
            })
            .await
            .map_err(db_err)?;

        let mut result_objects = Vec::new();
        let mut common_prefixes = std::collections::BTreeSet::new();

        for obj in objects {
            // Handle delimiter for common prefixes (only if delimiter is non-empty)
            if let Some(ref delim) = delimiter {
                if !delim.is_empty() {
                    let suffix = &obj.key[prefix.len()..];
                    if let Some(pos) = suffix.find(delim.as_str()) {
                        let common_prefix = format!("{}{}{}", prefix, &suffix[..pos], delim);
                        common_prefixes.insert(common_prefix);
                        continue;
                    }
                }
            }

            let metadata: HashMap<String, String> = obj
                .user_metadata
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            // Return version_id only if it's not "null" (unversioned marker)
            let version_id = if obj.version_id == "null" {
                None
            } else {
                Some(obj.version_id.clone())
            };

            // Parse encryption info
            let encryption =
                Self::parse_encryption_info(&obj.encryption_algorithm, &obj.encryption_key_md5);

            result_objects.push(ObjectInfo {
                key: obj.key,
                size: obj.size as u64,
                etag: obj.etag,
                content_type: obj.content_type,
                last_modified: Self::parse_datetime(&obj.last_modified),
                metadata,
                storage_class: Self::parse_storage_class(&obj.storage_class),
                version_id,
                is_latest: obj.is_latest,
                encryption,
                is_delete_marker: obj.is_delete_marker,
            });

            if result_objects.len() > max_keys {
                break;
            }
        }

        let is_truncated = result_objects.len() > max_keys;
        if is_truncated {
            result_objects.truncate(max_keys);
        }

        let next_token = if is_truncated {
            result_objects.last().map(|o| o.key.clone())
        } else {
            None
        };

        Ok(ListObjectsResponse {
            objects: result_objects,
            common_prefixes: common_prefixes.into_iter().collect(),
            is_truncated,
            next_continuation_token: next_token,
        })
    }

    #[instrument(skip(self))]
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        opts: CopyObjectOpts,
    ) -> Result<CopyObjectResponse> {
        // Get source object
        let src_info = self.head_object(src_bucket, src_key).await?;

        // Get source blob path and encryption info
        let src_bucket_str = src_bucket.to_string();
        let src_key_str = src_key.to_string();

        let (src_object_id, src_encryption_algorithm, src_encryption_key_md5): (String, Option<String>, Option<String>) = self
            .db
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT id, encryption_algorithm, encryption_key_md5 FROM objects WHERE bucket = ?1 AND key = ?2 AND is_latest = 1",
                    params![&src_bucket_str, &src_key_str],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .await
            .map_err(db_err)?;

        // Get destination bucket versioning status
        let dst_bucket_name = dst_bucket.to_string();
        let versioning_status = self
            .db
            .call({
                let bucket = dst_bucket_name.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT versioning_status FROM buckets WHERE name = ?1",
                            params![&bucket],
                            |row| row.get::<_, String>(0),
                        )
                        .ok())
                }
            })
            .await
            .map_err(db_err)?;

        let versioning_status =
            versioning_status.ok_or_else(|| Error::BucketNotFound(dst_bucket_name.clone()))?;
        let is_versioned = versioning_status == "Enabled";

        // Generate new object ID and version_id
        let new_object_id = Ulid::new().to_string();
        let version_id = if is_versioned {
            new_object_id.clone()
        } else {
            "null".to_string()
        };

        // Copy blob file
        let src_blob = self.blob_path(&src_object_id);
        let dst_blob = self.blob_path(&new_object_id);

        if let Some(parent) = dst_blob.parent() {
            fs::create_dir_all(parent).await?;
        }

        // If source is encrypted with SSE-S3, we need to re-encrypt for the destination
        // (because the encryption key is derived from bucket/key/version_id)
        if let Some(ref alg) = src_encryption_algorithm {
            if alg == "AES256" {
                // Read and decrypt source
                let ciphertext = fs::read(&src_blob).await?;
                let src_version = src_info.version_id.as_deref().unwrap_or("null");
                let plaintext =
                    self.decrypt_sse_s3(src_bucket, src_key, src_version, &ciphertext)?;

                // Re-encrypt for destination
                let new_ciphertext =
                    self.encrypt_sse_s3(dst_bucket, dst_key, &version_id, &plaintext)?;
                fs::write(&dst_blob, &new_ciphertext).await?;
            } else {
                // SSE-C: Can't re-encrypt without customer key, just copy the file
                fs::copy(&src_blob, &dst_blob).await?;
            }
        } else {
            // No encryption, just copy the file
            fs::copy(&src_blob, &dst_blob).await?;
        }

        // Insert metadata
        let dst_bucket = dst_bucket.to_string();
        let dst_key = dst_key.to_string();
        let etag = src_info.etag.clone();
        let content_type = src_info.content_type.clone();
        let storage_class = src_info.storage_class.to_string();
        let now = Utc::now();
        let last_modified = now.format("%Y-%m-%d %H:%M:%S").to_string();

        let metadata = match opts.metadata_directive {
            MetadataDirective::Copy => src_info.metadata.clone(),
            MetadataDirective::Replace => opts.metadata,
        };
        let user_metadata = serde_json::to_string(&metadata).ok();
        let size = src_info.size as i64;
        let version_id_clone = version_id.clone();

        self.db
            .call(move |conn| {
                if is_versioned {
                    // Mark existing versions as not latest
                    conn.execute(
                        "UPDATE objects SET is_latest = 0 WHERE bucket = ?1 AND key = ?2",
                        params![&dst_bucket, &dst_key],
                    )?;
                } else {
                    // Delete existing if any
                    conn.execute(
                        "DELETE FROM objects WHERE bucket = ?1 AND key = ?2",
                        params![&dst_bucket, &dst_key],
                    )?;
                }

                conn.execute(
                    r#"INSERT INTO objects (id, bucket, key, version_id, size, etag, content_type,
                       storage_class, user_metadata, is_latest, encryption_algorithm, encryption_key_md5, last_modified)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12)"#,
                    params![
                        &new_object_id,
                        &dst_bucket,
                        &dst_key,
                        &version_id_clone,
                        size,
                        &etag,
                        &content_type,
                        &storage_class,
                        &user_metadata,
                        &src_encryption_algorithm,
                        &src_encryption_key_md5,
                        &last_modified,
                    ],
                )?;

                Ok(())
            })
            .await
            .map_err(db_err)?;

        // Return version_id only for versioned buckets
        let response_version_id = if is_versioned { Some(version_id) } else { None };

        Ok(CopyObjectResponse {
            etag: src_info.etag,
            last_modified: now,
            version_id: response_version_id,
        })
    }

    // === Multipart Upload Operations ===

    #[instrument(skip(self))]
    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        opts: PutObjectOpts,
    ) -> Result<MultipartUpload> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let upload_id = Ulid::new().to_string();
        let bucket = bucket.to_string();
        let key = key.to_string();
        let content_type = opts.content_type;
        let content_encoding = opts.content_encoding;
        let content_disposition = opts.content_disposition;
        let cache_control = opts.cache_control;
        let storage_class = opts.storage_class.unwrap_or_default().to_string();
        let user_metadata = serde_json::to_string(&opts.metadata).ok();
        let now = Utc::now();

        // Determine encryption settings
        let (encryption_algorithm, encryption_key_md5) =
            if let Some(ref sse) = opts.server_side_encryption {
                match sse {
                    ServerSideEncryption::Aes256 => (Some("AES256".to_string()), None),
                    ServerSideEncryption::SseC => {
                        let key_md5 = opts.sse_customer_key_md5.clone();
                        (Some("SSE-C".to_string()), key_md5)
                    }
                }
            } else {
                (None, None)
            };

        // Create multipart directory
        let upload_dir = self.root.join("multipart").join(&upload_id);
        fs::create_dir_all(&upload_dir).await?;

        let upload_id_clone = upload_id.clone();
        let bucket_clone = bucket.clone();
        let key_clone = key.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    r#"INSERT INTO multipart_uploads (upload_id, bucket, key, content_type,
                       content_encoding, content_disposition, cache_control,
                       storage_class, user_metadata, encryption_algorithm, encryption_key_md5)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                    params![
                        &upload_id_clone,
                        &bucket_clone,
                        &key_clone,
                        &content_type,
                        &content_encoding,
                        &content_disposition,
                        &cache_control,
                        &storage_class,
                        &user_metadata,
                        &encryption_algorithm,
                        &encryption_key_md5,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        debug!("Created multipart upload: {}", upload_id);
        Ok(MultipartUpload {
            upload_id,
            bucket,
            key,
            initiated: now,
        })
    }

    #[instrument(skip(self, body))]
    async fn upload_part(
        &self,
        upload: &MultipartUpload,
        part_number: u16,
        body: ObjectBody,
        _size: u64,
    ) -> Result<PartInfo> {
        if !(1..=10000).contains(&part_number) {
            return Err(Error::InvalidPartNumber(part_number));
        }

        // Verify upload exists
        let upload_id = upload.upload_id.clone();
        let exists = self
            .db
            .call({
                let upload_id = upload_id.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT 1 FROM multipart_uploads WHERE upload_id = ?1",
                            params![&upload_id],
                            |_| Ok(true),
                        )
                        .unwrap_or(false))
                }
            })
            .await
            .map_err(db_err)?;

        if !exists {
            return Err(Error::UploadNotFound(upload_id));
        }

        // Generate blob ID for this part
        let blob_id = Ulid::new().to_string();
        let part_path = self.part_path(&upload.upload_id, part_number);

        if let Some(parent) = part_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&part_path)
            .await?;

        let mut hasher = md5::Context::new();
        let mut written = 0u64;

        let mut body = body;
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            hasher.consume(&chunk);
            file.write_all(&chunk).await?;
            written += chunk.len() as u64;
        }

        file.flush().await?;
        file.sync_all().await?;

        let etag = format_etag(&format!("{:x}", hasher.compute()));
        let now = Utc::now();

        let upload_id = upload.upload_id.clone();
        let etag_clone = etag.clone();

        self.db
            .call(move |conn| {
                // Delete existing part if any
                conn.execute(
                    "DELETE FROM parts WHERE upload_id = ?1 AND part_number = ?2",
                    params![&upload_id, part_number as i64],
                )?;

                conn.execute(
                    r#"INSERT INTO parts (upload_id, part_number, blob_id, size, etag)
                       VALUES (?1, ?2, ?3, ?4, ?5)"#,
                    params![
                        &upload_id,
                        part_number as i64,
                        &blob_id,
                        written as i64,
                        &etag_clone,
                    ],
                )?;

                Ok(())
            })
            .await
            .map_err(db_err)?;

        debug!("Uploaded part {} for {}", part_number, upload.upload_id);
        Ok(PartInfo {
            part_number,
            etag,
            size: written,
            last_modified: now,
        })
    }

    #[instrument(skip(self))]
    async fn complete_multipart_upload(
        &self,
        upload: &MultipartUpload,
        parts: Vec<CompletePart>,
    ) -> Result<PutObjectResponse> {
        // Verify parts are in order and validate ETags
        let mut last_part = 0u16;
        for part in &parts {
            if part.part_number <= last_part {
                return Err(Error::InvalidPartOrder);
            }
            last_part = part.part_number;
        }

        // Get upload metadata including encryption settings
        let upload_id = upload.upload_id.clone();
        let upload_meta = self
            .db
            .call({
                let upload_id = upload_id.clone();
                move |conn| {
                    Ok(conn.query_row(
                        r#"SELECT upload_id, bucket, key, content_type, content_encoding,
                           content_disposition, cache_control, storage_class, user_metadata,
                           encryption_algorithm, encryption_key_md5, initiated_at
                           FROM multipart_uploads WHERE upload_id = ?1"#,
                        params![&upload_id],
                        |row| {
                            Ok(UploadRow {
                                upload_id: row.get(0)?,
                                bucket: row.get(1)?,
                                key: row.get(2)?,
                                content_type: row.get(3)?,
                                content_encoding: row.get(4)?,
                                content_disposition: row.get(5)?,
                                cache_control: row.get(6)?,
                                storage_class: row.get(7)?,
                                user_metadata: row.get(8)?,
                                encryption_algorithm: row.get(9)?,
                                encryption_key_md5: row.get(10)?,
                                initiated_at: row.get(11)?,
                            })
                        },
                    )?)
                }
            })
            .await
            .map_err(|_| Error::UploadNotFound(upload_id.clone()))?;

        // Get bucket versioning status
        let bucket_name = upload_meta.bucket.clone();
        let versioning_status = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT versioning_status FROM buckets WHERE name = ?1",
                            params![&bucket],
                            |row| row.get::<_, String>(0),
                        )
                        .ok())
                }
            })
            .await
            .map_err(db_err)?;

        let versioning_status =
            versioning_status.ok_or_else(|| Error::BucketNotFound(bucket_name.clone()))?;
        let is_versioned = versioning_status == "Enabled";

        // Validate ETags and part sizes
        // Minimum part size is 5MB (except for the last part)
        const MIN_PART_SIZE: i64 = 5 * 1024 * 1024; // 5 MB

        let parts_to_validate = parts.clone();
        let upload_id_for_validation = upload.upload_id.clone();
        let num_parts = parts.len();

        // Result: Ok(0) = success, Ok(1) = ETag mismatch, Ok(2) = part too small, Ok(3) = part not found
        let validation_result: i32 = self
            .db
            .call(move |conn| {
                for (idx, part) in parts_to_validate.iter().enumerate() {
                    let result = conn.query_row(
                        "SELECT etag, size FROM parts WHERE upload_id = ?1 AND part_number = ?2",
                        params![&upload_id_for_validation, part.part_number as i64],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                    );

                    let (stored_etag, stored_size) = match result {
                        Ok(v) => v,
                        Err(_) => return Ok(3), // Part not found
                    };

                    // Compare ETags (strip quotes for comparison)
                    let stored = stored_etag.trim_matches('"');
                    let provided = part.etag.trim_matches('"');
                    if stored != provided {
                        return Ok(1); // ETag mismatch
                    }

                    // Check minimum part size (except for the last part)
                    let is_last_part = idx == num_parts - 1;
                    if !is_last_part && stored_size < MIN_PART_SIZE {
                        return Ok(2); // Part too small
                    }
                }
                Ok(0) // Success
            })
            .await
            .map_err(db_err)?;

        match validation_result {
            0 => {}
            1 => return Err(Error::InvalidArgument("Part ETag mismatch".to_string())),
            2 => return Err(Error::EntityTooSmall),
            _ => return Err(Error::NoSuchPart(0)),
        }

        // Generate final object ID and version_id
        let object_id = Ulid::new().to_string();
        let version_id = if is_versioned {
            object_id.clone()
        } else {
            "null".to_string()
        };

        let blob_path = self.blob_path(&object_id);

        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Concatenate parts into plaintext first
        let mut plaintext = Vec::new();
        let mut part_etags = Vec::new();

        for part in &parts {
            let part_path = self.part_path(&upload.upload_id, part.part_number);

            if !part_path.exists() {
                return Err(Error::NoSuchPart(part.part_number));
            }

            let part_data = fs::read(&part_path).await?;
            plaintext.extend_from_slice(&part_data);
            part_etags.push(part.etag.clone());
        }

        let total_size = plaintext.len() as u64;
        let etag = format_multipart_etag(&part_etags);

        // Apply encryption if configured
        let (data_to_write, encryption_algorithm, encryption_key_md5) =
            if let Some(ref alg) = upload_meta.encryption_algorithm {
                match alg.as_str() {
                    "AES256" => {
                        // SSE-S3: Encrypt with server key
                        let encrypted = self.encrypt_sse_s3(
                            &upload_meta.bucket,
                            &upload_meta.key,
                            &version_id,
                            &plaintext,
                        )?;
                        (encrypted, Some("AES256".to_string()), None::<String>)
                    }
                    "SSE-C" => {
                        return Err(Error::MissingSecurityHeader);
                    }
                    _ => (plaintext, None::<String>, None::<String>),
                }
            } else {
                (plaintext, None::<String>, None::<String>)
            };

        // Write final object
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&blob_path)
            .await?;

        file.write_all(&data_to_write).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        // Insert object with versioning support
        let bucket = upload_meta.bucket.clone();
        let key = upload_meta.key.clone();
        let etag_clone = etag.clone();
        let content_type = upload_meta.content_type;
        let content_encoding = upload_meta.content_encoding;
        let content_disposition = upload_meta.content_disposition;
        let cache_control = upload_meta.cache_control;
        let storage_class = upload_meta.storage_class;
        let user_metadata = upload_meta.user_metadata;
        let part_count = parts.len() as i64;
        let upload_id_del = upload.upload_id.clone();
        let version_id_clone = version_id.clone();

        self.db
            .call(move |conn| {
                if is_versioned {
                    // Mark all existing versions as not latest
                    conn.execute(
                        "UPDATE objects SET is_latest = 0 WHERE bucket = ?1 AND key = ?2",
                        params![&bucket, &key],
                    )?;
                } else {
                    // Delete existing object if any
                    conn.execute(
                        "DELETE FROM objects WHERE bucket = ?1 AND key = ?2",
                        params![&bucket, &key],
                    )?;
                }

                // Insert new object with all metadata and encryption info
                conn.execute(
                    r#"INSERT INTO objects (id, bucket, key, version_id, size, etag, content_type,
                       content_encoding, content_disposition, cache_control,
                       storage_class, user_metadata, is_multipart, part_count, is_latest,
                       encryption_algorithm, encryption_key_md5)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13, 1, ?14, ?15)"#,
                    params![
                        &object_id,
                        &bucket,
                        &key,
                        &version_id_clone,
                        total_size as i64,
                        &etag_clone,
                        &content_type,
                        &content_encoding,
                        &content_disposition,
                        &cache_control,
                        &storage_class,
                        &user_metadata,
                        part_count,
                        &encryption_algorithm,
                        &encryption_key_md5,
                    ],
                )?;

                // Delete parts from database
                conn.execute(
                    "DELETE FROM parts WHERE upload_id = ?1",
                    params![&upload_id_del],
                )?;

                // Delete upload record
                conn.execute(
                    "DELETE FROM multipart_uploads WHERE upload_id = ?1",
                    params![&upload_id_del],
                )?;

                Ok(())
            })
            .await
            .map_err(db_err)?;

        // Clean up part files
        let upload_dir = self.root.join("multipart").join(&upload.upload_id);
        let _ = fs::remove_dir_all(&upload_dir).await;

        debug!(
            "Completed multipart upload {} as {}/{}",
            upload.upload_id, upload.bucket, upload.key
        );

        // Return version_id only for versioned buckets
        let response_version_id = if is_versioned { Some(version_id) } else { None };

        Ok(PutObjectResponse {
            etag,
            version_id: response_version_id,
        })
    }

    #[instrument(skip(self))]
    async fn abort_multipart_upload(&self, upload: &MultipartUpload) -> Result<()> {
        let upload_id = upload.upload_id.clone();

        // Verify upload exists
        let exists = self
            .db
            .call({
                let upload_id = upload_id.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT 1 FROM multipart_uploads WHERE upload_id = ?1",
                            params![&upload_id],
                            |_| Ok(true),
                        )
                        .unwrap_or(false))
                }
            })
            .await
            .map_err(db_err)?;

        if !exists {
            return Err(Error::UploadNotFound(upload_id));
        }

        // Delete parts and upload from database
        let upload_id_del = upload.upload_id.clone();
        self.db
            .call(move |conn| {
                // Delete parts first (foreign key constraint)
                conn.execute(
                    "DELETE FROM parts WHERE upload_id = ?1",
                    params![&upload_id_del],
                )?;
                // Delete upload record
                conn.execute(
                    "DELETE FROM multipart_uploads WHERE upload_id = ?1",
                    params![&upload_id_del],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        // Clean up part files
        let upload_dir = self.root.join("multipart").join(&upload.upload_id);
        let _ = fs::remove_dir_all(&upload_dir).await;

        debug!("Aborted multipart upload {}", upload.upload_id);
        Ok(())
    }

    async fn list_parts(
        &self,
        upload: &MultipartUpload,
        opts: ListPartsOpts,
    ) -> Result<ListPartsResponse> {
        let upload_id = upload.upload_id.clone();
        let max_parts = opts.max_parts.unwrap_or(1000) as usize;
        let marker = opts.part_number_marker.unwrap_or(0) as i64;

        let parts = self
            .db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r#"SELECT upload_id, part_number, blob_id, size, etag, created_at
                       FROM parts WHERE upload_id = ?1 AND part_number > ?2
                       ORDER BY part_number"#,
                )?;

                let rows: Vec<PartRow> = stmt
                    .query_map(params![&upload_id, marker], |row| {
                        Ok(PartRow {
                            upload_id: row.get(0)?,
                            part_number: row.get(1)?,
                            blob_id: row.get(2)?,
                            size: row.get(3)?,
                            etag: row.get(4)?,
                            created_at: row.get(5)?,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(rows)
            })
            .await
            .map_err(db_err)?;

        let mut result_parts: Vec<PartInfo> = parts
            .into_iter()
            .map(|p| PartInfo {
                part_number: p.part_number as u16,
                etag: p.etag,
                size: p.size as u64,
                last_modified: Self::parse_datetime(&p.created_at),
            })
            .collect();

        let is_truncated = result_parts.len() > max_parts;
        if is_truncated {
            result_parts.truncate(max_parts);
        }

        let next_marker = if is_truncated {
            result_parts.last().map(|p| p.part_number)
        } else {
            None
        };

        Ok(ListPartsResponse {
            parts: result_parts,
            is_truncated,
            next_part_number_marker: next_marker,
        })
    }

    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        opts: ListUploadsOpts,
    ) -> Result<ListUploadsResponse> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let prefix = opts.prefix.unwrap_or_default();
        let max_uploads = opts.max_uploads.unwrap_or(1000) as usize;

        let uploads = self
            .db
            .call(move |conn| {
                let prefix_pattern = format!("{}%", prefix.replace('%', "\\%"));

                let mut stmt = conn.prepare(
                    r#"SELECT upload_id, bucket, key, content_type, content_encoding,
                       content_disposition, cache_control, storage_class,
                       user_metadata, encryption_algorithm, encryption_key_md5, initiated_at
                       FROM multipart_uploads WHERE bucket = ?1 AND key LIKE ?2
                       ORDER BY key, initiated_at"#,
                )?;

                let rows: Vec<UploadRow> = stmt
                    .query_map(params![&bucket, &prefix_pattern], |row| {
                        Ok(UploadRow {
                            upload_id: row.get(0)?,
                            bucket: row.get(1)?,
                            key: row.get(2)?,
                            content_type: row.get(3)?,
                            content_encoding: row.get(4)?,
                            content_disposition: row.get(5)?,
                            cache_control: row.get(6)?,
                            storage_class: row.get(7)?,
                            user_metadata: row.get(8)?,
                            encryption_algorithm: row.get(9)?,
                            encryption_key_md5: row.get(10)?,
                            initiated_at: row.get(11)?,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(rows)
            })
            .await
            .map_err(db_err)?;

        let mut result_uploads: Vec<MultipartUpload> = uploads
            .into_iter()
            .map(|u| MultipartUpload {
                upload_id: u.upload_id,
                bucket: u.bucket,
                key: u.key,
                initiated: Self::parse_datetime(&u.initiated_at),
            })
            .collect();

        let is_truncated = result_uploads.len() > max_uploads;
        if is_truncated {
            result_uploads.truncate(max_uploads);
        }

        Ok(ListUploadsResponse {
            uploads: result_uploads,
            common_prefixes: Vec::new(),
            is_truncated,
            next_key_marker: None,
            next_upload_id_marker: None,
        })
    }

    // === Versioning Operations ===

    #[instrument(skip(self))]
    async fn set_bucket_versioning(&self, bucket: &str, enabled: bool) -> Result<()> {
        let bucket_name = bucket.to_string();
        let status = if enabled {
            VersioningStatus::Enabled
        } else {
            VersioningStatus::Suspended
        };

        let updated = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    let rows = conn.execute(
                        "UPDATE buckets SET versioning_status = ?1 WHERE name = ?2",
                        params![status.as_str(), &bucket],
                    )?;
                    Ok(rows)
                }
            })
            .await
            .map_err(db_err)?;

        if updated == 0 {
            return Err(Error::BucketNotFound(bucket_name));
        }

        debug!(
            "Set bucket {} versioning to {}",
            bucket_name,
            status.as_str()
        );
        Ok(())
    }

    async fn get_bucket_versioning(&self, bucket: &str) -> Result<Option<bool>> {
        let bucket_name = bucket.to_string();

        let status = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    Ok(conn.query_row(
                        "SELECT versioning_status FROM buckets WHERE name = ?1",
                        params![&bucket],
                        |row| row.get::<_, String>(0),
                    )?)
                }
            })
            .await
            .map_err(|_| Error::BucketNotFound(bucket_name))?;

        Ok(match VersioningStatus::from_db_str(&status) {
            VersioningStatus::Unversioned => None,
            VersioningStatus::Enabled => Some(true),
            VersioningStatus::Suspended => Some(false),
        })
    }

    #[instrument(skip(self))]
    async fn delete_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<DeleteObjectResponse> {
        let bucket_name = bucket.to_string();
        let key_str = key.to_string();
        let version_id_str = version_id.map(|s| s.to_string());

        // Get bucket versioning status
        let versioning_status = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    Ok(conn
                        .query_row(
                            "SELECT versioning_status FROM buckets WHERE name = ?1",
                            params![&bucket],
                            |row| row.get::<_, String>(0),
                        )
                        .ok())
                }
            })
            .await
            .map_err(db_err)?
            .ok_or_else(|| Error::BucketNotFound(bucket_name.clone()))?;

        let is_versioned = versioning_status == "Enabled" || versioning_status == "Suspended";

        if self.object_delete_locked(bucket, key, version_id).await? {
            return Err(Error::ObjectLocked);
        }

        if let Some(vid) = version_id_str.clone() {
            // Delete specific version (hard delete)
            let object_id: Option<String> = self
                .db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    let vid = vid.clone();
                    move |conn| {
                        Ok(conn
                            .query_row(
                                "SELECT id FROM objects WHERE bucket = ?1 AND key = ?2 AND version_id = ?3",
                                params![&bucket, &key, &vid],
                                |row| row.get(0),
                            )
                            .ok())
                    }
                })
                .await
                .map_err(db_err)?;

            // Delete from database
            self.db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    let vid = vid.clone();
                    move |conn| {
                        conn.execute(
                            "DELETE FROM objects WHERE bucket = ?1 AND key = ?2 AND version_id = ?3",
                            params![&bucket, &key, &vid],
                        )?;
                        Ok(())
                    }
                })
                .await
                .map_err(db_err)?;

            // Delete blob file if it existed
            if let Some(id) = object_id {
                let blob_path = self.blob_path(&id);
                let _ = fs::remove_file(&blob_path).await;
            }

            // If we deleted the latest, mark the next most recent as latest
            self.db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    move |conn| {
                        // Find the most recent remaining version
                        if let Ok(id) = conn.query_row::<String, _, _>(
                            "SELECT id FROM objects WHERE bucket = ?1 AND key = ?2 ORDER BY created_at DESC LIMIT 1",
                            params![&bucket, &key],
                            |row| row.get(0),
                        ) {
                            conn.execute(
                                "UPDATE objects SET is_latest = 1 WHERE id = ?1",
                                params![&id],
                            )?;
                        }
                        Ok(())
                    }
                })
                .await
                .map_err(db_err)?;

            debug!(
                "Hard deleted object version: {}/{} v{}",
                bucket_name, key_str, vid
            );
            Ok(DeleteObjectResponse {
                delete_marker: false,
                version_id: Some(vid),
            })
        } else if is_versioned {
            // Create delete marker (soft delete)
            let delete_marker_id = Ulid::new().to_string();
            let version_id = delete_marker_id.clone();

            self.db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    let dm_id = delete_marker_id.clone();
                    let vid = version_id.clone();
                    move |conn| {
                        // Mark existing latest as not latest
                        conn.execute(
                            "UPDATE objects SET is_latest = 0 WHERE bucket = ?1 AND key = ?2 AND is_latest = 1",
                            params![&bucket, &key],
                        )?;

                        // Insert delete marker
                        conn.execute(
                            r#"INSERT INTO objects (id, bucket, key, version_id, size, etag, storage_class,
                               is_latest, is_delete_marker)
                               VALUES (?1, ?2, ?3, ?4, 0, '', 'STANDARD', 1, 1)"#,
                            params![&dm_id, &bucket, &key, &vid],
                        )?;

                        Ok(())
                    }
                })
                .await
                .map_err(db_err)?;

            debug!("Created delete marker for: {}/{}", bucket_name, key_str);
            Ok(DeleteObjectResponse {
                delete_marker: true,
                version_id: Some(version_id),
            })
        } else {
            // Unversioned bucket - hard delete all versions
            let object_ids: Vec<String> = self
                .db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    move |conn| {
                        let mut stmt =
                            conn.prepare("SELECT id FROM objects WHERE bucket = ?1 AND key = ?2")?;
                        let ids: Vec<String> = stmt
                            .query_map(params![&bucket, &key], |row| row.get(0))?
                            .filter_map(|r| r.ok())
                            .collect();
                        Ok(ids)
                    }
                })
                .await
                .map_err(db_err)?;

            // Delete from database
            self.db
                .call({
                    let bucket = bucket_name.clone();
                    let key = key_str.clone();
                    move |conn| {
                        conn.execute(
                            "DELETE FROM objects WHERE bucket = ?1 AND key = ?2",
                            params![&bucket, &key],
                        )?;
                        Ok(())
                    }
                })
                .await
                .map_err(db_err)?;

            // Delete blob files
            for id in object_ids {
                let blob_path = self.blob_path(&id);
                let _ = fs::remove_file(&blob_path).await;
            }

            debug!("Deleted object: {}/{}", bucket_name, key_str);
            Ok(DeleteObjectResponse {
                delete_marker: false,
                version_id: None,
            })
        }
    }

    async fn list_object_versions(
        &self,
        bucket: &str,
        opts: ListVersionsOpts,
    ) -> Result<ListVersionsResponse> {
        // Verify bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let prefix = opts.prefix.clone().unwrap_or_default();
        let delimiter = opts.delimiter.clone();
        let max_keys = opts.max_keys.unwrap_or(1000) as usize;
        let key_marker = opts.key_marker.clone();
        let version_id_marker = opts.version_id_marker.clone();

        let objects = self
            .db
            .call({
                let bucket = bucket.clone();
                let prefix = prefix.clone();
                let key_marker = key_marker.clone();
                move |conn| {
                    let prefix_pattern = format!("{}%", prefix.replace('%', "\\%"));

                    let sql = if key_marker.is_some() {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type,
                           storage_class, is_latest, is_delete_marker, last_modified
                           FROM objects WHERE bucket = ?1 AND key LIKE ?2 AND key >= ?3
                           ORDER BY key, last_modified DESC"#
                    } else {
                        r#"SELECT id, bucket, key, version_id, size, etag, content_type,
                           storage_class, is_latest, is_delete_marker, last_modified
                           FROM objects WHERE bucket = ?1 AND key LIKE ?2
                           ORDER BY key, last_modified DESC"#
                    };

                    let mut stmt = conn.prepare(sql)?;

                    let rows: Vec<(
                        String,
                        String,
                        String,
                        i64,
                        String,
                        Option<String>,
                        String,
                        bool,
                        bool,
                        String,
                    )> = if let Some(marker) = key_marker {
                        stmt.query_map(params![&bucket, &prefix_pattern, &marker], |row| {
                            Ok((
                                row.get::<_, String>(2)?,         // key
                                row.get::<_, String>(3)?,         // version_id
                                row.get::<_, String>(5)?,         // etag
                                row.get::<_, i64>(4)?,            // size
                                row.get::<_, String>(7)?,         // storage_class
                                row.get::<_, Option<String>>(6)?, // content_type (unused here)
                                row.get::<_, String>(10)?,        // last_modified
                                row.get::<_, i64>(8)? != 0,       // is_latest
                                row.get::<_, i64>(9)? != 0,       // is_delete_marker
                                row.get::<_, String>(0)?,         // id (unused)
                            ))
                        })?
                        .filter_map(|r| r.ok())
                        .collect()
                    } else {
                        stmt.query_map(params![&bucket, &prefix_pattern], |row| {
                            Ok((
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(5)?,
                                row.get::<_, i64>(4)?,
                                row.get::<_, String>(7)?,
                                row.get::<_, Option<String>>(6)?,
                                row.get::<_, String>(10)?,
                                row.get::<_, i64>(8)? != 0,
                                row.get::<_, i64>(9)? != 0,
                                row.get::<_, String>(0)?,
                            ))
                        })?
                        .filter_map(|r| r.ok())
                        .collect()
                    };

                    Ok(rows)
                }
            })
            .await
            .map_err(db_err)?;

        let mut versions = Vec::new();
        let mut delete_markers = Vec::new();
        let mut common_prefixes = std::collections::BTreeSet::new();

        // Filter by version_id_marker if provided
        let mut past_marker = version_id_marker.is_none();

        for (
            key,
            version_id,
            etag,
            size,
            storage_class,
            _content_type,
            last_modified,
            is_latest,
            is_delete_marker,
            _id,
        ) in objects
        {
            // Handle version_id_marker pagination
            if !past_marker {
                if let Some(ref vid_marker) = version_id_marker {
                    if &version_id == vid_marker {
                        past_marker = true;
                        continue; // Skip the marker itself
                    }
                    continue;
                }
            }

            // Handle delimiter for common prefixes
            if let Some(ref delim) = delimiter {
                if !delim.is_empty() {
                    let suffix = &key[prefix.len()..];
                    if let Some(pos) = suffix.find(delim.as_str()) {
                        let common_prefix = format!("{}{}{}", prefix, &suffix[..pos], delim);
                        common_prefixes.insert(common_prefix);
                        continue;
                    }
                }
            }

            let obj_version = ObjectVersion {
                key,
                version_id,
                is_latest,
                is_delete_marker,
                last_modified: Self::parse_datetime(&last_modified),
                etag: if is_delete_marker { None } else { Some(etag) },
                size: if is_delete_marker {
                    None
                } else {
                    Some(size as u64)
                },
                storage_class: if is_delete_marker {
                    None
                } else {
                    Some(Self::parse_storage_class(&storage_class))
                },
            };

            if is_delete_marker {
                delete_markers.push(obj_version);
            } else {
                versions.push(obj_version);
            }

            if versions.len() + delete_markers.len() > max_keys {
                break;
            }
        }

        let total = versions.len() + delete_markers.len();
        let is_truncated = total > max_keys;

        // Truncate if needed
        if is_truncated {
            let to_remove = total - max_keys;
            // Remove from whichever list has more items
            if delete_markers.len() >= to_remove {
                delete_markers.truncate(delete_markers.len() - to_remove);
            } else {
                let remaining = to_remove - delete_markers.len();
                delete_markers.clear();
                versions.truncate(versions.len() - remaining);
            }
        }

        // Get markers for pagination
        let (next_key_marker, next_version_id_marker) = if is_truncated {
            // Find the last item
            let last_version = versions.last();
            let last_dm = delete_markers.last();
            match (last_version, last_dm) {
                (Some(v), Some(dm)) => {
                    if v.last_modified >= dm.last_modified {
                        (Some(v.key.clone()), Some(v.version_id.clone()))
                    } else {
                        (Some(dm.key.clone()), Some(dm.version_id.clone()))
                    }
                }
                (Some(v), None) => (Some(v.key.clone()), Some(v.version_id.clone())),
                (None, Some(dm)) => (Some(dm.key.clone()), Some(dm.version_id.clone())),
                (None, None) => (None, None),
            }
        } else {
            (None, None)
        };

        Ok(ListVersionsResponse {
            versions,
            delete_markers,
            common_prefixes: common_prefixes.into_iter().collect(),
            is_truncated,
            next_key_marker,
            next_version_id_marker,
        })
    }

    // === CORS Operations ===

    async fn get_bucket_cors(&self, bucket: &str) -> Result<Option<CorsConfiguration>> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let result = self
            .db
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT cors_config FROM bucket_cors WHERE bucket = ?1")?;
                let cors_json: Option<String> = stmt
                    .query_row(params![&bucket], |row| row.get(0))
                    .optional()?;
                Ok(cors_json)
            })
            .await
            .map_err(db_err)?;

        match result {
            Some(json) => {
                let config: CorsConfiguration =
                    serde_json::from_str(&json).map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    async fn put_bucket_cors(&self, bucket: &str, config: CorsConfiguration) -> Result<()> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        // Validate CORS rules
        for rule in &config.rules {
            if rule.allowed_origins.is_empty() {
                return Err(Error::InvalidArgument(
                    "CORS rule must have at least one allowed origin".to_string(),
                ));
            }
            if rule.allowed_methods.is_empty() {
                return Err(Error::InvalidArgument(
                    "CORS rule must have at least one allowed method".to_string(),
                ));
            }
        }

        let config_json =
            serde_json::to_string(&config).map_err(|e| Error::Serialization(e.to_string()))?;

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO bucket_cors (bucket, cors_config) VALUES (?1, ?2)
                     ON CONFLICT(bucket) DO UPDATE SET cors_config = excluded.cors_config",
                    params![&bucket, &config_json],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    async fn delete_bucket_cors(&self, bucket: &str) -> Result<()> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM bucket_cors WHERE bucket = ?1",
                    params![&bucket],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    // === Object Lock Operations ===

    async fn get_object_lock_configuration(
        &self,
        bucket: &str,
    ) -> Result<Option<ObjectLockConfiguration>> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let result = self
            .db
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT lock_config FROM bucket_object_lock WHERE bucket = ?1")?;
                let config_json: Option<String> = stmt
                    .query_row(params![&bucket], |row| row.get(0))
                    .optional()?;
                Ok(config_json)
            })
            .await
            .map_err(db_err)?;

        match result {
            Some(json) => {
                let config: ObjectLockConfiguration =
                    serde_json::from_str(&json).map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    async fn put_object_lock_configuration(
        &self,
        bucket: &str,
        config: ObjectLockConfiguration,
    ) -> Result<()> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        // Check if bucket has object lock enabled
        let bucket_name = bucket.to_string();
        let lock_enabled = self
            .db
            .call({
                let bucket = bucket_name.clone();
                move |conn| {
                    let enabled: i64 = conn.query_row(
                        "SELECT object_lock_enabled FROM buckets WHERE name = ?1",
                        params![&bucket],
                        |row| row.get(0),
                    )?;
                    Ok(enabled != 0)
                }
            })
            .await
            .map_err(db_err)?;

        if !lock_enabled && config.enabled {
            return Err(Error::InvalidArgument(
                "Object lock can only be enabled when creating a bucket".to_string(),
            ));
        }

        let config_json =
            serde_json::to_string(&config).map_err(|e| Error::Serialization(e.to_string()))?;

        let bucket = bucket_name;
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO bucket_object_lock (bucket, lock_config) VALUES (?1, ?2)
                     ON CONFLICT(bucket) DO UPDATE SET lock_config = excluded.lock_config",
                    params![&bucket, &config_json],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    async fn get_object_retention(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<Option<ObjectRetention>> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let key = key.to_string();
        let version_id = version_id.map(|v| v.to_string());

        let result = self
            .db
            .call(move |conn| {
                let (mode, until): (Option<String>, Option<String>) = if let Some(vid) = version_id
                {
                    conn.query_row(
                        "SELECT retention_mode, retain_until_date FROM objects
                         WHERE bucket = ?1 AND key = ?2 AND version_id = ?3",
                        params![&bucket, &key, &vid],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?
                    .unwrap_or((None, None))
                } else {
                    conn.query_row(
                        "SELECT retention_mode, retain_until_date FROM objects
                         WHERE bucket = ?1 AND key = ?2 AND is_latest = 1",
                        params![&bucket, &key],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?
                    .unwrap_or((None, None))
                };
                Ok((mode, until))
            })
            .await
            .map_err(db_err)?;

        match result {
            (Some(mode), Some(until)) => {
                let retention_mode: RetentionMode = mode
                    .parse()
                    .map_err(|e: String| Error::InvalidArgument(e))?;
                let retain_until = Self::parse_datetime(&until);
                Ok(Some(ObjectRetention {
                    mode: retention_mode,
                    retain_until_date: retain_until,
                }))
            }
            _ => Ok(None),
        }
    }

    async fn put_object_retention(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
        retention: ObjectRetention,
        bypass_governance: bool,
    ) -> Result<()> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let key = key.to_string();
        let version_id = version_id.map(|v| v.to_string());
        let mode = retention.mode.to_string();
        let until = retention.retain_until_date.to_rfc3339();

        // First, check if we can modify retention
        let can_modify = self
            .db
            .call({
                let bucket = bucket.clone();
                let key = key.clone();
                let version_id = version_id.clone();
                move |conn| {
                    let (current_mode, current_until): (Option<String>, Option<String>) =
                        if let Some(ref vid) = version_id {
                            conn.query_row(
                                "SELECT retention_mode, retain_until_date FROM objects
                                 WHERE bucket = ?1 AND key = ?2 AND version_id = ?3",
                                params![&bucket, &key, &vid],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .optional()?
                            .unwrap_or((None, None))
                        } else {
                            conn.query_row(
                                "SELECT retention_mode, retain_until_date FROM objects
                                 WHERE bucket = ?1 AND key = ?2 AND is_latest = 1",
                                params![&bucket, &key],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .optional()?
                            .unwrap_or((None, None))
                        };

                    // Check if we can modify retention
                    if let (Some(cm), Some(_cu)) = (&current_mode, &current_until) {
                        if cm == "COMPLIANCE" {
                            return Ok(false); // Cannot modify compliance retention
                        }
                        if cm == "GOVERNANCE" && !bypass_governance {
                            return Ok(false); // Need bypass for governance
                        }
                    }
                    Ok(true)
                }
            })
            .await
            .map_err(db_err)?;

        if !can_modify {
            return Err(Error::AccessDenied);
        }

        // Update retention
        self.db
            .call(move |conn| {
                if let Some(vid) = version_id {
                    conn.execute(
                        "UPDATE objects SET retention_mode = ?1, retain_until_date = ?2
                         WHERE bucket = ?3 AND key = ?4 AND version_id = ?5",
                        params![&mode, &until, &bucket, &key, &vid],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE objects SET retention_mode = ?1, retain_until_date = ?2
                         WHERE bucket = ?3 AND key = ?4 AND is_latest = 1",
                        params![&mode, &until, &bucket, &key],
                    )?;
                }
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    async fn get_object_legal_hold(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<LegalHoldStatus> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let key = key.to_string();
        let version_id = version_id.map(|v| v.to_string());

        let hold = self
            .db
            .call(move |conn| {
                let legal_hold: i64 = if let Some(vid) = version_id {
                    conn.query_row(
                        "SELECT legal_hold FROM objects
                         WHERE bucket = ?1 AND key = ?2 AND version_id = ?3",
                        params![&bucket, &key, &vid],
                        |row| row.get(0),
                    )?
                } else {
                    conn.query_row(
                        "SELECT legal_hold FROM objects
                         WHERE bucket = ?1 AND key = ?2 AND is_latest = 1",
                        params![&bucket, &key],
                        |row| row.get(0),
                    )?
                };
                Ok(legal_hold != 0)
            })
            .await
            .map_err(db_err)?;

        Ok(if hold {
            LegalHoldStatus::On
        } else {
            LegalHoldStatus::Off
        })
    }

    async fn put_object_legal_hold(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
        status: LegalHoldStatus,
    ) -> Result<()> {
        // Check bucket exists
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let key = key.to_string();
        let version_id = version_id.map(|v| v.to_string());
        let hold = match status {
            LegalHoldStatus::On => 1,
            LegalHoldStatus::Off => 0,
        };

        self.db
            .call(move |conn| {
                if let Some(vid) = version_id {
                    conn.execute(
                        "UPDATE objects SET legal_hold = ?1
                         WHERE bucket = ?2 AND key = ?3 AND version_id = ?4",
                        params![hold, &bucket, &key, &vid],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE objects SET legal_hold = ?1
                         WHERE bucket = ?2 AND key = ?3 AND is_latest = 1",
                        params![hold, &bucket, &key],
                    )?;
                }
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    // === Lifecycle Operations ===

    async fn get_bucket_lifecycle(&self, bucket: &str) -> Result<Option<LifecycleConfiguration>> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let result = self
            .db
            .call(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT lifecycle_config FROM bucket_lifecycle WHERE bucket = ?1")?;
                let config_json: Option<String> = stmt
                    .query_row(params![&bucket], |row| row.get(0))
                    .optional()?;
                Ok(config_json)
            })
            .await
            .map_err(db_err)?;

        match result {
            Some(json) => {
                let config: LifecycleConfiguration =
                    serde_json::from_str(&json).map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    async fn put_bucket_lifecycle(
        &self,
        bucket: &str,
        config: LifecycleConfiguration,
    ) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        // Validate rules have unique IDs
        let mut ids = std::collections::HashSet::new();
        for rule in &config.rules {
            if !ids.insert(&rule.id) {
                return Err(Error::InvalidArgument(format!(
                    "Duplicate lifecycle rule ID: {}",
                    rule.id
                )));
            }
        }

        let config_json =
            serde_json::to_string(&config).map_err(|e| Error::Serialization(e.to_string()))?;

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO bucket_lifecycle (bucket, lifecycle_config) VALUES (?1, ?2)
                     ON CONFLICT(bucket) DO UPDATE SET lifecycle_config = excluded.lifecycle_config",
                    params![&bucket, &config_json],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    async fn delete_bucket_lifecycle(&self, bucket: &str) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM bucket_lifecycle WHERE bucket = ?1",
                    params![&bucket],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    // === Event Notification Operations ===

    async fn get_bucket_notification(
        &self,
        bucket: &str,
    ) -> Result<Option<NotificationConfiguration>> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let result = self
            .db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT notification_config FROM bucket_notification WHERE bucket = ?1",
                )?;
                let config_json: Option<String> = stmt
                    .query_row(params![&bucket], |row| row.get(0))
                    .optional()?;
                Ok(config_json)
            })
            .await
            .map_err(db_err)?;

        match result {
            Some(json) => {
                let config: NotificationConfiguration =
                    serde_json::from_str(&json).map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    async fn put_bucket_notification(
        &self,
        bucket: &str,
        config: NotificationConfiguration,
    ) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        // Validate rules have unique IDs
        let mut ids = std::collections::HashSet::new();
        for rule in &config.rules {
            if !ids.insert(&rule.id) {
                return Err(Error::InvalidArgument(format!(
                    "Duplicate notification rule ID: {}",
                    rule.id
                )));
            }
        }

        let config_json =
            serde_json::to_string(&config).map_err(|e| Error::Serialization(e.to_string()))?;

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO bucket_notification (bucket, notification_config) VALUES (?1, ?2)
                     ON CONFLICT(bucket) DO UPDATE SET notification_config = excluded.notification_config",
                    params![&bucket, &config_json],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    // === Bucket Tagging Operations ===

    async fn get_bucket_tagging(&self, bucket: &str) -> Result<Option<TaggingConfiguration>> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        let result = self
            .db
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT tags_config FROM bucket_tags WHERE bucket = ?1")?;
                let tags_json: Option<String> = stmt
                    .query_row(params![&bucket], |row| row.get(0))
                    .optional()?;
                Ok(tags_json)
            })
            .await
            .map_err(db_err)?;

        match result {
            Some(json) => {
                let config: TaggingConfiguration =
                    serde_json::from_str(&json).map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    async fn put_bucket_tagging(&self, bucket: &str, config: TaggingConfiguration) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        // Validate: max 50 tags, key length 1-128, value length 0-256
        if config.tags.len() > 50 {
            return Err(Error::InvalidArgument(
                "Maximum 50 tags allowed".to_string(),
            ));
        }
        for tag in &config.tags {
            if tag.key.is_empty() || tag.key.len() > 128 {
                return Err(Error::InvalidArgument(
                    "Tag key must be 1-128 characters".to_string(),
                ));
            }
            if tag.value.len() > 256 {
                return Err(Error::InvalidArgument(
                    "Tag value must be at most 256 characters".to_string(),
                ));
            }
        }

        let config_json =
            serde_json::to_string(&config).map_err(|e| Error::Serialization(e.to_string()))?;

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO bucket_tags (bucket, tags_config) VALUES (?1, ?2)
                     ON CONFLICT(bucket) DO UPDATE SET tags_config = excluded.tags_config",
                    params![&bucket, &config_json],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    async fn delete_bucket_tagging(&self, bucket: &str) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::BucketNotFound(bucket.to_string()));
        }

        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM bucket_tags WHERE bucket = ?1",
                    params![&bucket],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        Ok(())
    }

    // === Audit Logging Operations ===

    #[instrument(skip(self, entry))]
    async fn log_audit_event(&self, entry: AuditLogEntry) -> Result<()> {
        let id = entry.id;
        let timestamp = entry.timestamp.to_rfc3339();
        let operation = entry.operation;
        let bucket = entry.bucket;
        let key = entry.key;
        let principal = entry.principal;
        let source_ip = entry.source_ip;
        let status_code = entry.status_code as i32;
        let error_code = entry.error_code;
        let duration_ms = entry.duration_ms.map(|d| d as i64);
        let bytes_sent = entry.bytes_sent.map(|b| b as i64);
        let request_id = entry.request_id;

        // Clone for debug log after the move
        let id_for_log = id.clone();
        let op_for_log = operation.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO audit_log (id, timestamp, operation, bucket, key, principal, source_ip, status_code, error_code, duration_ms, bytes_sent, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        &id,
                        &timestamp,
                        &operation,
                        &bucket,
                        &key,
                        &principal,
                        &source_ip,
                        status_code,
                        &error_code,
                        duration_ms,
                        bytes_sent,
                        &request_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)?;

        debug!("Logged audit event: {} {}", op_for_log, id_for_log);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn query_audit_log(&self, opts: AuditQueryOpts) -> Result<Vec<AuditLogEntry>> {
        let limit = opts.limit.unwrap_or(100) as i64;
        let offset = opts.offset.unwrap_or(0) as i64;

        self.db
            .call(move |conn| {
                // Build WHERE clause dynamically
                let mut conditions = Vec::new();
                let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

                if let Some(ref bucket) = opts.bucket {
                    conditions.push("bucket = ?");
                    params_vec.push(Box::new(bucket.clone()));
                }

                if let Some(ref key_prefix) = opts.key_prefix {
                    conditions.push("key LIKE ?");
                    params_vec.push(Box::new(format!("{}%", key_prefix)));
                }

                if let Some(ref operation) = opts.operation {
                    conditions.push("operation = ?");
                    params_vec.push(Box::new(operation.clone()));
                }

                if let Some(ref principal) = opts.principal {
                    conditions.push("principal = ?");
                    params_vec.push(Box::new(principal.clone()));
                }

                if let Some(ref start_time) = opts.start_time {
                    conditions.push("timestamp >= ?");
                    params_vec.push(Box::new(start_time.to_rfc3339()));
                }

                if let Some(ref end_time) = opts.end_time {
                    conditions.push("timestamp <= ?");
                    params_vec.push(Box::new(end_time.to_rfc3339()));
                }

                let where_clause = if conditions.is_empty() {
                    String::new()
                } else {
                    format!("WHERE {}", conditions.join(" AND "))
                };

                let sql = format!(
                    "SELECT id, timestamp, operation, bucket, key, principal, source_ip, status_code, error_code, duration_ms, bytes_sent, request_id
                     FROM audit_log
                     {}
                     ORDER BY timestamp DESC
                     LIMIT ? OFFSET ?",
                    where_clause
                );

                params_vec.push(Box::new(limit));
                params_vec.push(Box::new(offset));

                let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params_refs.as_slice(), |row| {
                    let timestamp_str: String = row.get(1)?;
                    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());

                    Ok(AuditLogEntry {
                        id: row.get(0)?,
                        timestamp,
                        operation: row.get(2)?,
                        bucket: row.get(3)?,
                        key: row.get(4)?,
                        principal: row.get(5)?,
                        source_ip: row.get(6)?,
                        status_code: row.get::<_, i32>(7)? as u16,
                        error_code: row.get(8)?,
                        duration_ms: row.get::<_, Option<i64>>(9)?.map(|d| d as u64),
                        bytes_sent: row.get::<_, Option<i64>>(10)?.map(|b| b as u64),
                        request_id: row.get(11)?,
                    })
                })?;

                let mut entries = Vec::new();
                for row in rows {
                    entries.push(row?);
                }
                Ok(entries)
            })
            .await
            .map_err(db_err)
    }
}

// === Cleanup Operations (not part of ObjectStore trait) ===

impl LocalFsStore {
    /// Find stale multipart uploads older than the cutoff time.
    pub async fn find_stale_multipart_uploads(
        &self,
        cutoff: &str,
    ) -> Result<Vec<crate::cleanup::StaleUpload>> {
        let cutoff = Self::parse_datetime(cutoff)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT upload_id, bucket, key, initiated_at
                     FROM multipart_uploads
                     WHERE initiated_at < ?1",
                )?;

                let rows = stmt.query_map([&cutoff], |row| {
                    Ok(crate::cleanup::StaleUpload {
                        upload_id: row.get(0)?,
                        bucket: row.get(1)?,
                        key: row.get(2)?,
                        initiated_at: row.get(3)?,
                    })
                })?;

                let mut uploads = Vec::new();
                for row in rows {
                    uploads.push(row?);
                }
                Ok(uploads)
            })
            .await
            .map_err(db_err)
    }

    /// Abort a stale multipart upload and clean up its parts.
    pub async fn abort_stale_multipart(&self, upload_id: &str) -> Result<()> {
        let upload_id = upload_id.to_string();
        let upload_id_clone = upload_id.clone();

        // Delete multipart part files from multipart directory.
        let upload_dir = self.root.join("multipart").join(&upload_id);
        if let Err(e) = tokio::fs::remove_dir_all(&upload_dir).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Failed to remove multipart dir {:?}: {}", upload_dir, e);
            }
        }

        // Delete the upload and parts from the database
        self.db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM parts WHERE upload_id = ?1",
                    rusqlite::params![upload_id_clone.clone()],
                )?;
                conn.execute(
                    "DELETE FROM multipart_uploads WHERE upload_id = ?1",
                    rusqlite::params![upload_id_clone],
                )?;
                Ok(())
            })
            .await
            .map_err(db_err)
    }
}
