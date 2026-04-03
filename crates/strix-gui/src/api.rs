//! API client for communicating with the Strix admin API.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// API client for the Strix admin API.
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    /// JWT token for authentication (stored after login).
    token: Arc<RwLock<Option<String>>>,
}

impl ApiClient {
    /// Create a new API client without authentication.
    pub fn new() -> Self {
        Self {
            base_url: "/api/v1".to_string(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new API client with an existing JWT token.
    pub fn new_with_token(token: &str) -> Self {
        let client = Self::new();
        client.set_token(token);
        client
    }

    /// Set the JWT token for authentication.
    pub fn set_token(&self, token: &str) {
        *self.token.write().expect("token lock poisoned") = Some(token.to_string());
    }

    /// Clear the JWT token.
    pub fn clear_token(&self) {
        *self.token.write().expect("token lock poisoned") = None;
    }

    /// Get the current token (if set).
    fn get_token(&self) -> Option<String> {
        self.token.read().expect("token lock poisoned").clone()
    }

    /// Login with access key credentials.
    /// Returns the login response containing the JWT token on success.
    pub async fn login(
        &self,
        access_key_id: &str,
        secret_access_key: &str,
    ) -> Result<LoginResponse, ApiError> {
        let url = format!("{}/login", self.base_url);
        let body = LoginRequest {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
        };

        let request = Request::post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            response
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string()))
        } else {
            // Check for rate limiting
            if response.status() == 429 {
                let rate_limit: RateLimitError = response
                    .json()
                    .await
                    .unwrap_or(RateLimitError {
                        error: "Too many requests".to_string(),
                        retry_after: 60,
                    });
                return Err(ApiError::RateLimited(rate_limit.retry_after));
            }

            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Login with username and password.
    /// Returns the login response containing the JWT token on success.
    pub async fn login_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LoginResponse, ApiError> {
        let url = format!("{}/login/password", self.base_url);
        let body = PasswordLoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };

        let request = Request::post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            response
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string()))
        } else {
            // Check for rate limiting
            if response.status() == 429 {
                let rate_limit: RateLimitError = response
                    .json()
                    .await
                    .unwrap_or(RateLimitError {
                        error: "Too many requests".to_string(),
                        retry_after: 60,
                    });
                return Err(ApiError::RateLimited(rate_limit.retry_after));
            }

            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Make a GET request with authentication.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::get(&url);

        if let Some(token) = self.get_token() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            response
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string()))
        } else if response.status() == 401 {
            Err(ApiError::Unauthorized)
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Make a POST request with JSON body and authentication.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::post(&url).header("Content-Type", "application/json");

        if let Some(token) = self.get_token() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        let request = request
            .body(serde_json::to_string(body).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            response
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string()))
        } else if response.status() == 401 {
            Err(ApiError::Unauthorized)
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Make a POST request without expecting a response body.
    pub async fn post_no_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::post(&url).header("Content-Type", "application/json");

        if let Some(token) = self.get_token() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        let request = request
            .body(serde_json::to_string(body).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            Ok(())
        } else if response.status() == 401 {
            Err(ApiError::Unauthorized)
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Make a DELETE request with authentication.
    pub async fn delete(&self, path: &str) -> Result<(), ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::delete(&url);

        if let Some(token) = self.get_token() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            Ok(())
        } else if response.status() == 401 {
            Err(ApiError::Unauthorized)
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    /// Make a PUT request with JSON body and authentication.
    pub async fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<(), ApiError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = Request::put(&url).header("Content-Type", "application/json");

        if let Some(token) = self.get_token() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        let request = request
            .body(serde_json::to_string(body).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            Ok(())
        } else if response.status() == 401 {
            Err(ApiError::Unauthorized)
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    // === API Methods ===

    /// Get server info.
    pub async fn get_server_info(&self) -> Result<ServerInfo, ApiError> {
        self.get("/info").await
    }

    /// Get server configuration.
    pub async fn get_server_config(&self) -> Result<ServerConfigInfo, ApiError> {
        self.get("/config").await
    }

    /// List all users.
    pub async fn list_users(&self) -> Result<ListUsersResponse, ApiError> {
        let mut resp: ListUsersResponse = self.get("/users").await?;
        if resp.users.is_empty() && !resp.items.is_empty() {
            resp.users = resp.items.clone();
        }
        Ok(resp)
    }

    /// Create a new user.
    pub async fn create_user(&self, username: &str) -> Result<CreateUserResponse, ApiError> {
        self.post("/users", &CreateUserRequest {
            username: username.to_string(),
        })
        .await
    }

    /// Get a user by username.
    pub async fn get_user(&self, username: &str) -> Result<UserInfo, ApiError> {
        self.get(&format!("/users/{}", username)).await
    }

    /// Delete a user.
    pub async fn delete_user(&self, username: &str) -> Result<(), ApiError> {
        self.delete(&format!("/users/{}", username)).await
    }

    /// List access keys for a user.
    pub async fn list_access_keys(&self, username: &str) -> Result<ListAccessKeysResponse, ApiError> {
        let mut resp: ListAccessKeysResponse =
            self.get(&format!("/users/{}/access-keys", username)).await?;
        if resp.access_keys.is_empty() && !resp.items.is_empty() {
            resp.access_keys = resp.items.clone();
        }
        Ok(resp)
    }

    /// Create an access key for a user.
    pub async fn create_access_key(&self, username: &str) -> Result<AccessKeyResponse, ApiError> {
        self.post(&format!("/users/{}/access-keys", username), &())
            .await
    }

    /// Delete an access key.
    pub async fn delete_access_key(&self, access_key_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/access-keys/{}", access_key_id)).await
    }

    /// Get storage usage.
    pub async fn get_storage_usage(&self) -> Result<StorageUsage, ApiError> {
        self.get("/usage").await
    }

    pub async fn get_storage_usage_for_tenant(
        &self,
        tenant_slug: Option<&str>,
    ) -> Result<StorageUsage, ApiError> {
        match tenant_slug {
            Some(slug) if !slug.is_empty() => {
                self.get(&format!("/usage?tenant_slug={}", urlencoding::encode(slug)))
                    .await
            }
            _ => self.get_storage_usage().await,
        }
    }

    // === Group Operations ===

    /// List all groups.
    pub async fn list_groups(&self) -> Result<ListGroupsResponse, ApiError> {
        let mut resp: ListGroupsResponse = self.get("/groups").await?;
        if resp.groups.is_empty() && !resp.items.is_empty() {
            resp.groups = resp.items.clone();
        }
        Ok(resp)
    }

    /// Create a new group.
    pub async fn create_group(&self, name: &str) -> Result<GroupInfo, ApiError> {
        self.post("/groups", &CreateGroupRequest {
            name: name.to_string(),
        })
        .await
    }

    /// Get a group by name.
    pub async fn get_group(&self, name: &str) -> Result<GroupInfo, ApiError> {
        self.get(&format!("/groups/{}", name)).await
    }

    /// Delete a group.
    pub async fn delete_group(&self, name: &str) -> Result<(), ApiError> {
        self.delete(&format!("/groups/{}", name)).await
    }

    /// Add a user to a group.
    pub async fn add_user_to_group(&self, group_name: &str, username: &str) -> Result<(), ApiError> {
        self.post_no_response(
            &format!("/groups/{}/members", group_name),
            &AddUserToGroupRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    /// Remove a user from a group.
    pub async fn remove_user_from_group(&self, group_name: &str, username: &str) -> Result<(), ApiError> {
        self.delete(&format!("/groups/{}/members/{}", group_name, username))
            .await
    }

    /// List groups a user belongs to.
    pub async fn list_user_groups(&self, username: &str) -> Result<ListGroupsResponse, ApiError> {
        let mut resp: ListGroupsResponse = self.get(&format!("/users/{}/groups", username)).await?;
        if resp.groups.is_empty() && !resp.items.is_empty() {
            resp.groups = resp.items.clone();
        }
        Ok(resp)
    }

    // === Managed Policy Operations ===

    /// List all managed policies.
    pub async fn list_policies(&self) -> Result<ListPoliciesResponse, ApiError> {
        let mut resp: ListPoliciesResponse = self.get("/policies").await?;
        if resp.policies.is_empty() && !resp.items.is_empty() {
            resp.policies = resp.items.clone();
        }
        Ok(resp)
    }

    /// Create or update a managed policy.
    pub async fn create_policy(
        &self,
        policy: PolicyDocument,
        description: Option<String>,
    ) -> Result<PolicyInfo, ApiError> {
        self.post("/policies", &CreatePolicyRequest { policy, description })
            .await
    }

    /// Get a managed policy by name.
    pub async fn get_policy(&self, name: &str) -> Result<PolicyInfo, ApiError> {
        self.get(&format!("/policies/{}", name)).await
    }

    /// Delete a managed policy.
    pub async fn delete_policy(&self, name: &str) -> Result<(), ApiError> {
        self.delete(&format!("/policies/{}", name)).await
    }

    // === Bucket Operations ===

    /// List all buckets.
    pub async fn list_buckets(&self) -> Result<ListBucketsResponse, ApiError> {
        let mut resp: ListBucketsResponse = self.get("/buckets").await?;
        if resp.buckets.is_empty() && !resp.items.is_empty() {
            resp.buckets = resp.items.clone();
        }
        Ok(resp)
    }

    pub async fn list_buckets_for_tenant(
        &self,
        tenant_slug: Option<&str>,
    ) -> Result<ListBucketsResponse, ApiError> {
        match tenant_slug {
            Some(slug) if !slug.is_empty() => {
                let mut resp: ListBucketsResponse = self.get(&format!("/buckets?tenant_slug={}", urlencoding::encode(slug)))
                    .await?;
                if resp.buckets.is_empty() && !resp.items.is_empty() {
                    resp.buckets = resp.items.clone();
                }
                Ok(resp)
            }
            _ => self.list_buckets().await,
        }
    }

    /// Create a new bucket.
    pub async fn create_bucket(&self, name: &str) -> Result<BucketInfo, ApiError> {
        self.create_bucket_with_options(name, None, false, false).await
    }

    /// Create a new bucket with options.
    pub async fn create_bucket_with_options(
        &self,
        name: &str,
        tenant_slug: Option<String>,
        versioning: bool,
        object_locking: bool,
    ) -> Result<BucketInfo, ApiError> {
        self.post("/buckets", &CreateBucketRequest {
            name: name.to_string(),
            tenant_slug,
            versioning,
            object_locking,
        })
        .await
    }

    // === Tenant Operations ===

    pub async fn list_tenants(&self) -> Result<ListTenantsResponse, ApiError> {
        self.get("/tenants").await
    }

    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        owner: &str,
        notes: Option<String>,
    ) -> Result<TenantInfo, ApiError> {
        self.post(
            "/tenants",
            &CreateTenantRequest {
                name: name.to_string(),
                slug: slug.to_string(),
                owner: owner.to_string(),
                notes,
            },
        )
        .await
    }

    pub async fn delete_tenant(&self, slug: &str) -> Result<(), ApiError> {
        self.delete(&format!("/tenants/{}", slug)).await
    }

    /// Delete a bucket.
    pub async fn delete_bucket(&self, name: &str) -> Result<(), ApiError> {
        self.delete(&format!("/buckets/{}", name)).await
    }

    // === Object Operations ===

    /// List objects in a bucket.
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
    ) -> Result<ListObjectsResponse, ApiError> {
        let mut url = format!("/buckets/{}/objects", bucket);
        let mut params = Vec::new();

        if let Some(p) = prefix {
            params.push(format!("prefix={}", urlencoding::encode(p)));
        }
        if let Some(d) = delimiter {
            params.push(format!("delimiter={}", urlencoding::encode(d)));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        self.get(&url).await
    }

    /// Delete an object.
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), ApiError> {
        self.delete(&format!(
            "/buckets/{}/objects/{}",
            bucket,
            urlencoding::encode(key)
        ))
        .await
    }

    /// Delete multiple objects.
    pub async fn delete_objects(&self, bucket: &str, keys: Vec<String>) -> Result<(), ApiError> {
        let url = format!("/buckets/{}/objects", bucket);
        let request = Request::delete(&format!("{}{}", self.base_url, url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&DeleteObjectsRequest { keys }).map_err(|e| ApiError::Parse(e.to_string()))?)?;

        let response = request.send().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if response.ok() {
            Ok(())
        } else {
            let error: ErrorResponse = response
                .json()
                .await
                .unwrap_or(ErrorResponse {
                    error: "Unknown error".to_string(),
                    message: None,
                });
            Err(ApiError::Api(error.error))
        }
    }

    // === Pre-signed URLs ===

    /// Generate a pre-signed URL for an object.
    pub async fn presign_url(
        &self,
        bucket: &str,
        key: &str,
        method: &str,
        expires_in: u32,
    ) -> Result<PresignResponse, ApiError> {
        self.post("/presign", &PresignRequest {
            bucket: bucket.to_string(),
            key: key.to_string(),
            method: method.to_string(),
            expires_in,
        })
        .await
    }

    /// Get a download URL for an object.
    pub async fn get_download_url(&self, bucket: &str, key: &str) -> Result<String, ApiError> {
        let response = self.presign_url(bucket, key, "GET", 3600).await?;
        Ok(response.url)
    }

    /// Get an upload URL for an object.
    pub async fn get_upload_url(&self, bucket: &str, key: &str) -> Result<String, ApiError> {
        let response = self.presign_url(bucket, key, "PUT", 3600).await?;
        Ok(response.url)
    }

    // === Bucket Notification Operations ===

    /// Get bucket notifications.
    pub async fn get_bucket_notifications(&self, bucket: &str) -> Result<ListNotificationsResponse, ApiError> {
        self.get(&format!("/buckets/{}/notifications", bucket)).await
    }

    /// Create a bucket notification rule.
    pub async fn create_bucket_notification(
        &self,
        bucket: &str,
        rule: CreateNotificationRuleRequest,
    ) -> Result<NotificationRuleInfo, ApiError> {
        self.post(&format!("/buckets/{}/notifications", bucket), &rule)
            .await
    }

    /// Delete a bucket notification rule.
    pub async fn delete_bucket_notification(&self, bucket: &str, rule_id: &str) -> Result<(), ApiError> {
        self.delete(&format!("/buckets/{}/notifications/{}", bucket, rule_id))
            .await
    }

    // === Audit Log Operations ===

    /// Query audit log entries.
    pub async fn query_audit_log(&self, opts: AuditLogQueryOpts) -> Result<AuditLogResponse, ApiError> {
        let mut params = Vec::new();

        if let Some(bucket) = &opts.bucket {
            params.push(format!("bucket={}", urlencoding::encode(bucket)));
        }
        if let Some(operation) = &opts.operation {
            params.push(format!("operation={}", urlencoding::encode(operation)));
        }
        if let Some(principal) = &opts.principal {
            params.push(format!("principal={}", urlencoding::encode(principal)));
        }
        if let Some(start_time) = &opts.start_time {
            params.push(format!("start_time={}", urlencoding::encode(start_time)));
        }
        if let Some(end_time) = &opts.end_time {
            params.push(format!("end_time={}", urlencoding::encode(end_time)));
        }
        if let Some(limit) = opts.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = opts.offset {
            params.push(format!("offset={}", offset));
        }

        let url = if params.is_empty() {
            "/audit".to_string()
        } else {
            format!("/audit?{}", params.join("&"))
        };

        self.get(&url).await
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// API error types.
#[derive(Debug, Clone)]
pub enum ApiError {
    Network(String),
    Parse(String),
    Api(String),
    /// Authentication failed or token expired.
    Unauthorized,
    /// Rate limited - contains retry_after in seconds.
    RateLimited(u64),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(e) => write!(f, "Network error: {}", e),
            ApiError::Parse(e) => write!(f, "Parse error: {}", e),
            ApiError::Api(e) => write!(f, "{}", e),
            ApiError::Unauthorized => write!(f, "Authentication required"),
            ApiError::RateLimited(secs) => {
                write!(f, "Too many requests. Please try again in {} seconds", secs)
            }
        }
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(e: gloo_net::Error) -> Self {
        ApiError::Network(e.to_string())
    }
}

// === API Types ===

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    #[allow(dead_code)]
    message: Option<String>,
}

/// Rate limit error response from the server.
#[derive(Debug, Deserialize)]
struct RateLimitError {
    #[allow(dead_code)]
    error: String,
    retry_after: u64,
}

/// Login request body.
#[derive(Debug, Serialize)]
struct LoginRequest {
    access_key_id: String,
    secret_access_key: String,
}

/// Password login request body.
#[derive(Debug, Serialize)]
struct PasswordLoginRequest {
    username: String,
    password: String,
}

/// Login response from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    /// JWT session token.
    pub token: String,
    /// Token expiration time (ISO 8601).
    pub expires_at: String,
    /// Authenticated username.
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub commit: Option<String>,
    pub mode: String,
    pub uptime: u64,
    pub region: String,
}

/// Server configuration info.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfigInfo {
    pub s3_address: String,
    pub console_address: String,
    pub metrics_address: String,
    pub data_dir: String,
    pub log_level: String,
    pub region: String,
    pub storage_backend: String,
    pub disk_total: Option<u64>,
    pub disk_available: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CreateUserRequest {
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserResponse {
    pub user: User,
    pub access_key: Option<AccessKey>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub username: String,
    pub arn: String,
    pub created_at: String,
    pub status: String,
    pub is_root: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessKey {
    pub access_key_id: String,
    pub secret_access_key: Option<String>,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListUsersResponse {
    #[serde(default)]
    pub users: Vec<UserInfo>,
    #[serde(default)]
    pub items: Vec<UserInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub arn: String,
    pub created_at: String,
    pub status: String,
    pub policies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListAccessKeysResponse {
    #[serde(default)]
    pub access_keys: Vec<AccessKeyInfo>,
    #[serde(default)]
    pub items: Vec<AccessKeyInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessKeyInfo {
    pub access_key_id: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessKeyResponse {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

// === Group Types ===

#[derive(Debug, Serialize)]
struct CreateGroupRequest {
    name: String,
}

#[derive(Debug, Serialize)]
struct AddUserToGroupRequest {
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListGroupsResponse {
    #[serde(default)]
    pub groups: Vec<GroupInfo>,
    #[serde(default)]
    pub items: Vec<GroupInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub members: Vec<String>,
    pub policies: Vec<String>,
}

// === Policy Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDocument {
    pub name: String,
    #[serde(default = "default_policy_version")]
    pub version: String,
    #[serde(rename = "Statement")]
    pub statements: Vec<PolicyStatement>,
}

fn default_policy_version() -> String {
    "2012-10-17".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatement {
    #[serde(rename = "Effect")]
    pub effect: String,
    #[serde(rename = "Action")]
    pub actions: Vec<String>,
    #[serde(rename = "Resource")]
    pub resources: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CreatePolicyRequest {
    policy: PolicyDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListPoliciesResponse {
    #[serde(default)]
    pub policies: Vec<PolicyInfo>,
    #[serde(default)]
    pub items: Vec<PolicyInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyInfo {
    pub name: String,
    pub version: String,
    pub statements: Vec<PolicyStatement>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageUsage {
    pub buckets: Vec<BucketUsage>,
    pub total_buckets: u64,
    pub total_objects: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BucketUsage {
    pub name: String,
    pub created_at: String,
    pub object_count: u64,
    pub total_size: u64,
}

// === Bucket/Object Types ===

#[derive(Debug, Clone, Deserialize)]
pub struct ListBucketsResponse {
    #[serde(default)]
    pub buckets: Vec<BucketInfo>,
    #[serde(default)]
    pub items: Vec<BucketInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    #[serde(default)]
    pub tenant_slug: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
struct CreateBucketRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_slug: Option<String>,
    #[serde(default)]
    versioning: bool,
    #[serde(default)]
    object_locking: bool,
}

#[derive(Debug, Serialize)]
struct CreateTenantRequest {
    name: String,
    slug: String,
    owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner: String,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListTenantsResponse {
    pub items: Vec<TenantInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListObjectsResponse {
    pub objects: Vec<ObjectInfo>,
    pub prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
    pub etag: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeleteObjectsRequest {
    keys: Vec<String>,
}

// === Pre-signed URL Types ===

#[derive(Debug, Serialize)]
struct PresignRequest {
    bucket: String,
    key: String,
    method: String,
    expires_in: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresignResponse {
    pub url: String,
    pub expires_in: u32,
    pub method: String,
}

// === Bucket Notification Types ===

/// List bucket notifications response.
#[derive(Debug, Clone, Deserialize)]
pub struct ListNotificationsResponse {
    pub rules: Vec<NotificationRuleInfo>,
}

/// Notification rule info.
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationRuleInfo {
    pub id: String,
    pub events: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub destination_type: String,
    pub destination_url: String,
}

/// Create notification rule request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateNotificationRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub events: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    pub destination_type: String,
    pub destination_url: String,
}

// === Audit Log Types ===

/// Options for querying audit logs.
#[derive(Debug, Clone, Default)]
pub struct AuditLogQueryOpts {
    pub bucket: Option<String>,
    pub operation: Option<String>,
    pub principal: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Audit log query response.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditLogEntry>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

/// Audit log entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogEntry {
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
