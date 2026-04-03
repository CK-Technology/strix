//! Authentication, rate limiting, and CSRF protection for the admin API.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration as ChronoDuration, Utc};
use dashmap::DashMap;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use strix_iam::{Action, IamProvider, Resource};

use crate::ErrorResponse;

/// JWT claims for session tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (username).
    pub sub: String,
    /// Access key ID used for authentication.
    pub access_key_id: String,
    /// Whether this is the root user.
    #[serde(default)]
    pub is_root: bool,
    /// Issued at (Unix timestamp).
    pub iat: i64,
    /// Expiration (Unix timestamp).
    pub exp: i64,
}

/// Session token configuration.
#[derive(Clone)]
pub struct SessionConfig {
    /// JWT secret key.
    secret: Arc<[u8; 32]>,
    /// Token expiration duration.
    pub expiry: Duration,
}

impl SessionConfig {
    /// Create a new session config with a random secret.
    pub fn new(expiry: Duration) -> Self {
        use rand::Rng;
        let mut secret = [0u8; 32];
        rand::rng().fill(&mut secret);
        Self {
            secret: Arc::new(secret),
            expiry,
        }
    }

    /// Create a new session config with a specific secret.
    pub fn with_secret(secret: [u8; 32], expiry: Duration) -> Self {
        Self {
            secret: Arc::new(secret),
            expiry,
        }
    }

    /// Generate a session token for a user.
    pub fn create_token(&self, username: &str, access_key_id: &str, is_root: bool) -> Result<String, String> {
        let now = Utc::now();
        let exp = now + ChronoDuration::seconds(self.expiry.as_secs() as i64);

        let claims = Claims {
            sub: username.to_string(),
            access_key_id: access_key_id.to_string(),
            is_root,
            iat: now.timestamp(),
            exp: exp.timestamp(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_ref()),
        )
        .map_err(|e| e.to_string())
    }

    /// Verify and decode a session token.
    pub fn verify_token(&self, token: &str) -> Result<Claims, String> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_ref()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| e.to_string())
    }
}

/// Rate limiter for protecting against brute-force attacks.
pub struct RateLimiter {
    /// Map of IP address -> (attempt count, first attempt time).
    attempts: DashMap<IpAddr, (u32, Instant)>,
    /// Maximum attempts allowed in the window.
    max_attempts: u32,
    /// Time window for rate limiting.
    window: Duration,
    /// Lockout duration after max attempts exceeded.
    lockout: Duration,
    /// Last cleanup time.
    last_cleanup: RwLock<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_attempts: u32, window: Duration, lockout: Duration) -> Self {
        Self {
            attempts: DashMap::new(),
            max_attempts,
            window,
            lockout,
            last_cleanup: RwLock::new(Instant::now()),
        }
    }

    /// Create a rate limiter with default settings (5 attempts per minute, 15 minute lockout).
    pub fn default_login() -> Self {
        Self::new(5, Duration::from_secs(60), Duration::from_secs(15 * 60))
    }

    /// Check if an IP is rate limited. Returns true if the request should be blocked.
    pub fn is_limited(&self, ip: &IpAddr) -> bool {
        self.cleanup_if_needed();

        if let Some(entry) = self.attempts.get(ip) {
            let (count, first_attempt) = *entry;
            let elapsed = first_attempt.elapsed();

            // If we're past the lockout period, allow
            if count >= self.max_attempts && elapsed < self.lockout {
                return true;
            }

            // If we're past the window, this will be reset on next record
            false
        } else {
            false
        }
    }

    /// Record a failed attempt from an IP.
    pub fn record_failure(&self, ip: &IpAddr) {
        let now = Instant::now();

        self.attempts
            .entry(*ip)
            .and_modify(|(count, first_attempt)| {
                // Reset if window expired
                if first_attempt.elapsed() > self.window {
                    *count = 1;
                    *first_attempt = now;
                } else {
                    *count += 1;
                }
            })
            .or_insert((1, now));
    }

    /// Clear attempts for an IP (on successful login).
    pub fn clear(&self, ip: &IpAddr) {
        self.attempts.remove(ip);
    }

    /// Get remaining attempts for an IP.
    pub fn remaining_attempts(&self, ip: &IpAddr) -> u32 {
        if let Some(entry) = self.attempts.get(ip) {
            let (count, first_attempt) = *entry;
            if first_attempt.elapsed() > self.window {
                return self.max_attempts;
            }
            self.max_attempts.saturating_sub(count)
        } else {
            self.max_attempts
        }
    }

    /// Get lockout remaining time in seconds (if locked out).
    pub fn lockout_remaining(&self, ip: &IpAddr) -> Option<u64> {
        if let Some(entry) = self.attempts.get(ip) {
            let (count, first_attempt) = *entry;
            if count >= self.max_attempts {
                let elapsed = first_attempt.elapsed();
                if elapsed < self.lockout {
                    return Some((self.lockout - elapsed).as_secs());
                }
            }
        }
        None
    }

    /// Cleanup old entries periodically.
    fn cleanup_if_needed(&self) {
        let now = Instant::now();
        let mut last = self.last_cleanup.write();

        // Only cleanup every 5 minutes
        if now.duration_since(*last) > Duration::from_secs(300) {
            self.attempts
                .retain(|_, (_, first_attempt)| first_attempt.elapsed() < self.lockout);
            *last = now;
        }
    }
}

/// Authentication state shared across handlers.
#[derive(Clone)]
pub struct AuthState {
    pub session_config: SessionConfig,
    pub rate_limiter: Arc<RateLimiter>,
}

impl AuthState {
    /// Create a new auth state with default settings.
    pub fn new() -> Self {
        Self {
            session_config: SessionConfig::new(Duration::from_secs(3600)), // 1 hour sessions
            rate_limiter: Arc::new(RateLimiter::default_login()),
        }
    }

    /// Create auth state with custom session expiry.
    pub fn with_expiry(expiry: Duration) -> Self {
        Self {
            session_config: SessionConfig::new(expiry),
            rate_limiter: Arc::new(RateLimiter::default_login()),
        }
    }

    /// Create auth state with a stable JWT secret and custom session expiry.
    pub fn with_secret(secret: [u8; 32], expiry: Duration) -> Self {
        Self {
            session_config: SessionConfig::with_secret(secret, expiry),
            rate_limiter: Arc::new(RateLimiter::default_login()),
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// CSRF protection configuration.
#[derive(Clone)]
pub struct CsrfConfig {
    /// Allowed origins for cross-origin requests.
    allowed_origins: Arc<HashSet<String>>,
    /// Whether to enforce CSRF checks (can be disabled for development).
    enabled: bool,
}

impl CsrfConfig {
    /// Create a new CSRF config with allowed origins.
    pub fn new(allowed_origins: Vec<String>) -> Self {
        Self {
            allowed_origins: Arc::new(allowed_origins.into_iter().collect()),
            enabled: true,
        }
    }

    /// Create a CSRF config that allows a single origin.
    pub fn single_origin(origin: &str) -> Self {
        Self::new(vec![origin.to_string()])
    }

    /// Create a CSRF config that's disabled (for development).
    pub fn disabled() -> Self {
        Self {
            allowed_origins: Arc::new(HashSet::new()),
            enabled: false,
        }
    }

    /// Add an allowed origin.
    pub fn with_origin(mut self, origin: &str) -> Self {
        Arc::make_mut(&mut self.allowed_origins).insert(origin.to_string());
        self
    }

    /// Check if an origin is allowed.
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        if !self.enabled {
            return true;
        }
        self.allowed_origins.contains(origin)
    }
}

impl Default for CsrfConfig {
    fn default() -> Self {
        // By default, allow localhost origins for development
        Self::new(vec![
            "http://localhost:9001".to_string(),
            "http://127.0.0.1:9001".to_string(),
        ])
    }
}

/// CSRF protection middleware.
///
/// Validates that state-changing requests (POST, PUT, DELETE, PATCH) come from
/// allowed origins by checking the Origin or Referer header.
pub async fn csrf_middleware(
    State(csrf_config): State<CsrfConfig>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Skip CSRF check for safe methods
    if matches!(
        req.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    ) {
        return next.run(req).await;
    }

    // Skip if CSRF is disabled
    if !csrf_config.enabled {
        return next.run(req).await;
    }

    // Check Origin header first, fall back to Referer
    let origin: Option<String> = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get(header::REFERER)
                .and_then(|h| h.to_str().ok())
                .and_then(|referer| {
                    // Extract origin from referer URL
                    url::Url::parse(referer).ok().map(|u| {
                        format!(
                            "{}://{}{}",
                            u.scheme(),
                            u.host_str().unwrap_or(""),
                            u.port().map(|p| format!(":{}", p)).unwrap_or_default()
                        )
                    })
                })
        });

    match origin {
        Some(ref origin) if csrf_config.is_origin_allowed(origin) => next.run(req).await,
        Some(ref origin) => {
            tracing::warn!("CSRF check failed: origin '{}' not allowed", origin);
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("Cross-origin request not allowed")),
            )
                .into_response()
        }
        None => {
            // No origin header - could be same-origin or non-browser client
            // For API clients, we rely on the Authorization header check
            // which can't be forged by cross-origin requests
            next.run(req).await
        }
    }
}

/// Authenticated user info extracted from JWT.
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
    pub access_key_id: String,
    pub is_root: bool,
}

/// Authentication middleware for protected routes.
pub async fn auth_middleware(
    State(auth): State<AuthState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Extract token from Authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new(
                    "Missing or invalid authorization header",
                )),
            )
                .into_response();
        }
    };

    // Verify token
    match auth.session_config.verify_token(token) {
        Ok(claims) => {
            // Add authenticated user to request extensions
            req.extensions_mut().insert(AuthenticatedUser {
                username: claims.sub,
                access_key_id: claims.access_key_id,
                is_root: claims.is_root,
            });
            next.run(req).await
        }
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid or expired token")),
        )
            .into_response(),
    }
}

/// Authorization middleware for protected admin routes.
///
/// Root users bypass IAM checks. Non-root users require IAM permission
/// based on a route-to-action mapping.
pub async fn authorize_middleware(
    State(iam): State<Arc<dyn IamProvider>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let Some(user) = req.extensions().get::<AuthenticatedUser>().cloned() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Authentication required")),
        )
            .into_response();
    };

    if user.is_root {
        return next.run(req).await;
    }

    let (action, resource) = authorization_target(req.method(), req.uri().path());

    let allowed = match iam.is_authorized(&user.username, &action, &resource).await {
        Ok(allowed) => allowed,
        Err(e) => {
            tracing::warn!(
                "Admin authorization lookup failed for '{}': {}",
                user.username,
                e
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Authorization check failed")),
            )
                .into_response();
        }
    };

    if !allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Insufficient permissions for admin operation",
            )),
        )
            .into_response();
    }

    next.run(req).await
}

fn authorization_target(method: &Method, path: &str) -> (Action, Resource) {
    let p = path.strip_prefix("/api/v1").unwrap_or(path);
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();

    // Route set without resource context.
    if parts.is_empty() || parts[0] == "usage" {
        return (Action::ListAllMyBuckets, Resource::all());
    }

    if parts[0] == "tenants" {
        return match *method {
            Method::GET => (Action::ListAllMyBuckets, Resource::all()),
            Method::POST | Method::PUT | Method::DELETE => (Action::All, Resource::all()),
            _ => (Action::All, Resource::all()),
        };
    }

    if parts[0] != "buckets" {
        // Non-bucket admin control-plane routes require broad admin privileges.
        return (Action::All, Resource::all());
    }

    // /buckets
    if parts.len() == 1 {
        return match *method {
            Method::GET | Method::HEAD => (Action::ListAllMyBuckets, Resource::all()),
            Method::POST => (Action::CreateBucket, Resource::all()),
            _ => (Action::All, Resource::all()),
        };
    }

    let bucket = parts[1].to_string();

    // /buckets/{bucket}
    if parts.len() == 2 {
        return match *method {
            Method::GET | Method::HEAD => (Action::HeadBucket, Resource::bucket(bucket)),
            Method::DELETE => (Action::DeleteBucket, Resource::bucket(bucket)),
            _ => (Action::All, Resource::bucket(bucket)),
        };
    }

    // /buckets/{bucket}/versioning
    if parts[2] == "versioning" {
        return match *method {
            Method::GET => (Action::GetBucketVersioning, Resource::bucket(bucket)),
            Method::PUT => (Action::PutBucketVersioning, Resource::bucket(bucket)),
            _ => (Action::All, Resource::bucket(bucket)),
        };
    }

    // /buckets/{bucket}/objects and /buckets/{bucket}/objects/{key...}
    if parts[2] == "objects" {
        if parts.len() == 3 {
            return match *method {
                Method::GET => (Action::ListBucket, Resource::bucket(bucket)),
                Method::DELETE => (Action::DeleteObject, Resource::bucket(bucket)),
                _ => (Action::All, Resource::bucket(bucket)),
            };
        }

        let key = parts[3..].join("/");
        return match *method {
            Method::DELETE => (Action::DeleteObject, Resource::object(bucket, key)),
            Method::GET => (Action::GetObject, Resource::object(bucket, key)),
            Method::PUT | Method::POST => (Action::PutObject, Resource::object(bucket, key)),
            _ => (Action::All, Resource::object(bucket, key)),
        };
    }

    // bucket policy/notification/presign and other control APIs currently require admin-level access.
    (Action::All, Resource::bucket(bucket))
}

/// Login request body (access key authentication).
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Access key ID.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
}

/// Password login request body (username/password authentication).
#[derive(Debug, Deserialize)]
pub struct PasswordLoginRequest {
    /// Username.
    pub username: String,
    /// Password.
    pub password: String,
}

/// Login response body.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Session token (JWT).
    pub token: String,
    /// Token expiration time (ISO 8601).
    pub expires_at: String,
    /// Authenticated username.
    pub username: String,
}

/// Rate limit error response.
#[derive(Debug, Serialize)]
pub struct RateLimitResponse {
    pub error: String,
    pub retry_after: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60), Duration::from_secs(300));
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // First few attempts should not be limited
        assert!(!limiter.is_limited(&ip));
        limiter.record_failure(&ip);
        assert!(!limiter.is_limited(&ip));
        limiter.record_failure(&ip);
        assert!(!limiter.is_limited(&ip));
        limiter.record_failure(&ip);

        // After max attempts, should be limited
        assert!(limiter.is_limited(&ip));
        assert!(limiter.lockout_remaining(&ip).is_some());

        // Clear should reset
        limiter.clear(&ip);
        assert!(!limiter.is_limited(&ip));
    }

    #[test]
    fn test_session_token() {
        let config = SessionConfig::new(Duration::from_secs(3600));

        let token = config.create_token("testuser", "AKIATEST123").unwrap();
        let claims = config.verify_token(&token).unwrap();

        assert_eq!(claims.sub, "testuser");
        assert_eq!(claims.access_key_id, "AKIATEST123");
    }

    #[test]
    fn test_authorization_target_bucket_object_route() {
        let (action, resource) = authorization_target(
            &Method::DELETE,
            "/api/v1/buckets/example-bucket/objects/path/to/object.txt",
        );

        assert_eq!(action, Action::DeleteObject);
        assert_eq!(
            resource,
            Resource::object("example-bucket", "path/to/object.txt")
        );
    }

    #[test]
    fn test_authorization_target_bucket_versioning_route() {
        let (action, resource) =
            authorization_target(&Method::PUT, "/api/v1/buckets/example/versioning");

        assert_eq!(action, Action::PutBucketVersioning);
        assert_eq!(resource, Resource::bucket("example"));
    }

    #[test]
    fn test_authorization_target_control_plane_defaults_to_admin() {
        let (action, resource) = authorization_target(&Method::POST, "/api/v1/users");

        assert_eq!(action, Action::All);
        assert_eq!(resource, Resource::all());
    }
}
