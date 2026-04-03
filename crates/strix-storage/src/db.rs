//! SQLite database schema and operations.

use rusqlite::Connection;

/// Versioning status for a bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VersioningStatus {
    /// Versioning has never been enabled (default).
    #[default]
    Unversioned,
    /// Versioning is enabled.
    Enabled,
    /// Versioning was enabled but is now suspended.
    Suspended,
}

impl VersioningStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VersioningStatus::Unversioned => "Unversioned",
            VersioningStatus::Enabled => "Enabled",
            VersioningStatus::Suspended => "Suspended",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "Enabled" => VersioningStatus::Enabled,
            "Suspended" => VersioningStatus::Suspended,
            _ => VersioningStatus::Unversioned,
        }
    }
}

/// Initialize the database schema.
/// Returns rusqlite::Result so it can be used in tokio-rusqlite closures.
pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    // Enable WAL mode for better concurrency and durability
    conn.execute_batch(
        r#"
        -- Enable WAL mode for better write concurrency and crash recovery
        PRAGMA journal_mode = WAL;

        -- Set busy timeout to 5 seconds to handle concurrent access
        PRAGMA busy_timeout = 5000;

        -- Enable foreign key enforcement
        PRAGMA foreign_keys = ON;

        -- Synchronous mode for durability (NORMAL is safe with WAL)
        PRAGMA synchronous = NORMAL;
        "#,
    )?;

    conn.execute_batch(
        r#"
        -- Buckets table with versioning and object lock support
        CREATE TABLE IF NOT EXISTS buckets (
            name TEXT PRIMARY KEY NOT NULL,
            region TEXT,
            versioning_status TEXT NOT NULL DEFAULT 'Unversioned',
            object_lock_enabled INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Objects table with versioning and encryption support
        CREATE TABLE IF NOT EXISTS objects (
            id TEXT PRIMARY KEY NOT NULL,           -- ULID (also serves as version_id)
            bucket TEXT NOT NULL,
            key TEXT NOT NULL,
            version_id TEXT NOT NULL,               -- Version identifier (same as id, or 'null' for unversioned)
            size INTEGER NOT NULL,
            etag TEXT NOT NULL,
            content_type TEXT,
            content_encoding TEXT,
            content_disposition TEXT,
            cache_control TEXT,
            storage_class TEXT NOT NULL DEFAULT 'STANDARD',
            user_metadata TEXT,                     -- JSON
            is_multipart INTEGER NOT NULL DEFAULT 0,
            part_count INTEGER,
            is_latest INTEGER NOT NULL DEFAULT 1,   -- 1 if this is the latest version
            is_delete_marker INTEGER NOT NULL DEFAULT 0, -- 1 if this is a delete marker
            encryption_algorithm TEXT,              -- 'AES256' for SSE-S3, 'SSE-C' for customer keys
            encryption_key_md5 TEXT,                -- For SSE-C: MD5 of customer key (base64)
            encryption_nonce TEXT,                  -- Base64-encoded nonce for decryption
            retention_mode TEXT,                    -- 'GOVERNANCE' or 'COMPLIANCE'
            retain_until_date TEXT,                 -- ISO 8601 datetime
            legal_hold INTEGER NOT NULL DEFAULT 0,  -- 1 if legal hold is ON
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_modified TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Index for listing objects (latest versions)
        CREATE INDEX IF NOT EXISTS idx_objects_bucket_key ON objects(bucket, key);
        CREATE INDEX IF NOT EXISTS idx_objects_latest ON objects(bucket, key, is_latest) WHERE is_latest = 1;

        -- Multipart uploads table
        CREATE TABLE IF NOT EXISTS multipart_uploads (
            upload_id TEXT PRIMARY KEY NOT NULL,
            bucket TEXT NOT NULL,
            key TEXT NOT NULL,
            content_type TEXT,
            content_encoding TEXT,
            content_disposition TEXT,
            cache_control TEXT,
            storage_class TEXT NOT NULL DEFAULT 'STANDARD',
            user_metadata TEXT,                     -- JSON
            encryption_algorithm TEXT,              -- 'AES256' for SSE-S3, 'SSE-C' for customer keys
            encryption_key_md5 TEXT,                -- For SSE-C: MD5 of customer key (base64)
            initiated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Parts table
        CREATE TABLE IF NOT EXISTS parts (
            upload_id TEXT NOT NULL,
            part_number INTEGER NOT NULL,
            blob_id TEXT NOT NULL,                  -- ULID for blob file
            size INTEGER NOT NULL,
            etag TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (upload_id, part_number),
            FOREIGN KEY (upload_id) REFERENCES multipart_uploads(upload_id) ON DELETE CASCADE
        );

        -- Access keys table
        CREATE TABLE IF NOT EXISTS access_keys (
            access_key TEXT PRIMARY KEY NOT NULL,
            secret_key TEXT NOT NULL,
            user_name TEXT,
            is_root INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Policies table
        CREATE TABLE IF NOT EXISTS policies (
            name TEXT PRIMARY KEY NOT NULL,
            policy_document TEXT NOT NULL,          -- JSON
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Bucket policies (associating policies with buckets)
        CREATE TABLE IF NOT EXISTS bucket_policies (
            bucket TEXT NOT NULL,
            policy_name TEXT NOT NULL,
            PRIMARY KEY (bucket, policy_name),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE,
            FOREIGN KEY (policy_name) REFERENCES policies(name) ON DELETE CASCADE
        );

        -- CORS configuration table
        CREATE TABLE IF NOT EXISTS bucket_cors (
            bucket TEXT PRIMARY KEY NOT NULL,
            cors_config TEXT NOT NULL,              -- JSON CorsConfiguration
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Object lock configuration table
        CREATE TABLE IF NOT EXISTS bucket_object_lock (
            bucket TEXT PRIMARY KEY NOT NULL,
            lock_config TEXT NOT NULL,              -- JSON ObjectLockConfiguration
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Lifecycle configuration table
        CREATE TABLE IF NOT EXISTS bucket_lifecycle (
            bucket TEXT PRIMARY KEY NOT NULL,
            lifecycle_config TEXT NOT NULL,         -- JSON LifecycleConfiguration
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Event notification configuration table
        CREATE TABLE IF NOT EXISTS bucket_notification (
            bucket TEXT PRIMARY KEY NOT NULL,
            notification_config TEXT NOT NULL,      -- JSON NotificationConfiguration
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
        );

        -- Audit log table
        CREATE TABLE IF NOT EXISTS audit_log (
            id TEXT PRIMARY KEY NOT NULL,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            operation TEXT NOT NULL,
            bucket TEXT,
            key TEXT,
            principal TEXT,
            source_ip TEXT,
            status_code INTEGER NOT NULL,
            error_code TEXT,
            duration_ms INTEGER,
            bytes_sent INTEGER,
            request_id TEXT NOT NULL
        );

        -- Index for audit log queries
        CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_log_bucket ON audit_log(bucket);

        -- Enable foreign keys
        PRAGMA foreign_keys = ON;
        "#,
    )?;

    Ok(())
}

/// Row type for bucket queries.
#[derive(Debug, Clone)]
pub struct BucketRow {
    pub name: String,
    pub tenant_slug: Option<String>,
    pub region: Option<String>,
    pub versioning_status: VersioningStatus,
    pub object_lock_enabled: bool,
    pub created_at: String,
}

/// Row type for tenant queries.
#[derive(Debug, Clone)]
pub struct TenantRow {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner: String,
    pub notes: Option<String>,
    pub created_at: String,
}

/// Row type for object queries.
#[derive(Debug, Clone)]
pub struct ObjectRow {
    pub id: String,
    pub bucket: String,
    pub key: String,
    pub version_id: String,
    pub size: i64,
    pub etag: String,
    pub content_type: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub cache_control: Option<String>,
    pub storage_class: String,
    pub user_metadata: Option<String>,
    pub is_multipart: bool,
    pub part_count: Option<i64>,
    pub is_latest: bool,
    pub is_delete_marker: bool,
    pub encryption_algorithm: Option<String>,
    pub encryption_key_md5: Option<String>,
    pub encryption_nonce: Option<String>,
    pub retention_mode: Option<String>,
    pub retain_until_date: Option<String>,
    pub legal_hold: bool,
    pub created_at: String,
    pub last_modified: String,
}

/// Row type for multipart upload queries.
#[derive(Debug, Clone)]
pub struct UploadRow {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub content_type: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub cache_control: Option<String>,
    pub storage_class: String,
    pub user_metadata: Option<String>,
    pub encryption_algorithm: Option<String>,
    pub encryption_key_md5: Option<String>,
    pub initiated_at: String,
}

/// Row type for part queries.
#[derive(Debug, Clone)]
pub struct PartRow {
    pub upload_id: String,
    pub part_number: i64,
    pub blob_id: String,
    pub size: i64,
    pub etag: String,
    pub created_at: String,
}

/// Row type for access key queries.
#[derive(Debug, Clone)]
pub struct AccessKeyRow {
    pub access_key: String,
    pub secret_key: String,
    pub user_name: Option<String>,
    pub is_root: bool,
    pub enabled: bool,
    pub created_at: String,
}
