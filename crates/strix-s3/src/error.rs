//! Error conversion for S3 responses.

use s3s::S3Error;
use s3s::s3_error;
use strix_core::Error;

/// Convert Strix errors to S3 errors.
pub fn to_s3_error(err: Error) -> S3Error {
    match err {
        // Bucket errors
        Error::BucketNotFound(name) => s3_error!(NoSuchBucket, "Bucket not found: {}", name),
        Error::BucketAlreadyExists(name) => {
            s3_error!(BucketAlreadyOwnedByYou, "Bucket already exists: {}", name)
        }
        Error::BucketNotEmpty(name) => s3_error!(BucketNotEmpty, "Bucket not empty: {}", name),
        Error::InvalidBucketName(msg) => s3_error!(InvalidBucketName, "{}", msg),
        Error::TenantNotFound(msg) => s3_error!(InvalidArgument, "Tenant not found: {}", msg),
        Error::TenantAlreadyExists(msg) => {
            s3_error!(BucketAlreadyOwnedByYou, "Tenant already exists: {}", msg)
        }

        // Object errors
        Error::ObjectNotFound { bucket, key } => {
            s3_error!(NoSuchKey, "Object not found: {}/{}", bucket, key)
        }
        Error::InvalidObjectKey(msg) => s3_error!(InvalidArgument, "Invalid key: {}", msg),

        // Versioning errors
        Error::VersionNotFound {
            bucket,
            key,
            version_id,
        } => s3_error!(
            NoSuchVersion,
            "Version not found: {}/{}?versionId={}",
            bucket,
            key,
            version_id
        ),
        Error::InvalidVersionId(msg) => s3_error!(InvalidArgument, "Invalid version ID: {}", msg),

        // Multipart errors
        Error::UploadNotFound(id) => s3_error!(NoSuchUpload, "Upload not found: {}", id),
        Error::InvalidPartNumber(n) => s3_error!(InvalidPart, "Invalid part number: {}", n),
        Error::InvalidPartOrder => s3_error!(InvalidPartOrder, "Parts not in ascending order"),
        Error::EntityTooSmall => s3_error!(EntityTooSmall, "Part size too small"),
        Error::EntityTooLarge => s3_error!(EntityTooLarge, "Part size too large"),
        Error::NoSuchPart(n) => s3_error!(InvalidPart, "No such part: {}", n),

        // Auth errors
        Error::AccessDenied => s3_error!(AccessDenied),
        Error::InvalidCredentials => s3_error!(InvalidAccessKeyId),
        Error::SignatureMismatch => s3_error!(SignatureDoesNotMatch),
        Error::ExpiredToken => s3_error!(ExpiredToken),
        Error::IncompleteSignature => s3_error!(InvalidArgument, "Incomplete signature"),

        // Precondition errors
        Error::PreconditionFailed => s3_error!(PreconditionFailed),
        Error::NotModified => s3_error!(NotModified),

        // Range errors
        Error::InvalidRange(msg) => s3_error!(InvalidRange, "{}", msg),

        // Lock errors
        Error::ObjectLocked => s3_error!(AccessDenied, "Object is locked"),
        Error::InvalidObjectLockConfiguration => {
            s3_error!(InvalidArgument, "Invalid object lock configuration")
        }

        // CORS errors
        Error::CorsNotConfigured => {
            s3_error!(NoSuchCORSConfiguration, "CORS configuration not found")
        }

        // Lifecycle errors
        Error::NoSuchLifecycleConfiguration => {
            s3_error!(
                NoSuchLifecycleConfiguration,
                "Lifecycle configuration not found"
            )
        }

        // Notification errors
        Error::NoSuchNotificationConfiguration => {
            s3_error!(InternalError, "Notification configuration not found")
        }

        // Storage errors
        Error::Io(e) => s3_error!(InternalError, "I/O error: {}", e),
        Error::MetadataCorruption(msg) => s3_error!(InternalError, "Metadata error: {}", msg),
        Error::ChecksumMismatch => s3_error!(BadDigest, "Checksum mismatch"),

        // Serialization errors
        Error::Serialization(msg) => s3_error!(InternalError, "Serialization error: {}", msg),

        // Invalid argument
        Error::InvalidArgument(msg) => s3_error!(InvalidArgument, "{}", msg),

        // Encryption errors
        Error::EncryptionError(msg) => s3_error!(InternalError, "Encryption error: {}", msg),
        Error::InvalidEncryptionKey => s3_error!(InvalidArgument, "Invalid encryption key"),
        Error::MissingSecurityHeader => {
            s3_error!(InvalidArgument, "Missing required security header")
        }

        // Request errors
        Error::RequestTimeout => s3_error!(RequestTimeout),
        Error::ServiceUnavailable => s3_error!(ServiceUnavailable),
        Error::SlowDown => s3_error!(SlowDown),

        // Internal errors
        Error::Internal(msg) => s3_error!(InternalError, "{}", msg),
    }
}
