//! OIDC login endpoints for the admin/console API.
//!
//! Implements the public side of the OAuth2 Authorization Code flow:
//! - `GET /login/oidc/{provider_id}` starts the flow (redirect to IdP).
//! - `GET /auth/callback` handles the IdP redirect, verifies the ID token,
//!   resolves/provisions the user, mints a session JWT, and redirects the
//!   browser back to the console with the token in the URL fragment.
//! - `GET /auth/providers` lists enabled providers for login-page buttons.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use strix_iam::{IamProvider, OidcClient, OidcConfig};

use crate::auth::AuthenticatedUser;
use crate::handlers::AdminState;
use crate::ErrorResponse;

/// How long an in-flight OIDC authorization request remains valid.
const FLOW_TTL: Duration = Duration::from_secs(600);

/// In-flight authorization request state, keyed by the opaque `state` value.
struct FlowState {
    nonce: String,
    provider_id: String,
    created: Instant,
}

/// Shared OIDC runtime state held on `AdminState`.
///
/// Providers are persisted (encrypted) in the IAM database and read per-request
/// for hot-reload; this struct only holds the in-flight authorization flows and
/// the shared HTTP/OIDC client.
pub struct OidcState {
    /// In-flight authorization flows keyed by `state`.
    flows: DashMap<String, FlowState>,
    /// Shared HTTP/OIDC client.
    client: OidcClient,
}

impl Default for OidcState {
    fn default() -> Self {
        Self::new()
    }
}

impl OidcState {
    /// Create a new OIDC runtime state.
    pub fn new() -> Self {
        Self {
            flows: DashMap::new(),
            client: OidcClient::new(),
        }
    }

    /// Remove expired in-flight flows.
    fn sweep_expired(&self) {
        self.flows.retain(|_, f| f.created.elapsed() < FLOW_TTL);
    }
}

/// Public provider info for rendering login buttons.
#[derive(Debug, Serialize)]
pub struct AuthProviderInfo {
    pub id: String,
    pub name: String,
    pub provider_type: String,
}

/// Response for `GET /auth/providers`.
#[derive(Debug, Serialize)]
pub struct AuthProvidersResponse {
    pub providers: Vec<AuthProviderInfo>,
}

/// Generate a URL-safe random token for `state`/`nonce`.
fn random_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Build a 302 redirect response to the given location.
fn redirect_to(location: String) -> Response {
    (
        StatusCode::FOUND,
        [(header::LOCATION, location)],
        axum::body::Body::empty(),
    )
        .into_response()
}

/// Redirect the browser back to the console login page with an SSO error.
fn redirect_error(message: &str) -> Response {
    redirect_to(format!(
        "/login?sso_error={}",
        urlencoding::encode(message)
    ))
}

/// `GET /auth/providers` — list enabled providers for login buttons.
pub async fn list_auth_providers(State(state): State<Arc<AdminState>>) -> Response {
    let providers = match state.iam.list_oidc_providers().await {
        Ok(list) => list
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| AuthProviderInfo {
                id: p.id,
                name: p.name,
                provider_type: "oidc".to_string(),
            })
            .collect(),
        Err(e) => {
            tracing::warn!("Failed to list OIDC providers: {}", e);
            Vec::new()
        }
    };
    Json(AuthProvidersResponse { providers }).into_response()
}

/// Fetch an enabled provider from the IAM store, or `None` if missing/disabled.
async fn enabled_provider(state: &Arc<AdminState>, id: &str) -> Option<OidcConfig> {
    match state.iam.get_oidc_provider(id).await {
        Ok(Some(p)) if p.enabled => Some(p),
        _ => None,
    }
}

/// `GET /login/oidc/{provider_id}` — start the OIDC authorization code flow.
pub async fn login_oidc(
    State(state): State<Arc<AdminState>>,
    Path(provider_id): Path<String>,
) -> Response {
    let oidc = &state.oidc;
    oidc.sweep_expired();

    let Some(config) = enabled_provider(&state, &provider_id).await else {
        return redirect_error("Unknown or disabled SSO provider");
    };

    let discovery = match oidc.client.discover(&config.issuer_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("OIDC discovery failed for '{}': {}", provider_id, e);
            return redirect_error("SSO provider discovery failed");
        }
    };

    let csrf_state = random_token();
    let nonce = random_token();
    oidc.flows.insert(
        csrf_state.clone(),
        FlowState {
            nonce: nonce.clone(),
            provider_id: provider_id.clone(),
            created: Instant::now(),
        },
    );

    let auth_url = oidc
        .client
        .authorization_url(&discovery, &config, &csrf_state, &nonce);
    redirect_to(auth_url)
}

/// Query parameters for the OAuth callback.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
}

/// `GET /auth/callback` — handle the IdP redirect and complete login.
pub async fn auth_callback(
    State(state): State<Arc<AdminState>>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let oidc = &state.oidc;

    if let Some(err) = query.error {
        let desc = query.error_description.unwrap_or_default();
        tracing::warn!("OIDC provider returned error: {} {}", err, desc);
        return redirect_error("SSO provider rejected the login");
    }

    let (Some(code), Some(csrf_state)) = (query.code, query.state) else {
        return redirect_error("Malformed SSO callback");
    };

    // Consume the flow (single-use) and validate it.
    let Some((_, flow)) = oidc.flows.remove(&csrf_state) else {
        return redirect_error("Invalid or expired SSO state");
    };
    if flow.created.elapsed() >= FLOW_TTL {
        return redirect_error("SSO session expired, please retry");
    }

    let Some(config) = enabled_provider(&state, &flow.provider_id).await else {
        return redirect_error("SSO provider is no longer available");
    };

    let discovery = match oidc.client.discover(&config.issuer_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("OIDC discovery failed during callback: {}", e);
            return redirect_error("SSO provider discovery failed");
        }
    };

    let auth = match oidc
        .client
        .exchange_and_verify(&config, &discovery, &code, &flow.nonce)
        .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("OIDC token exchange/verification failed: {}", e);
            return redirect_error("SSO token verification failed");
        }
    };

    // Resolve or auto-provision the user.
    let username = auth.username.clone();
    if state.iam.get_user(&username).await.is_err() {
        if !config.auto_create_users {
            tracing::warn!("OIDC user '{}' not provisioned and auto-create disabled", username);
            return redirect_error("Your account is not provisioned for SSO");
        }
        if let Err(e) = state.iam.create_user(&username).await {
            tracing::error!("Failed to auto-provision OIDC user '{}': {}", username, e);
            return redirect_error("Failed to provision SSO account");
        }
        if let Some(policy_name) = &config.default_policy
            && let Ok(policy) = state.iam.get_policy(policy_name).await
            && let Err(e) = state.iam.attach_user_policy(&username, &policy).await
        {
            tracing::warn!(
                "Failed to attach default policy '{}' to '{}': {}",
                policy_name,
                username,
                e
            );
        }
    }

    // Mint a console session token (non-root).
    let token = match state
        .auth
        .session_config
        .create_token(&username, "oidc-auth", false)
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to mint session token for '{}': {}", username, e);
            return redirect_error("Failed to establish session");
        }
    };

    // Deliver the token to the SPA via URL fragment (preserves SessionStorage flow).
    redirect_to(format!(
        "/#token={}&user={}",
        urlencoding::encode(&token),
        urlencoding::encode(&username),
    ))
}

// === Admin provider management (root-only) ===

/// Public (secret-free) view of a configured OIDC provider.
#[derive(Debug, Serialize)]
pub struct OidcProviderResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    /// Whether a client secret is stored (the secret itself is never returned).
    pub has_client_secret: bool,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub username_claim: String,
    pub groups_claim: Option<String>,
    pub auto_create_users: bool,
    pub default_policy: Option<String>,
}

impl From<OidcConfig> for OidcProviderResponse {
    fn from(c: OidcConfig) -> Self {
        Self {
            id: c.id,
            name: c.name,
            enabled: c.enabled,
            issuer_url: c.issuer_url,
            client_id: c.client_id,
            has_client_secret: !c.client_secret.is_empty(),
            redirect_uri: c.redirect_uri,
            scopes: c.scopes,
            username_claim: c.username_claim,
            groups_claim: c.groups_claim,
            auto_create_users: c.auto_create_users,
            default_policy: c.default_policy,
        }
    }
}

/// Create/update request body for an OIDC provider.
///
/// On update, an empty/omitted `client_secret` preserves the stored secret.
#[derive(Debug, Deserialize)]
pub struct OidcProviderRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_username_claim")]
    pub username_claim: String,
    #[serde(default)]
    pub groups_claim: Option<String>,
    #[serde(default = "default_true")]
    pub auto_create_users: bool,
    #[serde(default)]
    pub default_policy: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

fn default_username_claim() -> String {
    "preferred_username".to_string()
}

impl OidcProviderRequest {
    /// Build an `OidcConfig` from this request for the given provider id.
    fn into_config(self, id: String) -> OidcConfig {
        OidcConfig {
            id,
            name: self.name,
            enabled: self.enabled,
            issuer_url: self.issuer_url,
            client_id: self.client_id,
            client_secret: self.client_secret.unwrap_or_default(),
            redirect_uri: self.redirect_uri,
            scopes: self.scopes,
            username_claim: self.username_claim,
            groups_claim: self.groups_claim,
            auto_create_users: self.auto_create_users,
            default_policy: self.default_policy,
            group_policy_mappings: std::collections::HashMap::new(),
        }
    }
}

/// Path/body wrapper for create (id supplied in body).
#[derive(Debug, Deserialize)]
pub struct CreateOidcProviderRequest {
    pub id: String,
    #[serde(flatten)]
    pub config: OidcProviderRequest,
}

fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("Root privileges required")),
    )
        .into_response()
}

fn bad_request(msg: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(msg.into()))).into_response()
}

/// `GET /admin/oidc/providers` — list providers (root-only, secrets omitted).
pub async fn list_oidc_providers_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    match state.iam.list_oidc_providers().await {
        Ok(list) => {
            let providers: Vec<OidcProviderResponse> =
                list.into_iter().map(OidcProviderResponse::from).collect();
            Json(providers).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

/// `GET /admin/oidc/providers/{id}` — get one provider (root-only, no secret).
pub async fn get_oidc_provider_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    match state.iam.get_oidc_provider(&id).await {
        Ok(Some(p)) => Json(OidcProviderResponse::from(p)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Provider not found")),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

/// `POST /admin/oidc/providers` — create a provider (root-only).
pub async fn create_oidc_provider_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateOidcProviderRequest>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    if req.id.trim().is_empty() {
        return bad_request("Provider id is required");
    }
    if req.config.client_secret.as_deref().unwrap_or_default().is_empty() {
        return bad_request("Client secret is required when creating a provider");
    }
    let config = req.config.into_config(req.id);
    match state.iam.create_oidc_provider(&config).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(OidcProviderResponse::from(config)),
        )
            .into_response(),
        Err(e) => bad_request(e.to_string()),
    }
}

/// `PUT /admin/oidc/providers/{id}` — update a provider (root-only).
///
/// An empty `client_secret` preserves the existing stored secret.
pub async fn update_oidc_provider_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<OidcProviderRequest>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    let config = req.into_config(id);
    match state.iam.update_oidc_provider(&config).await {
        Ok(()) => Json(OidcProviderResponse::from(config)).into_response(),
        Err(e) => bad_request(e.to_string()),
    }
}

/// `DELETE /admin/oidc/providers/{id}` — delete a provider (root-only).
pub async fn delete_oidc_provider_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    match state.iam.delete_oidc_provider(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}
