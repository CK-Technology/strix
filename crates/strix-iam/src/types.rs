//! IAM data types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A user in the IAM system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique username.
    pub username: String,
    /// User ARN (e.g., arn:strix:iam::user/username).
    pub arn: String,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// Current status.
    pub status: UserStatus,
    /// Whether this is the root user.
    pub is_root: bool,
    /// Argon2id password hash (for console login).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
}

impl User {
    /// Create a new user.
    pub fn new(username: String) -> Self {
        Self {
            arn: format!("arn:strix:iam::user/{}", username),
            username,
            created_at: Utc::now(),
            status: UserStatus::Active,
            is_root: false,
            password_hash: None,
        }
    }

    /// Create the root user.
    pub fn root() -> Self {
        Self {
            username: "root".to_string(),
            arn: "arn:strix:iam::root".to_string(),
            created_at: Utc::now(),
            status: UserStatus::Active,
            is_root: true,
            password_hash: None,
        }
    }
}

/// Status of a user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    /// User is active and can authenticate.
    Active,
    /// User is disabled and cannot authenticate.
    Inactive,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Inactive => "inactive",
        }
    }
}

impl std::str::FromStr for UserStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(UserStatus::Active),
            "inactive" => Ok(UserStatus::Inactive),
            _ => Err(format!("Invalid user status: {}", s)),
        }
    }
}

/// An access key for API authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessKey {
    /// Access key ID (20 characters).
    pub access_key_id: String,
    /// Secret access key (40 characters). Only set when first created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
    /// Username this key belongs to.
    pub username: String,
    /// When the key was created.
    pub created_at: DateTime<Utc>,
    /// Current status.
    pub status: AccessKeyStatus,
    /// When the key was last used for authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<DateTime<Utc>>,
}

/// Status of an access key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessKeyStatus {
    /// Key is active and can be used.
    Active,
    /// Key is disabled and cannot be used.
    Inactive,
}

impl AccessKeyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessKeyStatus::Active => "active",
            AccessKeyStatus::Inactive => "inactive",
        }
    }
}

impl std::str::FromStr for AccessKeyStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(AccessKeyStatus::Active),
            "inactive" => Ok(AccessKeyStatus::Inactive),
            _ => Err(format!("Invalid access key status: {}", s)),
        }
    }
}

/// A group in the IAM system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Unique group name.
    pub name: String,
    /// Group ARN (e.g., arn:strix:iam::group/name).
    pub arn: String,
    /// When the group was created.
    pub created_at: DateTime<Utc>,
    /// List of usernames in this group.
    pub members: Vec<String>,
    /// List of policy names attached to this group.
    pub policies: Vec<String>,
}

impl Group {
    /// Create a new group.
    pub fn new(name: String) -> Self {
        Self {
            arn: format!("arn:strix:iam::group/{}", name),
            name,
            created_at: Utc::now(),
            members: Vec::new(),
            policies: Vec::new(),
        }
    }
}

// === STS Temporary Credentials ===

/// Temporary security credentials from STS AssumeRole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporaryCredential {
    /// Access key ID for the session (prefixed with ASIA).
    pub access_key_id: String,
    /// Secret access key for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
    /// Session token (required for API calls with temp credentials).
    pub session_token: String,
    /// When these credentials expire.
    pub expiration: DateTime<Utc>,
    /// The username these credentials represent.
    pub assumed_identity: String,
}

/// Request for assuming a role / getting temporary credentials.
#[derive(Debug, Clone)]
pub struct AssumeRoleRequest {
    /// The username to assume (for simplified implementation).
    pub username: String,
    /// Session name for tracking/audit.
    pub session_name: Option<String>,
    /// Duration in seconds (900-43200, default 3600).
    pub duration_seconds: Option<u32>,
}
