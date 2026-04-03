//! IAM error types.

use thiserror::Error;

/// Result type for IAM operations.
pub type Result<T> = std::result::Result<T, IamError>;

/// Errors that can occur in IAM operations.
#[derive(Debug, Error)]
pub enum IamError {
    /// User already exists.
    #[error("User already exists: {0}")]
    UserExists(String),

    /// User not found.
    #[error("User not found: {0}")]
    UserNotFound(String),

    /// Access key not found.
    #[error("Access key not found: {0}")]
    AccessKeyNotFound(String),

    /// Policy not found.
    #[error("Policy not found: {0}")]
    PolicyNotFound(String),

    /// Group already exists.
    #[error("Group already exists: {0}")]
    GroupExists(String),

    /// Group not found.
    #[error("Group not found: {0}")]
    GroupNotFound(String),

    /// Cannot delete root user.
    #[error("Cannot delete root user")]
    CannotDeleteRoot,

    /// Cannot modify root user's access keys.
    #[error("Cannot modify root user access keys through IAM")]
    CannotModifyRootKeys,

    /// Maximum access keys per user exceeded.
    #[error("Maximum access keys (2) per user exceeded")]
    MaxAccessKeysExceeded,

    /// Invalid policy document.
    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),

    /// Database error.
    #[error("Database error: {0}")]
    Database(String),

    /// Password hashing error.
    #[error("Password hash error: {0}")]
    PasswordHash(String),

    /// Invalid credentials.
    #[error("Invalid credentials")]
    InvalidCredentials,

    /// Encryption error.
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for IamError {
    fn from(err: rusqlite::Error) -> Self {
        IamError::Database(err.to_string())
    }
}

impl From<tokio_rusqlite::Error> for IamError {
    fn from(err: tokio_rusqlite::Error) -> Self {
        IamError::Database(err.to_string())
    }
}
