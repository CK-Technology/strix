//! Admin API for Strix.
//!
//! Provides REST endpoints for server administration, user management,
//! and access key operations.

pub mod auth;
mod handlers;
mod routes;

pub use auth::{
    AuthState, AuthenticatedUser, CsrfConfig, LoginRequest, LoginResponse, RateLimiter,
    csrf_middleware,
};
pub use handlers::{AdminState, PresignConfig, ServerConfig};
pub use routes::admin_router;

use serde::{Deserialize, Serialize};
use strix_iam::{AccessKey, Group, Policy, User};

/// Standard error codes for admin API.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// User not found.
    UserNotFound,
    /// User already exists.
    UserAlreadyExists,
    /// Group not found.
    GroupNotFound,
    /// Group already exists.
    GroupAlreadyExists,
    /// Policy not found.
    PolicyNotFound,
    /// Access key not found.
    AccessKeyNotFound,
    /// Bucket not found.
    BucketNotFound,
    /// Bucket already exists.
    BucketAlreadyExists,
    /// Invalid request parameters.
    InvalidRequest,
    /// Authentication required.
    Unauthorized,
    /// Permission denied.
    Forbidden,
    /// Resource conflict.
    Conflict,
    /// Rate limit exceeded.
    RateLimitExceeded,
    /// Internal server error.
    InternalError,
}

/// Admin API error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error code (machine-readable).
    pub code: ErrorCode,
    /// Human-readable error message.
    pub error: String,
    /// Optional additional details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional request ID for tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InternalError,
            error: error.into(),
            message: None,
            request_id: None,
        }
    }

    pub fn with_code(code: ErrorCode, error: impl Into<String>) -> Self {
        Self {
            code,
            error: error.into(),
            message: None,
            request_id: None,
        }
    }

    pub fn with_message(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InternalError,
            error: error.into(),
            message: Some(message.into()),
            request_id: None,
        }
    }

    pub fn user_not_found(username: &str) -> Self {
        Self::with_code(
            ErrorCode::UserNotFound,
            format!("User not found: {}", username),
        )
    }

    pub fn group_not_found(name: &str) -> Self {
        Self::with_code(
            ErrorCode::GroupNotFound,
            format!("Group not found: {}", name),
        )
    }

    pub fn policy_not_found(name: &str) -> Self {
        Self::with_code(
            ErrorCode::PolicyNotFound,
            format!("Policy not found: {}", name),
        )
    }

    pub fn bucket_not_found(name: &str) -> Self {
        Self::with_code(
            ErrorCode::BucketNotFound,
            format!("Bucket not found: {}", name),
        )
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::with_code(ErrorCode::InvalidRequest, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::with_code(ErrorCode::InternalError, msg)
    }

    pub fn unauthorized() -> Self {
        Self::with_code(ErrorCode::Unauthorized, "Authentication required")
    }
}

/// Pagination query parameters.
#[derive(Debug, Deserialize, Default)]
pub struct PaginationQuery {
    /// Maximum number of items to return (default 100, max 1000).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Number of items to skip (for offset-based pagination).
    #[serde(default)]
    pub offset: u32,
    /// Continuation token (for cursor-based pagination).
    #[serde(default)]
    pub marker: Option<String>,
}

fn default_limit() -> u32 {
    100
}

impl PaginationQuery {
    /// Validate and normalize pagination parameters.
    pub fn validate(&mut self) {
        if self.limit == 0 {
            self.limit = 100;
        } else if self.limit > 1000 {
            self.limit = 1000;
        }
    }
}

/// Paginated response wrapper.
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    /// Items in this page.
    pub items: Vec<T>,
    /// Total number of items (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Number of items in this page.
    pub count: u32,
    /// Current offset.
    pub offset: u32,
    /// Whether there are more items.
    pub has_more: bool,
    /// Marker for next page (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_marker: Option<String>,
}

/// Server info response.
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub version: String,
    pub commit: Option<String>,
    pub mode: String,
    pub uptime: u64,
    pub region: String,
}

/// Create user request.
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
}

/// Create user response.
#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub user: User,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key: Option<AccessKey>,
}

/// User info response (without sensitive data).
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub username: String,
    pub arn: String,
    pub created_at: String,
    pub status: String,
    pub policies: Vec<String>,
}

impl From<User> for UserInfo {
    fn from(user: User) -> Self {
        Self {
            username: user.username,
            arn: user.arn,
            created_at: user.created_at.to_rfc3339(),
            status: user.status.as_str().to_string(),
            policies: vec![],
        }
    }
}

/// List users response.
#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserInfo>,
}

/// Access key info (without secret).
#[derive(Debug, Serialize)]
pub struct AccessKeyInfo {
    pub access_key_id: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

impl From<AccessKey> for AccessKeyInfo {
    fn from(key: AccessKey) -> Self {
        Self {
            access_key_id: key.access_key_id,
            username: key.username,
            created_at: key.created_at.to_rfc3339(),
            status: key.status.as_str().to_string(),
        }
    }
}

/// Create access key response (includes secret).
#[derive(Debug, Serialize)]
pub struct CreateAccessKeyResponse {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

impl From<AccessKey> for CreateAccessKeyResponse {
    fn from(key: AccessKey) -> Self {
        Self {
            access_key_id: key.access_key_id,
            secret_access_key: key.secret_access_key.unwrap_or_default(),
            username: key.username,
            created_at: key.created_at.to_rfc3339(),
            status: key.status.as_str().to_string(),
        }
    }
}

/// List access keys response.
#[derive(Debug, Serialize)]
pub struct ListAccessKeysResponse {
    pub access_keys: Vec<AccessKeyInfo>,
}

/// Update access key request.
#[derive(Debug, Deserialize)]
pub struct UpdateAccessKeyRequest {
    pub status: String,
}

/// Attach policy request.
#[derive(Debug, Deserialize)]
pub struct AttachPolicyRequest {
    pub policy: Policy,
}

/// List policies response.
#[derive(Debug, Serialize)]
pub struct ListPoliciesResponse {
    pub policies: Vec<Policy>,
}

// === Standalone Policy Management Types ===

/// Create or update policy request.
#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub policy: Policy,
    #[serde(default)]
    pub description: Option<String>,
}

/// Policy info response.
#[derive(Debug, Serialize)]
pub struct PolicyInfo {
    pub name: String,
    pub version: String,
    pub statements: Vec<strix_iam::PolicyStatement>,
    pub description: Option<String>,
}

/// List managed policies response.
#[derive(Debug, Serialize)]
pub struct ListManagedPoliciesResponse {
    pub policies: Vec<PolicyInfo>,
}

// === Group Management Types ===

/// Create group request.
#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

/// Group info response.
#[derive(Debug, Serialize)]
pub struct GroupInfo {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub members: Vec<String>,
    pub policies: Vec<String>,
}

impl From<Group> for GroupInfo {
    fn from(group: Group) -> Self {
        Self {
            name: group.name,
            arn: group.arn,
            created_at: group.created_at.to_rfc3339(),
            members: group.members,
            policies: group.policies,
        }
    }
}

/// List groups response.
#[derive(Debug, Serialize)]
pub struct ListGroupsResponse {
    pub groups: Vec<GroupInfo>,
}

/// Add user to group request.
#[derive(Debug, Deserialize)]
pub struct AddUserToGroupRequest {
    pub username: String,
}

/// Bucket usage info.
#[derive(Debug, Serialize)]
pub struct BucketUsage {
    pub name: String,
    pub created_at: String,
    pub object_count: u64,
    pub total_size: u64,
}

/// Storage usage response.
#[derive(Debug, Serialize)]
pub struct StorageUsageResponse {
    pub buckets: Vec<BucketUsage>,
    pub total_buckets: u64,
    pub total_objects: u64,
    pub total_size: u64,
}

// === Bucket Management Types ===

/// Create bucket request.
#[derive(Debug, Deserialize)]
pub struct CreateBucketRequest {
    pub name: String,
    #[serde(default)]
    pub tenant_slug: Option<String>,
    /// Enable versioning on bucket creation.
    #[serde(default)]
    pub versioning: bool,
    /// Enable object locking (WORM) on bucket creation.
    /// This cannot be disabled after creation.
    #[serde(default)]
    pub object_locking: bool,
}

/// Set bucket versioning request.
#[derive(Debug, Deserialize)]
pub struct SetVersioningRequest {
    /// Enable (true) or suspend (false) versioning.
    pub enabled: bool,
}

/// Get bucket versioning response.
#[derive(Debug, Serialize)]
pub struct GetVersioningResponse {
    /// Versioning status: "Enabled", "Suspended", or null (never enabled).
    pub status: Option<String>,
}

/// List buckets response.
#[derive(Debug, Serialize)]
pub struct ListBucketsResponse {
    pub buckets: Vec<BucketInfo>,
}

/// Bucket info.
#[derive(Debug, Serialize)]
pub struct BucketInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_slug: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TenantFilterQuery {
    pub tenant_slug: Option<String>,
}

// === Tenant Types ===

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub owner: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantInfoResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: String,
}

/// Detailed bucket info including configuration.
#[derive(Debug, Serialize)]
pub struct BucketDetailInfo {
    pub name: String,
    pub created_at: String,
    /// Versioning status: true = enabled, false = suspended, None = never enabled.
    pub versioning_enabled: Option<bool>,
    /// Whether object locking is enabled.
    pub object_locking_enabled: bool,
}

// === Object Management Types ===

/// List objects response.
#[derive(Debug, Serialize)]
pub struct ListObjectsResponse {
    pub objects: Vec<ObjectInfo>,
    pub prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

/// Object info.
#[derive(Debug, Serialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
    pub etag: Option<String>,
}

/// Delete objects request.
#[derive(Debug, Deserialize)]
pub struct DeleteObjectsRequest {
    pub keys: Vec<String>,
}

// === Bucket Policy Types ===

/// Bucket policy response.
#[derive(Debug, Serialize)]
pub struct BucketPolicyResponse {
    pub policy: strix_iam::BucketPolicy,
}

// === Pre-signed URL Types ===

/// Pre-signed URL request.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Bucket name
    pub bucket: String,
    /// Object key
    pub key: String,
    /// HTTP method (GET, PUT, DELETE)
    #[serde(default = "default_method")]
    pub method: String,
    /// Expiration time in seconds (default 3600, max 604800)
    #[serde(default = "default_expires")]
    pub expires_in: u32,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_expires() -> u32 {
    3600
}

/// Pre-signed URL response.
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    pub url: String,
    pub expires_in: u32,
    pub method: String,
}

// === Audit Log Types ===

/// Query parameters for audit log.
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Filter by bucket name.
    pub bucket: Option<String>,
    /// Filter by operation type.
    pub operation: Option<String>,
    /// Filter by principal (user/access key).
    pub principal: Option<String>,
    /// Filter by status (success/error).
    pub status: Option<String>,
    /// Start time for query range (ISO 8601).
    pub start_time: Option<String>,
    /// End time for query range (ISO 8601).
    pub end_time: Option<String>,
    /// Maximum number of results (default 100).
    pub limit: Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}

/// Audit log entry response.
#[derive(Debug, Serialize)]
pub struct AuditLogEntryResponse {
    pub id: String,
    pub timestamp: String,
    pub operation: String,
    pub bucket: Option<String>,
    pub key: Option<String>,
    pub principal: Option<String>,
    pub source_ip: Option<String>,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub duration_ms: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub request_id: String,
}

/// List audit log response.
#[derive(Debug, Serialize)]
pub struct ListAuditLogResponse {
    pub entries: Vec<AuditLogEntryResponse>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

// === Event Notification Types ===

/// List bucket notifications response.
#[derive(Debug, Serialize)]
pub struct ListBucketNotificationsResponse {
    pub rules: Vec<NotificationRuleInfo>,
}

/// Notification rule info.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationRuleInfo {
    pub id: String,
    pub events: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub destination_type: String,
    pub destination_url: String,
}

/// Create notification rule request.
#[derive(Debug, Deserialize)]
pub struct CreateNotificationRuleRequest {
    pub id: Option<String>,
    pub events: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub destination_type: String,
    pub destination_url: String,
}

// === Server Configuration Types ===

/// Server configuration info.
#[derive(Debug, Serialize)]
pub struct ServerConfigInfo {
    /// S3 API endpoint address.
    pub s3_address: String,
    /// Admin console address.
    pub console_address: String,
    /// Metrics endpoint address.
    pub metrics_address: String,
    /// Data directory path.
    pub data_dir: String,
    /// Log level.
    pub log_level: String,
    /// Server region.
    pub region: String,
    /// Storage backend type.
    pub storage_backend: String,
    /// Total disk space (bytes).
    pub disk_total: Option<u64>,
    /// Available disk space (bytes).
    pub disk_available: Option<u64>,
}

// === Identity Provider Types ===

/// List identity providers response.
#[derive(Debug, Serialize)]
pub struct ListIdentityProvidersResponse {
    pub local_enabled: bool,
    pub providers: Vec<IdentityProviderInfo>,
}

/// Identity provider info.
#[derive(Debug, Serialize)]
pub struct IdentityProviderInfo {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub auto_create_users: bool,
}

// === Policy Simulator Types ===

/// Request to simulate policy authorization.
#[derive(Debug, Deserialize)]
pub struct SimulatePolicyRequest {
    /// The username to simulate authorization for.
    pub username: String,
    /// The S3 action to simulate (e.g., "s3:GetObject", "s3:PutObject").
    pub action: String,
    /// The bucket name (optional for bucket-level actions).
    pub bucket: Option<String>,
    /// The object key (optional for object-level actions).
    pub key: Option<String>,
}

/// Response from policy simulation.
#[derive(Debug, Serialize)]
pub struct SimulatePolicyResponse {
    /// Whether the simulated request would be allowed.
    pub allowed: bool,
    /// The effect that determined the outcome.
    pub effect: String,
    /// The policy that made the decision (if any).
    pub matched_policy: Option<String>,
    /// Source of the matching policy.
    pub policy_source: Option<String>,
    /// Human-readable explanation.
    pub explanation: String,
}

// === STS Types ===

/// Request to assume a role and get temporary credentials.
#[derive(Debug, Deserialize)]
pub struct AssumeRoleRequest {
    /// The username to assume.
    pub username: String,
    /// Optional session name for tracking.
    #[serde(default)]
    pub session_name: Option<String>,
    /// Duration in seconds (900-43200, default 3600).
    #[serde(default)]
    pub duration_seconds: Option<u32>,
}

/// Response with temporary credentials.
#[derive(Debug, Serialize)]
pub struct AssumeRoleResponse {
    /// Temporary access key ID (ASIA prefix).
    pub access_key_id: String,
    /// Temporary secret access key.
    pub secret_access_key: String,
    /// Session token (required for API calls).
    pub session_token: String,
    /// When these credentials expire (ISO 8601).
    pub expiration: String,
}
