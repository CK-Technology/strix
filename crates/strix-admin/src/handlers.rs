//! Admin API request handlers.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;

use strix_core::{AuditLogEntry, ObjectStore};
use strix_iam::{AccessKeyStatus, IamProvider, IamStore, UserStatus};
use strix_storage::LocalFsStore;

use crate::auth::{AuthState, CsrfConfig, LoginRequest, LoginResponse, PasswordLoginRequest, RateLimitResponse};
use crate::{ErrorCode, PaginatedResponse, PaginationQuery};

use crate::{
    AccessKeyInfo, AddUserToGroupRequest, AttachPolicyRequest, AuditLogEntryResponse,
    AuditLogQuery, BucketDetailInfo, BucketInfo, BucketUsage, CreateAccessKeyResponse,
    CreateBucketRequest, CreateGroupRequest, CreateNotificationRuleRequest, CreatePolicyRequest,
    CreateTenantRequest, CreateUserRequest, CreateUserResponse, DeleteObjectsRequest,
    ErrorResponse, GetVersioningResponse, GroupInfo, ListAuditLogResponse,
    ListBucketNotificationsResponse, ListObjectsResponse, NotificationRuleInfo, ObjectInfo,
    PolicyInfo, PresignRequest, PresignResponse, ServerConfigInfo, ServerInfo,
    SetVersioningRequest, StorageUsageResponse, TenantFilterQuery, TenantInfoResponse,
    UpdateAccessKeyRequest, UserInfo,
};

fn bucket_visible_for_tenant(
    bucket_tenant_slug: Option<&str>,
    filter_tenant_slug: Option<&str>,
) -> bool {
    match filter_tenant_slug {
        Some(filter) => bucket_tenant_slug == Some(filter),
        None => true,
    }
}

async fn ensure_bucket_belongs_to_tenant(
    state: &Arc<AdminState>,
    bucket: &str,
    tenant_slug: Option<&str>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if tenant_slug.is_none() {
        return Ok(());
    }

    use strix_core::types::ObjectStore;
    let info = state.storage.head_bucket(bucket).await.map_err(|e| {
        let status =
            if e.to_string().contains("not found") || e.to_string().contains("NoSuchBucket") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
        (status, Json(ErrorResponse::new(e.to_string())))
    })?;

    if !bucket_visible_for_tenant(info.tenant_slug.as_deref(), tenant_slug) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Bucket is outside requested tenant scope",
            )),
        ));
    }

    Ok(())
}

/// Configuration for presigned URL generation.
#[derive(Clone)]
pub struct PresignConfig {
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
    pub region: String,
}

/// Server configuration info (safe to expose via API).
#[derive(Clone, Default)]
pub struct ServerConfig {
    pub s3_address: String,
    pub console_address: String,
    pub metrics_address: String,
    pub data_dir: String,
    pub log_level: String,
    pub region: String,
}

/// Shared state for admin handlers.
pub struct AdminState {
    pub iam: Arc<IamStore>,
    pub storage: Arc<LocalFsStore>,
    pub start_time: Instant,
    pub version: String,
    pub presign_config: Option<PresignConfig>,
    pub server_config: ServerConfig,
    pub auth: AuthState,
    pub csrf: CsrfConfig,
}

impl AdminState {
    pub fn new(iam: Arc<IamStore>, storage: Arc<LocalFsStore>) -> Self {
        Self {
            iam,
            storage,
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            presign_config: None,
            server_config: ServerConfig::default(),
            auth: AuthState::new(),
            csrf: CsrfConfig::default(),
        }
    }

    /// Set presign configuration.
    pub fn with_presign(mut self, config: PresignConfig) -> Self {
        self.presign_config = Some(config);
        self
    }

    /// Set server configuration.
    pub fn with_server_config(mut self, config: ServerConfig) -> Self {
        self.server_config = config;
        self
    }

    /// Set auth state.
    pub fn with_auth(mut self, auth: AuthState) -> Self {
        self.auth = auth;
        self
    }

    /// Set CSRF configuration.
    pub fn with_csrf(mut self, csrf: CsrfConfig) -> Self {
        self.csrf = csrf;
        self
    }
}

/// Middleware to emit structured audit entries for authenticated admin API calls.
pub async fn audit_middleware(
    State(state): State<Arc<AdminState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let principal = req
        .extensions()
        .get::<crate::auth::AuthenticatedUser>()
        .map(|u| u.username.clone());

    let source_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.split(',').map(str::trim).find(|s| !s.is_empty()))
        .map(ToString::to_string)
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .map(ToString::to_string)
        })
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|addr| addr.0.ip().to_string())
        });

    let request_id = req
        .headers()
        .get("x-request-id")
        .or_else(|| req.headers().get("x-amz-request-id"))
        .and_then(|h| h.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut response = next.run(req).await;
    response.headers_mut().insert(
        header::HeaderName::from_static("x-request-id"),
        header::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| header::HeaderValue::from_static("invalid-request-id")),
    );

    let status = response.status();

    let entry = AuditLogEntry {
        id: request_id.clone(),
        timestamp: Utc::now(),
        operation: format!("Admin:{} {}", method, path),
        bucket: None,
        key: None,
        principal,
        source_ip,
        status_code: status.as_u16(),
        error_code: if status.is_success() {
            None
        } else {
            Some(status.as_str().to_string())
        },
        duration_ms: Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64),
        bytes_sent: None,
        request_id,
    };

    if let Err(e) = state.storage.log_audit_event(entry).await {
        tracing::warn!("Failed to log admin audit event: {}", e);
    }

    response
}

// === Server Info ===

pub async fn get_server_info(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    Json(ServerInfo {
        version: state.version.clone(),
        commit: option_env!("GIT_COMMIT").map(String::from),
        mode: "standalone".to_string(),
        uptime,
        region: "us-east-1".to_string(),
    })
}

pub async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

// === Authentication ===

/// Login handler with rate limiting.
pub async fn login(
    State(state): State<Arc<AdminState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let ip = addr.ip();

    // Check rate limiting
    if state.auth.rate_limiter.is_limited(&ip) {
        let retry_after = state.auth.rate_limiter.lockout_remaining(&ip).unwrap_or(60);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(RateLimitResponse {
                error: "Too many failed login attempts. Please try again later.".to_string(),
                retry_after,
            }),
        )
            .into_response();
    }

    // Look up credentials
    match state.iam.get_credentials(&req.access_key_id).await {
        Ok(Some((access_key, user))) => {
            // Verify the secret matches
            if access_key.secret_access_key.as_deref() == Some(&req.secret_access_key) {
                // Successful login - clear rate limit and generate token
                state.auth.rate_limiter.clear(&ip);

                // Update last used timestamp
                let _ = state
                    .iam
                    .update_access_key_last_used(&req.access_key_id)
                    .await;

                match state
                    .auth
                    .session_config
                    .create_token(&user.username, &access_key.access_key_id, user.is_root)
                {
                    Ok(token) => {
                        let expires_at = Utc::now()
                            + chrono::Duration::seconds(
                                state.auth.session_config.expiry.as_secs() as i64
                            );

                        (
                            StatusCode::OK,
                            Json(LoginResponse {
                                token,
                                expires_at: expires_at.to_rfc3339(),
                                username: user.username,
                            }),
                        )
                            .into_response()
                    }
                    Err(e) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(format!(
                            "Failed to create session: {}",
                            e
                        ))),
                    )
                        .into_response(),
                }
            } else {
                // Wrong secret
                state.auth.rate_limiter.record_failure(&ip);
                (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse::new("Invalid credentials")),
                )
                    .into_response()
            }
        }
        Ok(None) => {
            // Access key not found
            state.auth.rate_limiter.record_failure(&ip);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Invalid credentials")),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Authentication error: {}", e))),
        )
            .into_response(),
    }
}

/// Password login handler with rate limiting.
///
/// Authenticates users with username/password (for console login).
pub async fn login_with_password(
    State(state): State<Arc<AdminState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<PasswordLoginRequest>,
) -> impl IntoResponse {
    let ip = addr.ip();

    // Check rate limiting
    if state.auth.rate_limiter.is_limited(&ip) {
        let retry_after = state.auth.rate_limiter.lockout_remaining(&ip).unwrap_or(60);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(RateLimitResponse {
                error: "Too many failed login attempts. Please try again later.".to_string(),
                retry_after,
            }),
        )
            .into_response();
    }

    // Verify username/password
    match state.iam.verify_user_password(&req.username, &req.password).await {
        Ok(true) => {
            // Password verified - check user exists and is active
            match state.iam.get_user(&req.username).await {
                Ok(user) => {
                    if user.status != UserStatus::Active {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(ErrorResponse::new("User account is not active")),
                        )
                            .into_response();
                    }

                    // Successful login - clear rate limit and generate token
                    state.auth.rate_limiter.clear(&ip);

                    // Create token with username (no access key for password login)
                    match state
                        .auth
                        .session_config
                        .create_token(&user.username, "password-auth", user.is_root)
                    {
                        Ok(token) => {
                            let expires_at = Utc::now()
                                + chrono::Duration::seconds(
                                    state.auth.session_config.expiry.as_secs() as i64
                                );

                            (
                                StatusCode::OK,
                                Json(LoginResponse {
                                    token,
                                    expires_at: expires_at.to_rfc3339(),
                                    username: user.username,
                                }),
                            )
                                .into_response()
                        }
                        Err(e) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new(format!(
                                "Failed to create session: {}",
                                e
                            ))),
                        )
                            .into_response(),
                    }
                }
                Err(_) => {
                    // User not found (shouldn't happen if password verified)
                    state.auth.rate_limiter.record_failure(&ip);
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse::new("Invalid credentials")),
                    )
                        .into_response()
                }
            }
        }
        Ok(false) => {
            // Wrong password
            state.auth.rate_limiter.record_failure(&ip);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Invalid credentials")),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Authentication error: {}", e))),
        )
            .into_response(),
    }
}

pub async fn get_server_config(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    let cfg = &state.server_config;

    // Try to get disk usage info
    let (disk_total, disk_available) = get_disk_usage(&cfg.data_dir);

    Json(ServerConfigInfo {
        s3_address: cfg.s3_address.clone(),
        console_address: cfg.console_address.clone(),
        metrics_address: cfg.metrics_address.clone(),
        data_dir: cfg.data_dir.clone(),
        log_level: cfg.log_level.clone(),
        region: cfg.region.clone(),
        storage_backend: "localfs".to_string(),
        disk_total,
        disk_available,
    })
}

fn get_disk_usage(_path: &str) -> (Option<u64>, Option<u64>) {
    // Disk usage would require the libc crate
    // For now, return None - the GUI can handle missing disk info gracefully
    (None, None)
}

// === User Management ===

pub async fn list_users(
    State(state): State<Arc<AdminState>>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_users().await {
        Ok(users) => {
            let total = users.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            // Apply pagination
            let paginated: Vec<UserInfo> = users
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(UserInfo::from)
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_code(
                ErrorCode::InternalError,
                e.to_string(),
            )),
        )
            .into_response(),
    }
}

pub async fn create_user(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    match state.iam.create_user(&req.username).await {
        Ok(user) => {
            // Also create an initial access key
            let access_key = state.iam.create_access_key(&req.username).await.ok();

            (
                StatusCode::CREATED,
                Json(CreateUserResponse { user, access_key }),
            )
                .into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserExists(_) => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn get_user(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match state.iam.get_user(&username).await {
        Ok(user) => {
            let policies = state
                .iam
                .list_user_policies(&username)
                .await
                .unwrap_or_default();
            let mut info = UserInfo::from(user);
            info.policies = policies.iter().map(|p| p.name.clone()).collect();
            (StatusCode::OK, Json(info)).into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_user(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match state.iam.delete_user(&username).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::CannotDeleteRoot => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn update_user_status(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
    Json(req): Json<UpdateAccessKeyRequest>,
) -> impl IntoResponse {
    let status = match req.status.parse::<UserStatus>() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Invalid status: {}", e))),
            )
                .into_response();
        }
    };

    match state.iam.update_user_status(&username, status).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Access Key Management ===

pub async fn list_access_keys(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_access_keys(&username).await {
        Ok(keys) => {
            let total = keys.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<AccessKeyInfo> = keys
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(AccessKeyInfo::from)
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

pub async fn create_access_key(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match state.iam.create_access_key(&username).await {
        Ok(key) => (
            StatusCode::CREATED,
            Json(CreateAccessKeyResponse::from(key)),
        )
            .into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::MaxAccessKeysExceeded => StatusCode::CONFLICT,
                strix_iam::IamError::CannotModifyRootKeys => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_access_key(
    State(state): State<Arc<AdminState>>,
    Path(access_key_id): Path<String>,
) -> impl IntoResponse {
    match state.iam.delete_access_key(&access_key_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::AccessKeyNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::CannotModifyRootKeys => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn update_access_key(
    State(state): State<Arc<AdminState>>,
    Path(access_key_id): Path<String>,
    Json(req): Json<UpdateAccessKeyRequest>,
) -> impl IntoResponse {
    let status = match req.status.parse::<AccessKeyStatus>() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Invalid status: {}", e))),
            )
                .into_response();
        }
    };

    match state
        .iam
        .update_access_key_status(&access_key_id, status)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::AccessKeyNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::CannotModifyRootKeys => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Policy Management ===

pub async fn list_user_policies(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_user_policies(&username).await {
        Ok(policies) => {
            let total = policies.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<_> = policies.into_iter().skip(offset).take(limit).collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

pub async fn attach_user_policy(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
    Json(req): Json<AttachPolicyRequest>,
) -> impl IntoResponse {
    match state.iam.attach_user_policy(&username, &req.policy).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::InvalidPolicy(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn detach_user_policy(
    State(state): State<Arc<AdminState>>,
    Path((username, policy_name)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.iam.detach_user_policy(&username, &policy_name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::PolicyNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Storage Usage ===

pub async fn get_storage_usage(
    State(state): State<Arc<AdminState>>,
    Query(filter): Query<TenantFilterQuery>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    let buckets = match state.storage.list_buckets().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };

    let mut bucket_usages = Vec::new();
    let mut total_objects: u64 = 0;
    let mut total_size: u64 = 0;

    for bucket in &buckets {
        if !bucket_visible_for_tenant(bucket.tenant_slug.as_deref(), filter.tenant_slug.as_deref())
        {
            continue;
        }

        let (count, size) = state
            .storage
            .get_bucket_usage(&bucket.name)
            .await
            .unwrap_or((0, 0));

        bucket_usages.push(BucketUsage {
            name: bucket.name.clone(),
            created_at: bucket.created_at.to_rfc3339(),
            object_count: count,
            total_size: size,
        });

        total_objects += count;
        total_size += size;
    }

    (
        StatusCode::OK,
        Json(StorageUsageResponse {
            buckets: bucket_usages,
            total_buckets: buckets.len() as u64,
            total_objects,
            total_size,
        }),
    )
        .into_response()
}

// === Bucket Management ===

pub async fn list_buckets(
    State(state): State<Arc<AdminState>>,
    Query(filter): Query<TenantFilterQuery>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;
    pagination.validate();

    match state.storage.list_buckets().await {
        Ok(buckets) => {
            let buckets: Vec<_> = buckets
                .into_iter()
                .filter(|b| {
                    bucket_visible_for_tenant(
                        b.tenant_slug.as_deref(),
                        filter.tenant_slug.as_deref(),
                    )
                })
                .collect();

            let total = buckets.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<BucketInfo> = buckets
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|b| BucketInfo {
                    name: b.name,
                    tenant_slug: b.tenant_slug,
                    created_at: b.created_at.to_rfc3339(),
                })
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_code(
                ErrorCode::InternalError,
                e.to_string(),
            )),
        )
            .into_response(),
    }
}

pub async fn create_bucket(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreateBucketRequest>,
) -> impl IntoResponse {
    use strix_core::types::{CreateBucketOpts, ObjectStore};

    // Create the bucket first
    if let Err(e) = state
        .storage
        .create_bucket(
            &req.name,
            CreateBucketOpts {
                region: None,
                tenant_slug: req.tenant_slug.clone(),
            },
        )
        .await
    {
        let status = if e.to_string().contains("already exists") {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (status, Json(ErrorResponse::new(e.to_string()))).into_response();
    }

    // Enable versioning if requested
    if req.versioning || req.object_locking {
        // Object locking requires versioning
        if let Err(e) = state.storage.set_bucket_versioning(&req.name, true).await {
            tracing::warn!(
                "Failed to enable versioning on new bucket {}: {}",
                req.name,
                e
            );
        }
    }

    // Enable object locking if requested
    if req.object_locking {
        use strix_core::types::ObjectLockConfiguration;
        let config = ObjectLockConfiguration {
            enabled: true,
            rule: None,
        };
        if let Err(e) = state
            .storage
            .put_object_lock_configuration(&req.name, config)
            .await
        {
            tracing::warn!(
                "Failed to enable object locking on new bucket {}: {}",
                req.name,
                e
            );
        }
    }

    (
        StatusCode::CREATED,
        Json(BucketInfo {
            name: req.name,
            tenant_slug: req.tenant_slug,
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
    )
        .into_response()
}

// === Tenant Management ===

pub async fn list_tenants(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state.storage.list_tenants().await {
        Ok(items) => {
            let out: Vec<TenantInfoResponse> = items
                .into_iter()
                .map(|t| TenantInfoResponse {
                    id: t.id,
                    name: t.name,
                    slug: t.slug,
                    owner: t.owner,
                    notes: t.notes,
                    created_at: t.created_at.to_rfc3339(),
                })
                .collect();
            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: out.clone(),
                    total: Some(out.len() as u64),
                    count: out.len() as u32,
                    offset: 0,
                    has_more: false,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

pub async fn create_tenant(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreateTenantRequest>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state
        .storage
        .create_tenant(&req.name, &req.slug, &req.owner, req.notes.as_deref())
        .await
    {
        Ok(t) => (
            StatusCode::CREATED,
            Json(TenantInfoResponse {
                id: t.id,
                name: t.name,
                slug: t.slug,
                owner: t.owner,
                notes: t.notes,
                created_at: t.created_at.to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => {
            let status = match e {
                strix_core::Error::TenantAlreadyExists(_) => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_tenant(
    State(state): State<Arc<AdminState>>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state.storage.delete_tenant(&slug).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match e {
                strix_core::Error::TenantNotFound(_) => StatusCode::NOT_FOUND,
                strix_core::Error::InvalidArgument(_) => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_bucket(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
    Query(filter): Query<TenantFilterQuery>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    if let Err(err) =
        ensure_bucket_belongs_to_tenant(&state, &name, filter.tenant_slug.as_deref()).await
    {
        return err.into_response();
    }

    match state.storage.delete_bucket(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status =
                if e.to_string().contains("not found") || e.to_string().contains("NoSuchBucket") {
                    StatusCode::NOT_FOUND
                } else if e.to_string().contains("not empty") {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn get_bucket(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
    Query(filter): Query<TenantFilterQuery>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state.storage.head_bucket(&name).await {
        Ok(info) => {
            if !bucket_visible_for_tenant(
                info.tenant_slug.as_deref(),
                filter.tenant_slug.as_deref(),
            ) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::new(
                        "Bucket is outside requested tenant scope",
                    )),
                )
                    .into_response();
            }

            let detail = BucketDetailInfo {
                name: info.name,
                created_at: info.created_at.to_rfc3339(),
                versioning_enabled: info.versioning_enabled,
                object_locking_enabled: info.object_locking_enabled,
            };
            (StatusCode::OK, Json(detail)).into_response()
        }
        Err(e) => {
            let status =
                if e.to_string().contains("not found") || e.to_string().contains("NoSuchBucket") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn get_bucket_versioning(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state.storage.get_bucket_versioning(&name).await {
        Ok(status) => {
            let status_str = match status {
                Some(true) => Some("Enabled".to_string()),
                Some(false) => Some("Suspended".to_string()),
                None => None,
            };
            (
                StatusCode::OK,
                Json(GetVersioningResponse { status: status_str }),
            )
                .into_response()
        }
        Err(e) => {
            let status =
                if e.to_string().contains("not found") || e.to_string().contains("NoSuchBucket") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn set_bucket_versioning(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
    Json(req): Json<SetVersioningRequest>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state
        .storage
        .set_bucket_versioning(&name, req.enabled)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status =
                if e.to_string().contains("not found") || e.to_string().contains("NoSuchBucket") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Object Management ===

#[derive(Debug, serde::Deserialize)]
pub struct ListObjectsQuery {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<i32>,
    pub continuation_token: Option<String>,
}

pub async fn list_objects(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
    Query(filter): Query<TenantFilterQuery>,
    axum::extract::Query(query): axum::extract::Query<ListObjectsQuery>,
) -> impl IntoResponse {
    use strix_core::types::{ListObjectsOpts, ObjectStore};

    if let Err(err) =
        ensure_bucket_belongs_to_tenant(&state, &bucket, filter.tenant_slug.as_deref()).await
    {
        return err.into_response();
    }

    let opts = ListObjectsOpts {
        prefix: query.prefix,
        delimiter: query.delimiter,
        max_keys: query.max_keys.map(|k| k as u32),
        continuation_token: query.continuation_token,
        ..Default::default()
    };

    match state.storage.list_objects(&bucket, opts).await {
        Ok(response) => {
            let objects: Vec<ObjectInfo> = response
                .objects
                .into_iter()
                .map(|o| ObjectInfo {
                    key: o.key,
                    size: o.size,
                    last_modified: o.last_modified.to_rfc3339(),
                    etag: Some(o.etag),
                })
                .collect();

            (
                StatusCode::OK,
                Json(ListObjectsResponse {
                    objects,
                    prefixes: response.common_prefixes,
                    is_truncated: response.is_truncated,
                    next_continuation_token: response.next_continuation_token,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let status = if e.to_string().contains("NoSuchBucket") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_object(
    State(state): State<Arc<AdminState>>,
    Path((bucket, key)): Path<(String, String)>,
    Query(filter): Query<TenantFilterQuery>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    if let Err(err) =
        ensure_bucket_belongs_to_tenant(&state, &bucket, filter.tenant_slug.as_deref()).await
    {
        return err.into_response();
    }

    // URL decode the key
    let key = urlencoding::decode(&key)
        .map(|s| s.into_owned())
        .unwrap_or(key);

    match state.storage.delete_object(&bucket, &key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status =
                if e.to_string().contains("NoSuchBucket") || e.to_string().contains("NoSuchKey") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_objects(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
    Query(filter): Query<TenantFilterQuery>,
    Json(req): Json<DeleteObjectsRequest>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    if let Err(err) =
        ensure_bucket_belongs_to_tenant(&state, &bucket, filter.tenant_slug.as_deref()).await
    {
        return err.into_response();
    }

    let mut errors = Vec::new();

    for key in req.keys {
        if let Err(e) = state.storage.delete_object(&bucket, &key).await {
            errors.push(format!("{}: {}", key, e));
        }
    }

    if errors.is_empty() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::MULTI_STATUS,
            Json(ErrorResponse::with_message(
                "Some objects failed to delete",
                errors.join("; "),
            )),
        )
            .into_response()
    }
}

// === Bucket Policies ===

pub async fn get_bucket_policy(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    use crate::BucketPolicyResponse;

    match state.iam.get_bucket_policy(&bucket).await {
        Ok(Some(policy)) => (StatusCode::OK, Json(BucketPolicyResponse { policy })).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(format!(
                "No policy for bucket '{}'",
                bucket
            ))),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

pub async fn set_bucket_policy(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
    Json(req): Json<strix_iam::BucketPolicy>,
) -> impl IntoResponse {
    match state.iam.set_bucket_policy(&bucket, &req).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::InvalidPolicy(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_bucket_policy(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    match state.iam.delete_bucket_policy(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::PolicyNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Group Management ===

pub async fn list_groups(
    State(state): State<Arc<AdminState>>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_groups().await {
        Ok(groups) => {
            let total = groups.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<GroupInfo> = groups
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(GroupInfo::from)
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_code(
                ErrorCode::InternalError,
                e.to_string(),
            )),
        )
            .into_response(),
    }
}

pub async fn create_group(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    match state.iam.create_group(&req.name).await {
        Ok(group) => (StatusCode::CREATED, Json(GroupInfo::from(group))).into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupExists(_) => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn get_group(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.iam.get_group(&name).await {
        Ok(group) => (StatusCode::OK, Json(GroupInfo::from(group))).into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_group(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.iam.delete_group(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn add_user_to_group(
    State(state): State<Arc<AdminState>>,
    Path(group_name): Path<String>,
    Json(req): Json<AddUserToGroupRequest>,
) -> impl IntoResponse {
    match state
        .iam
        .add_user_to_group(&group_name, &req.username)
        .await
    {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn remove_user_from_group(
    State(state): State<Arc<AdminState>>,
    Path((group_name, username)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .iam
        .remove_user_from_group(&group_name, &username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn list_group_policies(
    State(state): State<Arc<AdminState>>,
    Path(group_name): Path<String>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_group_policies(&group_name).await {
        Ok(policies) => {
            let total = policies.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<_> = policies.into_iter().skip(offset).take(limit).collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn attach_group_policy(
    State(state): State<Arc<AdminState>>,
    Path(group_name): Path<String>,
    Json(req): Json<AttachPolicyRequest>,
) -> impl IntoResponse {
    match state
        .iam
        .attach_group_policy(&group_name, &req.policy)
        .await
    {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::InvalidPolicy(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn detach_group_policy(
    State(state): State<Arc<AdminState>>,
    Path((group_name, policy_name)): Path<(String, String)>,
) -> impl IntoResponse {
    match state
        .iam
        .detach_group_policy(&group_name, &policy_name)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::GroupNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::PolicyNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn list_user_groups(
    State(state): State<Arc<AdminState>>,
    Path(username): Path<String>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_user_groups(&username).await {
        Ok(groups) => {
            let total = groups.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<GroupInfo> = groups
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(GroupInfo::from)
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Managed Policy Operations ===

pub async fn list_managed_policies(
    State(state): State<Arc<AdminState>>,
    Query(mut pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    pagination.validate();

    match state.iam.list_policies().await {
        Ok(policies) => {
            let total = policies.len() as u64;
            let offset = pagination.offset as usize;
            let limit = pagination.limit as usize;

            let paginated: Vec<PolicyInfo> = policies
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|(p, desc)| PolicyInfo {
                    name: p.name,
                    version: p.version,
                    statements: p.statements,
                    description: desc,
                })
                .collect();

            let count = paginated.len() as u32;
            let has_more = (offset + count as usize) < (total as usize);

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    items: paginated,
                    total: Some(total),
                    count,
                    offset: pagination.offset,
                    has_more,
                    next_marker: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::with_code(
                ErrorCode::InternalError,
                e.to_string(),
            )),
        )
            .into_response(),
    }
}

pub async fn create_managed_policy(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<CreatePolicyRequest>,
) -> impl IntoResponse {
    match state
        .iam
        .create_policy(&req.policy, req.description.as_deref())
        .await
    {
        Ok(()) => {
            let info = PolicyInfo {
                name: req.policy.name,
                version: req.policy.version,
                statements: req.policy.statements,
                description: req.description,
            };
            (StatusCode::CREATED, Json(info)).into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::InvalidPolicy(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn get_managed_policy(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.iam.get_policy(&name).await {
        Ok(policy) => {
            let info = PolicyInfo {
                name: policy.name,
                version: policy.version,
                statements: policy.statements,
                description: None, // Would need to query separately
            };
            (StatusCode::OK, Json(info)).into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::PolicyNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_managed_policy(
    State(state): State<Arc<AdminState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.iam.delete_policy(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::PolicyNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// === Pre-signed URLs ===

pub async fn generate_presign_url(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<PresignRequest>,
) -> impl IntoResponse {
    use strix_s3::{PresignMethod, PresignOptions, PresignUrlGenerator};

    // Validate presign config exists
    let config = match &state.presign_config {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new("Pre-signed URLs not configured")),
            )
                .into_response();
        }
    };

    // Parse method
    let method = match req.method.to_uppercase().as_str() {
        "GET" => PresignMethod::Get,
        "PUT" => PresignMethod::Put,
        "DELETE" => PresignMethod::Delete,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "Invalid method. Use GET, PUT, or DELETE",
                )),
            )
                .into_response();
        }
    };

    // Validate expiration (max 7 days)
    let expires_in = req.expires_in.min(604800);

    // Generate the URL
    let generator = PresignUrlGenerator::new(
        config.access_key.clone(),
        config.secret_key.clone(),
        config.endpoint.clone(),
        Some(config.region.clone()),
    );

    let url = generator.generate(&PresignOptions {
        method,
        bucket: req.bucket,
        key: req.key,
        expires_in,
        content_type: None,
        region: Some(config.region.clone()),
    });

    (
        StatusCode::OK,
        Json(PresignResponse {
            url,
            expires_in,
            method: req.method.to_uppercase(),
        }),
    )
        .into_response()
}

// === Audit Log ===

pub async fn query_audit_log(
    State(state): State<Arc<AdminState>>,
    axum::extract::Query(query): axum::extract::Query<AuditLogQuery>,
) -> impl IntoResponse {
    use strix_core::types::{AuditQueryOpts, ObjectStore};

    // Parse time filters
    let start_time = query.start_time.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let end_time = query.end_time.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let opts = AuditQueryOpts {
        bucket: query.bucket,
        key_prefix: None,
        operation: query.operation,
        principal: query.principal,
        start_time,
        end_time,
        limit: Some(limit),
        offset: Some(offset),
    };

    match state.storage.query_audit_log(opts).await {
        Ok(entries) => {
            let response_entries: Vec<AuditLogEntryResponse> = entries
                .into_iter()
                .map(|e| AuditLogEntryResponse {
                    id: e.id,
                    timestamp: e.timestamp.to_rfc3339(),
                    operation: e.operation,
                    bucket: e.bucket,
                    key: e.key,
                    principal: e.principal,
                    source_ip: e.source_ip,
                    status_code: e.status_code,
                    error_code: e.error_code,
                    duration_ms: e.duration_ms,
                    bytes_sent: e.bytes_sent,
                    request_id: e.request_id,
                })
                .collect();

            let total = response_entries.len() as u64;

            (
                StatusCode::OK,
                Json(ListAuditLogResponse {
                    entries: response_entries,
                    total,
                    limit,
                    offset,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

// === Bucket Notifications ===

pub async fn get_bucket_notifications(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    match state.storage.get_bucket_notification(&bucket).await {
        Ok(Some(config)) => {
            let rules: Vec<NotificationRuleInfo> = config
                .rules
                .into_iter()
                .map(|r| {
                    let (dest_type, dest_url) = match &r.destination {
                        strix_core::NotificationDestination::Webhook { url } => {
                            ("webhook".to_string(), url.clone())
                        }
                        strix_core::NotificationDestination::Amqp { url, .. } => {
                            ("amqp".to_string(), url.clone())
                        }
                        strix_core::NotificationDestination::Kafka { brokers, topic } => (
                            "kafka".to_string(),
                            format!("{}:{}", brokers.join(","), topic),
                        ),
                        strix_core::NotificationDestination::Redis { url, .. } => {
                            ("redis".to_string(), url.clone())
                        }
                    };
                    NotificationRuleInfo {
                        id: r.id,
                        events: r.events.iter().map(|e| e.to_string()).collect(),
                        prefix: r.filter.prefix,
                        suffix: r.filter.suffix,
                        destination_type: dest_type,
                        destination_url: dest_url,
                    }
                })
                .collect();

            (
                StatusCode::OK,
                Json(ListBucketNotificationsResponse { rules }),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::OK,
            Json(ListBucketNotificationsResponse { rules: vec![] }),
        )
            .into_response(),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn create_bucket_notification(
    State(state): State<Arc<AdminState>>,
    Path(bucket): Path<String>,
    Json(req): Json<CreateNotificationRuleRequest>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;
    use strix_core::{NotificationDestination, NotificationFilter, NotificationRule, S3EventType};

    // Parse events
    let events: Vec<S3EventType> = req
        .events
        .iter()
        .filter_map(|e| match e.as_str() {
            "s3:ObjectCreated:*" => Some(S3EventType::ObjectCreatedAll),
            "s3:ObjectCreated:Put" => Some(S3EventType::ObjectCreatedPut),
            "s3:ObjectCreated:Post" => Some(S3EventType::ObjectCreatedPost),
            "s3:ObjectCreated:Copy" => Some(S3EventType::ObjectCreatedCopy),
            "s3:ObjectCreated:CompleteMultipartUpload" => {
                Some(S3EventType::ObjectCreatedCompleteMultipartUpload)
            }
            "s3:ObjectRemoved:*" => Some(S3EventType::ObjectRemovedAll),
            "s3:ObjectRemoved:Delete" => Some(S3EventType::ObjectRemovedDelete),
            "s3:ObjectRemoved:DeleteMarkerCreated" => {
                Some(S3EventType::ObjectRemovedDeleteMarkerCreated)
            }
            _ => None,
        })
        .collect();

    if events.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("No valid events specified")),
        )
            .into_response();
    }

    // Parse destination
    let destination = match req.destination_type.as_str() {
        "webhook" => NotificationDestination::Webhook {
            url: req.destination_url.clone(),
        },
        "amqp" => NotificationDestination::Amqp {
            url: req.destination_url.clone(),
            exchange: "strix".to_string(),
            routing_key: "events".to_string(),
        },
        "kafka" => {
            // Expected format: brokers:topic
            let parts: Vec<&str> = req.destination_url.split(':').collect();
            if parts.len() < 2 {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        "Kafka destination should be 'brokers:topic'",
                    )),
                )
                    .into_response();
            }
            // Safe: we've already verified parts.len() >= 2 above
            let topic = parts.last().expect("checked above").to_string();
            let brokers: Vec<String> = parts[..parts.len() - 1]
                .join(":")
                .split(',')
                .map(|s| s.to_string())
                .collect();
            NotificationDestination::Kafka { brokers, topic }
        }
        "redis" => NotificationDestination::Redis {
            url: req.destination_url.clone(),
            channel: "strix:events".to_string(),
        },
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "Invalid destination type. Use 'webhook', 'amqp', 'kafka', or 'redis'",
                )),
            )
                .into_response();
        }
    };

    let rule_id = req.id.unwrap_or_else(|| ulid::Ulid::new().to_string());
    let prefix = req.prefix.clone();
    let suffix = req.suffix.clone();

    let new_rule = NotificationRule {
        id: rule_id.clone(),
        events,
        filter: NotificationFilter {
            prefix: req.prefix,
            suffix: req.suffix,
        },
        destination,
    };

    // Get existing config and add new rule
    let mut config = state
        .storage
        .get_bucket_notification(&bucket)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    config.rules.push(new_rule);

    // Save updated config
    match state.storage.put_bucket_notification(&bucket, config).await {
        Ok(()) => {
            let info = NotificationRuleInfo {
                id: rule_id,
                events: req.events,
                prefix,
                suffix,
                destination_type: req.destination_type,
                destination_url: req.destination_url,
            };
            (StatusCode::CREATED, Json(info)).into_response()
        }
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

pub async fn delete_bucket_notification(
    State(state): State<Arc<AdminState>>,
    Path((bucket, rule_id)): Path<(String, String)>,
) -> impl IntoResponse {
    use strix_core::types::ObjectStore;

    // Get existing config
    let mut config = match state.storage.get_bucket_notification(&bucket).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("No notification rules configured")),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };

    // Remove the rule
    let original_len = config.rules.len();
    config.rules.retain(|r| r.id != rule_id);

    if config.rules.len() == original_len {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(format!("Rule '{}' not found", rule_id))),
        )
            .into_response();
    }

    // Save updated config
    match state.storage.put_bucket_notification(&bucket, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

// === Policy Simulator ===

pub async fn simulate_policy(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<crate::SimulatePolicyRequest>,
) -> impl IntoResponse {
    use strix_iam::{Action, AuthorizationEffect, PolicySource, Resource};

    // Parse action
    let action = match Action::from_operation(&req.action) {
        Some(a) => a,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Invalid action: {}",
                    req.action
                ))),
            )
                .into_response();
        }
    };

    // Build resource
    let resource = Resource {
        bucket: req.bucket.clone(),
        key: req.key.clone(),
    };

    // Run detailed authorization check
    match state
        .iam
        .is_authorized_detailed(&req.username, &action, &resource)
        .await
    {
        Ok(result) => {
            let effect_str = match result.effect {
                AuthorizationEffect::ExplicitAllow => "ExplicitAllow",
                AuthorizationEffect::ExplicitDeny => "ExplicitDeny",
                AuthorizationEffect::ImplicitDeny => "ImplicitDeny",
                AuthorizationEffect::RootAccess => "RootAccess",
            };

            let source_str = result.policy_source.as_ref().map(|s| match s {
                PolicySource::User(u) => format!("user:{}", u),
                PolicySource::Group(g) => format!("group:{}", g),
                PolicySource::Bucket(b) => format!("bucket:{}", b),
            });

            let explanation = match result.effect {
                AuthorizationEffect::ExplicitAllow => {
                    format!(
                        "Request is ALLOWED by policy '{}' from {}",
                        result.matched_policy.as_deref().unwrap_or("unknown"),
                        source_str.as_deref().unwrap_or("unknown source")
                    )
                }
                AuthorizationEffect::ExplicitDeny => {
                    format!(
                        "Request is DENIED by explicit deny in policy '{}' from {}",
                        result.matched_policy.as_deref().unwrap_or("unknown"),
                        source_str.as_deref().unwrap_or("unknown source")
                    )
                }
                AuthorizationEffect::ImplicitDeny => {
                    "Request is DENIED because no policy explicitly allows this action".to_string()
                }
                AuthorizationEffect::RootAccess => {
                    "Request is ALLOWED because root user has full access".to_string()
                }
            };

            (
                StatusCode::OK,
                Json(crate::SimulatePolicyResponse {
                    allowed: result.allowed,
                    effect: effect_str.to_string(),
                    matched_policy: result.matched_policy,
                    policy_source: source_str,
                    explanation,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}

// ============================================================================
// STS Operations
// ============================================================================

/// Assume a role and get temporary credentials.
pub async fn assume_role(
    State(state): State<Arc<AdminState>>,
    Json(req): Json<crate::AssumeRoleRequest>,
) -> impl IntoResponse {
    use strix_iam::AssumeRoleRequest as IamRequest;

    let iam_request = IamRequest {
        username: req.username.clone(),
        session_name: req.session_name,
        duration_seconds: req.duration_seconds,
    };

    match state.iam.assume_role(iam_request).await {
        Ok(cred) => (
            StatusCode::OK,
            Json(crate::AssumeRoleResponse {
                access_key_id: cred.access_key_id,
                secret_access_key: cred.secret_access_key.unwrap_or_default(),
                session_token: cred.session_token,
                expiration: cred.expiration.to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => {
            let status = match &e {
                strix_iam::IamError::UserNotFound(_) => StatusCode::NOT_FOUND,
                strix_iam::IamError::InvalidCredentials => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        }
    }
}
