//! Error types for Strix.

use thiserror::Error;

/// Result type alias using Strix's Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for Strix operations.
#[derive(Debug, Error)]
pub enum Error {
    // Bucket errors
    #[error("bucket not found: {0}")]
    BucketNotFound(String),

    #[error("bucket already exists: {0}")]
    BucketAlreadyExists(String),

    #[error("bucket not empty: {0}")]
    BucketNotEmpty(String),

    #[error("invalid bucket name: {0}")]
    InvalidBucketName(String),

    #[error("tenant not found: {0}")]
    TenantNotFound(String),

    #[error("tenant already exists: {0}")]
    TenantAlreadyExists(String),

    // Object errors
    #[error("object not found: {bucket}/{key}")]
    ObjectNotFound { bucket: String, key: String },

    #[error("invalid object key: {0}")]
    InvalidObjectKey(String),

    // Versioning errors
    #[error("version not found: {bucket}/{key}?versionId={version_id}")]
    VersionNotFound {
        bucket: String,
        key: String,
        version_id: String,
    },

    #[error("invalid version id: {0}")]
    InvalidVersionId(String),

    // Multipart errors
    #[error("upload not found: {0}")]
    UploadNotFound(String),

    #[error("invalid part number: {0}")]
    InvalidPartNumber(u16),

    #[error("invalid part order")]
    InvalidPartOrder,

    #[error("entity too small")]
    EntityTooSmall,

    #[error("entity too large")]
    EntityTooLarge,

    #[error("no such part: {0}")]
    NoSuchPart(u16),

    // Auth errors
    #[error("access denied")]
    AccessDenied,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("signature mismatch")]
    SignatureMismatch,

    #[error("expired token")]
    ExpiredToken,

    #[error("incomplete signature")]
    IncompleteSignature,

    // Precondition errors
    #[error("precondition failed")]
    PreconditionFailed,

    #[error("not modified")]
    NotModified,

    // Range errors
    #[error("invalid range: {0}")]
    InvalidRange(String),

    // Lock errors
    #[error("object locked")]
    ObjectLocked,

    #[error("invalid object lock configuration")]
    InvalidObjectLockConfiguration,

    // CORS errors
    #[error("CORS not configured")]
    CorsNotConfigured,

    // Lifecycle errors
    #[error("no such lifecycle configuration")]
    NoSuchLifecycleConfiguration,

    // Notification errors
    #[error("no such notification configuration")]
    NoSuchNotificationConfiguration,

    // Storage errors
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("metadata corruption: {0}")]
    MetadataCorruption(String),

    #[error("checksum mismatch")]
    ChecksumMismatch,

    // Serialization errors
    #[error("serialization error: {0}")]
    Serialization(String),

    // Invalid argument
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    // Encryption errors
    #[error("encryption error: {0}")]
    EncryptionError(String),

    #[error("invalid encryption key")]
    InvalidEncryptionKey,

    #[error("SSE-C key missing")]
    MissingSecurityHeader,

    // Request errors
    #[error("request timeout")]
    RequestTimeout,

    #[error("service unavailable")]
    ServiceUnavailable,

    #[error("slow down: too many requests")]
    SlowDown,

    // Internal errors
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Returns the S3 error code for this error.
    pub fn s3_code(&self) -> &'static str {
        match self {
            // Bucket errors
            Error::BucketNotFound(_) => "NoSuchBucket",
            Error::BucketAlreadyExists(_) => "BucketAlreadyExists",
            Error::BucketNotEmpty(_) => "BucketNotEmpty",
            Error::InvalidBucketName(_) => "InvalidBucketName",
            Error::TenantNotFound(_) => "NoSuchTenant",
            Error::TenantAlreadyExists(_) => "TenantAlreadyExists",

            // Object errors
            Error::ObjectNotFound { .. } => "NoSuchKey",
            Error::InvalidObjectKey(_) => "InvalidArgument",

            // Versioning errors
            Error::VersionNotFound { .. } => "NoSuchVersion",
            Error::InvalidVersionId(_) => "InvalidArgument",

            // Multipart errors
            Error::UploadNotFound(_) => "NoSuchUpload",
            Error::InvalidPartNumber(_) => "InvalidPart",
            Error::InvalidPartOrder => "InvalidPartOrder",
            Error::EntityTooSmall => "EntityTooSmall",
            Error::EntityTooLarge => "EntityTooLarge",
            Error::NoSuchPart(_) => "NoSuchPart",

            // Auth errors
            Error::AccessDenied => "AccessDenied",
            Error::InvalidCredentials => "InvalidAccessKeyId",
            Error::SignatureMismatch => "SignatureDoesNotMatch",
            Error::ExpiredToken => "ExpiredToken",
            Error::IncompleteSignature => "IncompleteSignature",

            // Precondition errors
            Error::PreconditionFailed => "PreconditionFailed",
            Error::NotModified => "NotModified",

            // Range errors
            Error::InvalidRange(_) => "InvalidRange",

            // Lock errors
            Error::ObjectLocked => "ObjectLocked",
            Error::InvalidObjectLockConfiguration => "InvalidObjectLockConfiguration",

            // CORS errors
            Error::CorsNotConfigured => "NoSuchCORSConfiguration",

            // Lifecycle errors
            Error::NoSuchLifecycleConfiguration => "NoSuchLifecycleConfiguration",

            // Notification errors
            Error::NoSuchNotificationConfiguration => "NoSuchNotificationConfiguration",

            // Storage errors
            Error::Io(_) => "InternalError",
            Error::MetadataCorruption(_) => "InternalError",
            Error::ChecksumMismatch => "BadDigest",

            // Serialization errors
            Error::Serialization(_) => "InternalError",

            // Invalid argument
            Error::InvalidArgument(_) => "InvalidArgument",

            // Encryption errors
            Error::EncryptionError(_) => "InternalError",
            Error::InvalidEncryptionKey => "InvalidArgument",
            Error::MissingSecurityHeader => "MissingSecurityHeader",

            // Request errors
            Error::RequestTimeout => "RequestTimeout",
            Error::ServiceUnavailable => "ServiceUnavailable",
            Error::SlowDown => "SlowDown",

            // Internal errors
            Error::Internal(_) => "InternalError",
        }
    }

    /// Returns the HTTP status code for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            // Bucket errors
            Error::BucketNotFound(_) => 404,
            Error::BucketAlreadyExists(_) => 409,
            Error::BucketNotEmpty(_) => 409,
            Error::InvalidBucketName(_) => 400,
            Error::TenantNotFound(_) => 404,
            Error::TenantAlreadyExists(_) => 409,

            // Object errors
            Error::ObjectNotFound { .. } => 404,
            Error::InvalidObjectKey(_) => 400,

            // Versioning errors
            Error::VersionNotFound { .. } => 404,
            Error::InvalidVersionId(_) => 400,

            // Multipart errors
            Error::UploadNotFound(_) => 404,
            Error::InvalidPartNumber(_) => 400,
            Error::InvalidPartOrder => 400,
            Error::EntityTooSmall => 400,
            Error::EntityTooLarge => 400,
            Error::NoSuchPart(_) => 404,

            // Auth errors
            Error::AccessDenied => 403,
            Error::InvalidCredentials => 403,
            Error::SignatureMismatch => 403,
            Error::ExpiredToken => 400,
            Error::IncompleteSignature => 400,

            // Precondition errors
            Error::PreconditionFailed => 412,
            Error::NotModified => 304,

            // Range errors
            Error::InvalidRange(_) => 416,

            // Lock errors
            Error::ObjectLocked => 403,
            Error::InvalidObjectLockConfiguration => 400,

            // CORS errors
            Error::CorsNotConfigured => 404,

            // Lifecycle errors
            Error::NoSuchLifecycleConfiguration => 404,

            // Notification errors
            Error::NoSuchNotificationConfiguration => 404,

            // Storage errors
            Error::Io(_) => 500,
            Error::MetadataCorruption(_) => 500,
            Error::ChecksumMismatch => 400,

            // Serialization errors
            Error::Serialization(_) => 500,

            // Invalid argument
            Error::InvalidArgument(_) => 400,

            // Encryption errors
            Error::EncryptionError(_) => 500,
            Error::InvalidEncryptionKey => 400,
            Error::MissingSecurityHeader => 400,

            // Request errors
            Error::RequestTimeout => 408,
            Error::ServiceUnavailable => 503,
            Error::SlowDown => 503,

            // Internal errors
            Error::Internal(_) => 500,
        }
    }
}
