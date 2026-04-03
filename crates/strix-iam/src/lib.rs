//! IAM (Identity and Access Management) for Strix.
//!
//! Provides user management, access keys, policy enforcement, and identity provider support.

mod error;
pub mod idp;
mod password;
mod policy;
mod secrets;
mod store;
mod types;

pub use error::{IamError, Result};
pub use idp::{
    IdentityProviderConfig, IdentityProviderType, LoginMethod, OidcAuthResult, OidcClaims,
    OidcConfig, OidcTokenResponse, Session,
};
pub use password::{hash_password, verify_password};
pub use policy::{
    Action, ActionSpec, BucketPolicy, BucketPolicyStatement, ConditionContext, Effect, ParsedArn,
    Policy, PolicyStatement, PolicyValidationError, Principal, PrincipalSpec, Resource,
    ResourceSpec, evaluate_condition, validate_bucket_policy, validate_policy,
};
pub use secrets::{decrypt_secret, derive_encryption_key, encrypt_secret};
pub use store::IamStore;
pub use types::{
    AccessKey, AccessKeyStatus, AssumeRoleRequest, Group, TemporaryCredential, User, UserStatus,
};

use async_trait::async_trait;
use serde::Serialize;

/// Result of detailed authorization check (for policy simulation).
#[derive(Debug, Clone, Serialize)]
pub struct AuthorizationResult {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// The effect that determined the outcome.
    pub effect: AuthorizationEffect,
    /// The policy that made the decision (if any).
    pub matched_policy: Option<String>,
    /// The statement within the policy (if applicable).
    pub matched_statement: Option<String>,
    /// Source of the policy (user, group name, or bucket).
    pub policy_source: Option<PolicySource>,
}

/// The effect that determined authorization.
#[derive(Debug, Clone, Serialize)]
pub enum AuthorizationEffect {
    /// Explicitly allowed by a policy statement.
    ExplicitAllow,
    /// Explicitly denied by a policy statement.
    ExplicitDeny,
    /// Implicitly denied (no matching allow statement).
    ImplicitDeny,
    /// Root user has full access.
    RootAccess,
}

/// Source of the policy that matched.
#[derive(Debug, Clone, Serialize)]
pub enum PolicySource {
    /// User-attached policy.
    User(String),
    /// Group-attached policy.
    Group(String),
    /// Bucket policy.
    Bucket(String),
}

/// Trait for IAM operations.
#[async_trait]
pub trait IamProvider: Send + Sync {
    // === User Operations ===

    /// Create a new user.
    async fn create_user(&self, username: &str) -> Result<User>;

    /// Delete a user and all associated access keys.
    async fn delete_user(&self, username: &str) -> Result<()>;

    /// Get a user by username.
    async fn get_user(&self, username: &str) -> Result<User>;

    /// List all users.
    async fn list_users(&self) -> Result<Vec<User>>;

    /// Update user status.
    async fn update_user_status(&self, username: &str, status: UserStatus) -> Result<()>;

    /// Set a user's password (console login).
    async fn set_user_password(&self, username: &str, password: &str) -> Result<()>;

    /// Verify a user's password. Returns Ok(true) if valid, Ok(false) if invalid.
    async fn verify_user_password(&self, username: &str, password: &str) -> Result<bool>;

    // === Access Key Operations ===

    /// Create a new access key for a user.
    async fn create_access_key(&self, username: &str) -> Result<AccessKey>;

    /// Delete an access key.
    async fn delete_access_key(&self, access_key_id: &str) -> Result<()>;

    /// List access keys for a user.
    async fn list_access_keys(&self, username: &str) -> Result<Vec<AccessKey>>;

    /// Get access key by ID.
    async fn get_access_key(&self, access_key_id: &str) -> Result<AccessKey>;

    /// Update access key status.
    async fn update_access_key_status(
        &self,
        access_key_id: &str,
        status: AccessKeyStatus,
    ) -> Result<()>;

    /// Look up credentials for authentication.
    async fn get_credentials(&self, access_key_id: &str) -> Result<Option<(AccessKey, User)>>;

    /// Update the last_used timestamp for an access key.
    async fn update_access_key_last_used(&self, access_key_id: &str) -> Result<()>;

    // === Policy Operations ===

    /// Attach a policy to a user.
    async fn attach_user_policy(&self, username: &str, policy: &Policy) -> Result<()>;

    /// Detach a policy from a user.
    async fn detach_user_policy(&self, username: &str, policy_name: &str) -> Result<()>;

    /// List policies attached to a user.
    async fn list_user_policies(&self, username: &str) -> Result<Vec<Policy>>;

    /// Check if a user is authorized to perform an action on a resource.
    async fn is_authorized(
        &self,
        username: &str,
        action: &Action,
        resource: &Resource,
    ) -> Result<bool>;

    /// Check authorization with detailed result (for policy simulation).
    async fn is_authorized_detailed(
        &self,
        username: &str,
        action: &Action,
        resource: &Resource,
    ) -> Result<AuthorizationResult>;

    // === Group Operations ===

    /// Create a new group.
    async fn create_group(&self, name: &str) -> Result<Group>;

    /// Delete a group.
    async fn delete_group(&self, name: &str) -> Result<()>;

    /// Get a group by name.
    async fn get_group(&self, name: &str) -> Result<Group>;

    /// List all groups.
    async fn list_groups(&self) -> Result<Vec<Group>>;

    /// Add a user to a group.
    async fn add_user_to_group(&self, group_name: &str, username: &str) -> Result<()>;

    /// Remove a user from a group.
    async fn remove_user_from_group(&self, group_name: &str, username: &str) -> Result<()>;

    /// Attach a policy to a group.
    async fn attach_group_policy(&self, group_name: &str, policy: &Policy) -> Result<()>;

    /// Detach a policy from a group.
    async fn detach_group_policy(&self, group_name: &str, policy_name: &str) -> Result<()>;

    /// List policies attached to a group.
    async fn list_group_policies(&self, group_name: &str) -> Result<Vec<Policy>>;

    /// List groups a user belongs to.
    async fn list_user_groups(&self, username: &str) -> Result<Vec<Group>>;

    // === Standalone Policy Operations ===

    /// Create or update a managed policy.
    async fn create_policy(&self, policy: &Policy, description: Option<&str>) -> Result<()>;

    /// Delete a managed policy.
    async fn delete_policy(&self, policy_name: &str) -> Result<()>;

    /// Get a managed policy by name.
    async fn get_policy(&self, policy_name: &str) -> Result<Policy>;

    /// List all managed policies.
    async fn list_policies(&self) -> Result<Vec<(Policy, Option<String>)>>;

    // === Bucket Policy Operations ===

    /// Set a bucket policy.
    async fn set_bucket_policy(&self, bucket: &str, policy: &BucketPolicy) -> Result<()>;

    /// Get a bucket policy.
    async fn get_bucket_policy(&self, bucket: &str) -> Result<Option<BucketPolicy>>;

    /// Delete a bucket policy.
    async fn delete_bucket_policy(&self, bucket: &str) -> Result<()>;

    /// Check if a request is authorized by bucket policy.
    async fn is_authorized_by_bucket_policy(
        &self,
        bucket: &str,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
    ) -> Result<Option<Effect>>;

    // === STS Operations ===

    /// Assume a role and get temporary credentials.
    async fn assume_role(&self, request: AssumeRoleRequest) -> Result<TemporaryCredential>;

    /// Get temporary credentials by access key ID.
    /// Returns the credentials and associated user if valid and not expired.
    async fn get_temp_credentials(
        &self,
        access_key_id: &str,
    ) -> Result<Option<(TemporaryCredential, User)>>;

    /// Validate a session token. Returns true if the token is valid and not expired.
    async fn validate_session_token(
        &self,
        access_key_id: &str,
        session_token: &str,
    ) -> Result<bool>;
}

/// Generate a random access key ID (20 characters, like AWS).
pub fn generate_access_key_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..20)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a random secret access key (40 characters, like AWS).
pub fn generate_secret_key() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut rng = rand::rng();
    (0..40)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a random session token (256 characters for STS temporary credentials).
pub fn generate_session_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
    let mut rng = rand::rng();
    (0..256)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a temporary access key ID (prefixed with ASIA like AWS).
pub fn generate_temp_access_key_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let suffix: String = (0..16)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    format!("ASIA{}", suffix)
}
