//! Core data types for Strix.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

use crate::Result;

/// Information about a bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    #[serde(default)]
    pub tenant_slug: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Versioning status: None (never enabled), Some(true) (enabled), Some(false) (suspended)
    #[serde(default)]
    pub versioning_enabled: Option<bool>,
    /// Whether object locking is enabled.
    #[serde(default)]
    pub object_locking_enabled: bool,
}

/// Information about an object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub content_type: Option<String>,
    pub last_modified: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    pub storage_class: StorageClass,
    /// Version ID (None for unversioned buckets, "null" for objects before versioning was enabled)
    #[serde(default)]
    pub version_id: Option<String>,
    /// Whether this is the latest version
    #[serde(default = "default_is_latest")]
    pub is_latest: bool,
    /// Whether this is a delete marker (tombstone for deleted objects in versioned buckets)
    #[serde(default)]
    pub is_delete_marker: bool,
    /// Server-side encryption information (None if not encrypted)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<EncryptionInfo>,
}

fn default_is_latest() -> bool {
    true
}

/// Storage class for objects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StorageClass {
    #[default]
    Standard,
    ReducedRedundancy,
    Glacier,
    DeepArchive,
}

/// Server-side encryption algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerSideEncryption {
    /// SSE-S3: Server-managed encryption keys (AES-256)
    #[serde(rename = "AES256")]
    Aes256,
    /// SSE-C: Customer-provided encryption keys
    #[serde(rename = "aws:kms")]
    SseC,
}

impl std::fmt::Display for ServerSideEncryption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerSideEncryption::Aes256 => write!(f, "AES256"),
            ServerSideEncryption::SseC => write!(f, "SSE-C"),
        }
    }
}

/// Encryption information for an object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    /// The encryption algorithm used
    pub algorithm: ServerSideEncryption,
    /// For SSE-C: MD5 hash of the customer-provided key (for verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sse_customer_key_md5: Option<String>,
}

impl std::fmt::Display for StorageClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageClass::Standard => write!(f, "STANDARD"),
            StorageClass::ReducedRedundancy => write!(f, "REDUCED_REDUNDANCY"),
            StorageClass::Glacier => write!(f, "GLACIER"),
            StorageClass::DeepArchive => write!(f, "DEEP_ARCHIVE"),
        }
    }
}

/// Options for creating a bucket.
#[derive(Debug, Clone, Default)]
pub struct CreateBucketOpts {
    pub region: Option<String>,
    pub tenant_slug: Option<String>,
}

/// Tenant/workspace metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Options for listing objects.
#[derive(Debug, Clone, Default)]
pub struct ListObjectsOpts {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<u32>,
    pub continuation_token: Option<String>,
    pub start_after: Option<String>,
}

/// Response from listing objects.
#[derive(Debug, Clone)]
pub struct ListObjectsResponse {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

/// Options for getting an object.
#[derive(Debug, Clone, Default)]
pub struct GetObjectOpts {
    pub range: Option<(u64, Option<u64>)>,
    pub if_match: Option<String>,
    pub if_none_match: Option<String>,
    pub if_modified_since: Option<DateTime<Utc>>,
    pub if_unmodified_since: Option<DateTime<Utc>>,
    /// Specific version to retrieve (None = latest)
    pub version_id: Option<String>,
    /// For SSE-C: Base64-encoded customer-provided encryption key
    pub sse_customer_key: Option<String>,
    /// For SSE-C: Base64-encoded MD5 of the customer key (for verification)
    pub sse_customer_key_md5: Option<String>,
}

/// Response from getting an object.
pub struct GetObjectResponse {
    pub info: ObjectInfo,
    pub body: ObjectBody,
}

/// Body of an object - a stream of bytes.
pub type ObjectBody = Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>;

/// Options for putting an object.
#[derive(Debug, Clone, Default)]
pub struct PutObjectOpts {
    pub content_type: Option<String>,
    pub content_encoding: Option<String>,
    pub content_disposition: Option<String>,
    pub cache_control: Option<String>,
    pub metadata: HashMap<String, String>,
    pub storage_class: Option<StorageClass>,
    /// Server-side encryption algorithm (SSE-S3 or SSE-C)
    pub server_side_encryption: Option<ServerSideEncryption>,
    /// For SSE-C: Base64-encoded customer-provided encryption key
    pub sse_customer_key: Option<String>,
    /// For SSE-C: Base64-encoded MD5 of the customer key (for verification)
    pub sse_customer_key_md5: Option<String>,
}

/// Response from putting an object.
#[derive(Debug, Clone)]
pub struct PutObjectResponse {
    pub etag: String,
    pub version_id: Option<String>,
}

/// Options for copying an object.
#[derive(Debug, Clone, Default)]
pub struct CopyObjectOpts {
    pub metadata_directive: MetadataDirective,
    pub metadata: HashMap<String, String>,
}

/// Metadata directive for copy operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetadataDirective {
    #[default]
    Copy,
    Replace,
}

/// Response from copying an object.
#[derive(Debug, Clone)]
pub struct CopyObjectResponse {
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    /// Version ID of the newly created copy
    pub version_id: Option<String>,
}

/// Information about a multipart upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub initiated: DateTime<Utc>,
}

/// Information about an uploaded part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub part_number: u16,
    pub etag: String,
    pub size: u64,
    pub last_modified: DateTime<Utc>,
}

/// Information for completing a multipart upload.
#[derive(Debug, Clone)]
pub struct CompletePart {
    pub part_number: u16,
    pub etag: String,
}

/// Options for listing parts.
#[derive(Debug, Clone, Default)]
pub struct ListPartsOpts {
    pub max_parts: Option<u32>,
    pub part_number_marker: Option<u16>,
}

/// Response from listing parts.
#[derive(Debug, Clone)]
pub struct ListPartsResponse {
    pub parts: Vec<PartInfo>,
    pub is_truncated: bool,
    pub next_part_number_marker: Option<u16>,
}

/// Options for listing multipart uploads.
#[derive(Debug, Clone, Default)]
pub struct ListUploadsOpts {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_uploads: Option<u32>,
    pub key_marker: Option<String>,
    pub upload_id_marker: Option<String>,
}

/// Response from listing multipart uploads.
#[derive(Debug, Clone)]
pub struct ListUploadsResponse {
    pub uploads: Vec<MultipartUpload>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
}

/// The main object storage trait.
///
/// Implementations of this trait provide the actual storage functionality.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    // === Bucket Operations ===

    /// Create a new bucket.
    async fn create_bucket(&self, bucket: &str, opts: CreateBucketOpts) -> Result<()>;

    /// Delete a bucket. The bucket must be empty.
    async fn delete_bucket(&self, bucket: &str) -> Result<()>;

    /// List all buckets.
    async fn list_buckets(&self) -> Result<Vec<BucketInfo>>;

    /// Check if a bucket exists.
    async fn bucket_exists(&self, bucket: &str) -> Result<bool>;

    /// Get bucket info.
    async fn head_bucket(&self, bucket: &str) -> Result<BucketInfo>;

    // === Tenant Operations ===

    /// List all tenants/workspaces.
    async fn list_tenants(&self) -> Result<Vec<TenantInfo>>;

    /// Create a tenant/workspace.
    async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        owner: &str,
        notes: Option<&str>,
    ) -> Result<TenantInfo>;

    /// Get a tenant by slug.
    async fn get_tenant(&self, slug: &str) -> Result<TenantInfo>;

    /// Delete a tenant by slug.
    async fn delete_tenant(&self, slug: &str) -> Result<()>;

    // === Object Operations ===

    /// Get an object.
    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        opts: GetObjectOpts,
    ) -> Result<GetObjectResponse>;

    /// Put an object.
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: ObjectBody,
        size: u64,
        opts: PutObjectOpts,
    ) -> Result<PutObjectResponse>;

    /// Delete an object.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;

    /// Get object metadata without body.
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectInfo>;

    /// List objects in a bucket.
    async fn list_objects(
        &self,
        bucket: &str,
        opts: ListObjectsOpts,
    ) -> Result<ListObjectsResponse>;

    /// Copy an object.
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        opts: CopyObjectOpts,
    ) -> Result<CopyObjectResponse>;

    // === Multipart Upload Operations ===

    /// Create a new multipart upload.
    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        opts: PutObjectOpts,
    ) -> Result<MultipartUpload>;

    /// Upload a part.
    async fn upload_part(
        &self,
        upload: &MultipartUpload,
        part_number: u16,
        body: ObjectBody,
        size: u64,
    ) -> Result<PartInfo>;

    /// Complete a multipart upload.
    async fn complete_multipart_upload(
        &self,
        upload: &MultipartUpload,
        parts: Vec<CompletePart>,
    ) -> Result<PutObjectResponse>;

    /// Abort a multipart upload.
    async fn abort_multipart_upload(&self, upload: &MultipartUpload) -> Result<()>;

    /// List parts of a multipart upload.
    async fn list_parts(
        &self,
        upload: &MultipartUpload,
        opts: ListPartsOpts,
    ) -> Result<ListPartsResponse>;

    /// List multipart uploads.
    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        opts: ListUploadsOpts,
    ) -> Result<ListUploadsResponse>;

    // === Versioning Operations ===

    /// Set bucket versioning status.
    /// enabled = true to enable, false to suspend.
    async fn set_bucket_versioning(&self, bucket: &str, enabled: bool) -> Result<()>;

    /// Get bucket versioning status.
    /// Returns None if versioning was never enabled.
    async fn get_bucket_versioning(&self, bucket: &str) -> Result<Option<bool>>;

    /// Delete a specific object version.
    /// If version_id is None, creates a delete marker (soft delete).
    /// If version_id is provided, permanently deletes that version.
    async fn delete_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<DeleteObjectResponse>;

    /// List all versions of objects in a bucket.
    async fn list_object_versions(
        &self,
        bucket: &str,
        opts: ListVersionsOpts,
    ) -> Result<ListVersionsResponse>;

    // === CORS Operations ===

    /// Get CORS configuration for a bucket.
    async fn get_bucket_cors(&self, bucket: &str) -> Result<Option<CorsConfiguration>>;

    /// Set CORS configuration for a bucket.
    async fn put_bucket_cors(&self, bucket: &str, config: CorsConfiguration) -> Result<()>;

    /// Delete CORS configuration for a bucket.
    async fn delete_bucket_cors(&self, bucket: &str) -> Result<()>;

    // === Object Lock Operations ===

    /// Get object lock configuration for a bucket.
    async fn get_object_lock_configuration(
        &self,
        bucket: &str,
    ) -> Result<Option<ObjectLockConfiguration>>;

    /// Set object lock configuration for a bucket.
    /// Note: Object lock can only be enabled at bucket creation time.
    async fn put_object_lock_configuration(
        &self,
        bucket: &str,
        config: ObjectLockConfiguration,
    ) -> Result<()>;

    /// Get retention settings for an object version.
    async fn get_object_retention(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<Option<ObjectRetention>>;

    /// Set retention settings for an object version.
    async fn put_object_retention(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
        retention: ObjectRetention,
        bypass_governance: bool,
    ) -> Result<()>;

    /// Get legal hold status for an object version.
    async fn get_object_legal_hold(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
    ) -> Result<LegalHoldStatus>;

    /// Set legal hold status for an object version.
    async fn put_object_legal_hold(
        &self,
        bucket: &str,
        key: &str,
        version_id: Option<&str>,
        status: LegalHoldStatus,
    ) -> Result<()>;

    // === Lifecycle Operations ===

    /// Get lifecycle configuration for a bucket.
    async fn get_bucket_lifecycle(&self, bucket: &str) -> Result<Option<LifecycleConfiguration>>;

    /// Set lifecycle configuration for a bucket.
    async fn put_bucket_lifecycle(
        &self,
        bucket: &str,
        config: LifecycleConfiguration,
    ) -> Result<()>;

    /// Delete lifecycle configuration for a bucket.
    async fn delete_bucket_lifecycle(&self, bucket: &str) -> Result<()>;

    // === Event Notification Operations ===

    /// Get notification configuration for a bucket.
    async fn get_bucket_notification(
        &self,
        bucket: &str,
    ) -> Result<Option<NotificationConfiguration>>;

    /// Set notification configuration for a bucket.
    async fn put_bucket_notification(
        &self,
        bucket: &str,
        config: NotificationConfiguration,
    ) -> Result<()>;

    // === Bucket Tagging Operations ===

    /// Get tagging configuration for a bucket.
    async fn get_bucket_tagging(&self, bucket: &str) -> Result<Option<TaggingConfiguration>>;

    /// Set tagging configuration for a bucket.
    async fn put_bucket_tagging(&self, bucket: &str, config: TaggingConfiguration) -> Result<()>;

    /// Delete tagging configuration for a bucket.
    async fn delete_bucket_tagging(&self, bucket: &str) -> Result<()>;

    // === Audit Logging Operations ===

    /// Log an audit event.
    async fn log_audit_event(&self, entry: AuditLogEntry) -> Result<()>;

    /// Query audit log entries.
    async fn query_audit_log(&self, opts: AuditQueryOpts) -> Result<Vec<AuditLogEntry>>;
}

/// Response from deleting an object or version.
#[derive(Debug, Clone)]
pub struct DeleteObjectResponse {
    /// True if a delete marker was created (soft delete in versioned bucket)
    pub delete_marker: bool,
    /// Version ID of the deleted object or the new delete marker
    pub version_id: Option<String>,
}

/// Options for listing object versions.
#[derive(Debug, Clone, Default)]
pub struct ListVersionsOpts {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<u32>,
    pub key_marker: Option<String>,
    pub version_id_marker: Option<String>,
}

/// An object version or delete marker.
#[derive(Debug, Clone)]
pub struct ObjectVersion {
    pub key: String,
    pub version_id: String,
    pub is_latest: bool,
    pub is_delete_marker: bool,
    pub last_modified: DateTime<Utc>,
    pub etag: Option<String>,
    pub size: Option<u64>,
    pub storage_class: Option<StorageClass>,
}

/// Response from listing object versions.
#[derive(Debug, Clone)]
pub struct ListVersionsResponse {
    pub versions: Vec<ObjectVersion>,
    pub delete_markers: Vec<ObjectVersion>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_version_id_marker: Option<String>,
}

// === CORS Configuration ===

/// A single CORS rule for a bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsRule {
    /// Optional ID for this rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Allowed origins (can include wildcards like "*" or "https://*.example.com").
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods.
    pub allowed_methods: Vec<CorsMethod>,
    /// Allowed request headers.
    #[serde(default)]
    pub allowed_headers: Vec<String>,
    /// Response headers exposed to the browser.
    #[serde(default)]
    pub expose_headers: Vec<String>,
    /// Max age in seconds for preflight cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u32>,
}

/// HTTP methods allowed in CORS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CorsMethod {
    Get,
    Put,
    Post,
    Delete,
    Head,
}

impl std::fmt::Display for CorsMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorsMethod::Get => write!(f, "GET"),
            CorsMethod::Put => write!(f, "PUT"),
            CorsMethod::Post => write!(f, "POST"),
            CorsMethod::Delete => write!(f, "DELETE"),
            CorsMethod::Head => write!(f, "HEAD"),
        }
    }
}

impl std::str::FromStr for CorsMethod {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(CorsMethod::Get),
            "PUT" => Ok(CorsMethod::Put),
            "POST" => Ok(CorsMethod::Post),
            "DELETE" => Ok(CorsMethod::Delete),
            "HEAD" => Ok(CorsMethod::Head),
            _ => Err(format!("invalid CORS method: {}", s)),
        }
    }
}

/// CORS configuration for a bucket.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CorsConfiguration {
    pub rules: Vec<CorsRule>,
}

// === Object Lock (WORM) Configuration ===

/// Object lock retention mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetentionMode {
    /// Objects can be deleted by users with special permissions.
    Governance,
    /// Objects cannot be deleted until retention period expires.
    Compliance,
}

impl std::fmt::Display for RetentionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetentionMode::Governance => write!(f, "GOVERNANCE"),
            RetentionMode::Compliance => write!(f, "COMPLIANCE"),
        }
    }
}

impl std::str::FromStr for RetentionMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GOVERNANCE" => Ok(RetentionMode::Governance),
            "COMPLIANCE" => Ok(RetentionMode::Compliance),
            _ => Err(format!("invalid retention mode: {}", s)),
        }
    }
}

/// Default retention rule for a bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRetention {
    /// Retention mode (GOVERNANCE or COMPLIANCE).
    pub mode: RetentionMode,
    /// Number of days for retention (mutually exclusive with years).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    /// Number of years for retention (mutually exclusive with days).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub years: Option<u32>,
}

/// Object lock rule for a bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectLockRule {
    pub default_retention: Option<DefaultRetention>,
}

/// Object lock configuration for a bucket.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectLockConfiguration {
    /// Whether object lock is enabled for this bucket.
    pub enabled: bool,
    /// Optional default retention rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<ObjectLockRule>,
}

/// Object retention settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectRetention {
    /// Retention mode.
    pub mode: RetentionMode,
    /// Retention date until which the object is locked.
    pub retain_until_date: DateTime<Utc>,
}

/// Legal hold status for an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LegalHoldStatus {
    On,
    Off,
}

impl std::fmt::Display for LegalHoldStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LegalHoldStatus::On => write!(f, "ON"),
            LegalHoldStatus::Off => write!(f, "OFF"),
        }
    }
}

impl std::str::FromStr for LegalHoldStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ON" => Ok(LegalHoldStatus::On),
            "OFF" => Ok(LegalHoldStatus::Off),
            _ => Err(format!("invalid legal hold status: {}", s)),
        }
    }
}

// === Lifecycle Configuration ===

/// A lifecycle rule for automatic object management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
    /// Unique identifier for this rule.
    pub id: String,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Prefix filter (objects must match this prefix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Tag filters (objects must have all these tags).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<(String, String)>,
    /// Expiration settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration: Option<LifecycleExpiration>,
    /// Transition settings for storage class changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<LifecycleTransition>,
    /// Noncurrent version expiration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noncurrent_version_expiration: Option<NoncurrentVersionExpiration>,
    /// Abort incomplete multipart uploads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abort_incomplete_multipart_upload: Option<AbortIncompleteMultipartUpload>,
}

/// Expiration settings for current object versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleExpiration {
    /// Days after creation to expire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    /// Specific date to expire (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    /// Whether to expire delete markers with no noncurrent versions.
    #[serde(default)]
    pub expired_object_delete_marker: bool,
}

/// Transition to a different storage class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransition {
    /// Days after creation to transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    /// Specific date to transition (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    /// Target storage class.
    pub storage_class: StorageClass,
}

/// Expiration for noncurrent object versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoncurrentVersionExpiration {
    /// Days after becoming noncurrent to expire.
    pub noncurrent_days: u32,
    /// Maximum number of noncurrent versions to retain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newer_noncurrent_versions: Option<u32>,
}

/// Settings for aborting incomplete multipart uploads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbortIncompleteMultipartUpload {
    /// Days after initiation to abort.
    pub days_after_initiation: u32,
}

/// Lifecycle configuration for a bucket.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LifecycleConfiguration {
    pub rules: Vec<LifecycleRule>,
}

// === Event Notifications ===

/// Types of S3 events that can trigger notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum S3EventType {
    #[serde(rename = "s3:ObjectCreated:*")]
    ObjectCreatedAll,
    #[serde(rename = "s3:ObjectCreated:Put")]
    ObjectCreatedPut,
    #[serde(rename = "s3:ObjectCreated:Post")]
    ObjectCreatedPost,
    #[serde(rename = "s3:ObjectCreated:Copy")]
    ObjectCreatedCopy,
    #[serde(rename = "s3:ObjectCreated:CompleteMultipartUpload")]
    ObjectCreatedCompleteMultipartUpload,
    #[serde(rename = "s3:ObjectRemoved:*")]
    ObjectRemovedAll,
    #[serde(rename = "s3:ObjectRemoved:Delete")]
    ObjectRemovedDelete,
    #[serde(rename = "s3:ObjectRemoved:DeleteMarkerCreated")]
    ObjectRemovedDeleteMarkerCreated,
}

impl std::fmt::Display for S3EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            S3EventType::ObjectCreatedAll => write!(f, "s3:ObjectCreated:*"),
            S3EventType::ObjectCreatedPut => write!(f, "s3:ObjectCreated:Put"),
            S3EventType::ObjectCreatedPost => write!(f, "s3:ObjectCreated:Post"),
            S3EventType::ObjectCreatedCopy => write!(f, "s3:ObjectCreated:Copy"),
            S3EventType::ObjectCreatedCompleteMultipartUpload => {
                write!(f, "s3:ObjectCreated:CompleteMultipartUpload")
            }
            S3EventType::ObjectRemovedAll => write!(f, "s3:ObjectRemoved:*"),
            S3EventType::ObjectRemovedDelete => write!(f, "s3:ObjectRemoved:Delete"),
            S3EventType::ObjectRemovedDeleteMarkerCreated => {
                write!(f, "s3:ObjectRemoved:DeleteMarkerCreated")
            }
        }
    }
}

/// Notification destination type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationDestination {
    /// Webhook URL to POST events to.
    #[serde(rename = "webhook")]
    Webhook { url: String },
    /// AMQP/RabbitMQ queue.
    #[serde(rename = "amqp")]
    Amqp {
        url: String,
        exchange: String,
        routing_key: String,
    },
    /// Kafka topic.
    #[serde(rename = "kafka")]
    Kafka { brokers: Vec<String>, topic: String },
    /// Redis pub/sub channel.
    #[serde(rename = "redis")]
    Redis { url: String, channel: String },
}

/// Filter rules for event notifications.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationFilter {
    /// Prefix filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Suffix filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

/// A notification configuration rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    /// Unique identifier for this rule.
    pub id: String,
    /// Events that trigger this notification.
    pub events: Vec<S3EventType>,
    /// Filter for matching objects.
    #[serde(default)]
    pub filter: NotificationFilter,
    /// Destination for notifications.
    pub destination: NotificationDestination,
}

/// Event notification configuration for a bucket.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationConfiguration {
    pub rules: Vec<NotificationRule>,
}

// === Bucket Tagging ===

/// A tag for bucket or object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

/// Tagging configuration for a bucket.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaggingConfiguration {
    pub tags: Vec<Tag>,
}

/// An S3 event record (for notification payloads).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3EventRecord {
    pub event_version: String,
    pub event_source: String,
    pub event_time: DateTime<Utc>,
    pub event_name: String,
    pub bucket_name: String,
    pub object_key: String,
    pub object_size: Option<u64>,
    pub object_etag: Option<String>,
    pub object_version_id: Option<String>,
    pub request_id: String,
    pub source_ip: Option<String>,
}

// === Audit Logging ===

/// Audit log entry for tracking operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Unique ID for this log entry.
    pub id: String,
    /// Timestamp of the operation.
    pub timestamp: DateTime<Utc>,
    /// Operation type (e.g., "PutObject", "DeleteBucket").
    pub operation: String,
    /// Bucket name (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    /// Object key (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// User/access key that performed the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    /// Source IP address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
    /// HTTP status code of the response.
    pub status_code: u16,
    /// Error code if operation failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Request duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Bytes sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    /// Request ID.
    pub request_id: String,
}

/// Options for querying audit log entries.
#[derive(Debug, Clone, Default)]
pub struct AuditQueryOpts {
    /// Filter by bucket name.
    pub bucket: Option<String>,
    /// Filter by object key (prefix match).
    pub key_prefix: Option<String>,
    /// Filter by operation type.
    pub operation: Option<String>,
    /// Filter by principal (user/access key).
    pub principal: Option<String>,
    /// Start time for query range.
    pub start_time: Option<DateTime<Utc>>,
    /// End time for query range.
    pub end_time: Option<DateTime<Utc>>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}
