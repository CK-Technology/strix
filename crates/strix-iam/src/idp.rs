//! Identity Provider (IdP) support for external authentication.
//!
//! Supports OIDC providers like Azure AD/Entra ID and Google.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of identity provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IdentityProviderType {
    /// Local username/password authentication.
    #[default]
    Local,
    /// OpenID Connect (OIDC) provider.
    Oidc,
}

/// OIDC provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Unique identifier for this provider.
    pub id: String,
    /// Display name for the provider (e.g., "Azure AD", "Google").
    pub name: String,
    /// Whether this provider is enabled.
    pub enabled: bool,
    /// OIDC issuer URL (e.g., "https://login.microsoftonline.com/{tenant}/v2.0").
    pub issuer_url: String,
    /// Client ID from the OIDC provider.
    pub client_id: String,
    /// Client secret from the OIDC provider.
    pub client_secret: String,
    /// Redirect URI for OAuth callback.
    pub redirect_uri: String,
    /// Scopes to request (default: openid, email, profile).
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// Claim to use for username (default: "preferred_username" or "email").
    #[serde(default = "default_username_claim")]
    pub username_claim: String,
    /// Claim to use for groups (optional, for role mapping).
    #[serde(default)]
    pub groups_claim: Option<String>,
    /// Whether to create users automatically on first login.
    #[serde(default = "default_auto_create")]
    pub auto_create_users: bool,
    /// Default policy to assign to auto-created users.
    #[serde(default)]
    pub default_policy: Option<String>,
    /// Group to policy mappings.
    #[serde(default)]
    pub group_policy_mappings: HashMap<String, String>,
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

fn default_auto_create() -> bool {
    true
}

impl OidcConfig {
    /// Create a new Azure AD/Entra ID configuration.
    pub fn azure_ad(
        id: impl Into<String>,
        tenant_id: &str,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: "Azure AD".to_string(),
            enabled: true,
            issuer_url: format!("https://login.microsoftonline.com/{}/v2.0", tenant_id),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: redirect_uri.into(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            username_claim: "preferred_username".to_string(),
            groups_claim: Some("groups".to_string()),
            auto_create_users: true,
            default_policy: None,
            group_policy_mappings: HashMap::new(),
        }
    }

    /// Create a new Google configuration.
    pub fn google(
        id: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: "Google".to_string(),
            enabled: true,
            issuer_url: "https://accounts.google.com".to_string(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: redirect_uri.into(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            username_claim: "email".to_string(),
            groups_claim: None,
            auto_create_users: true,
            default_policy: None,
            group_policy_mappings: HashMap::new(),
        }
    }

    /// Get the authorization endpoint URL.
    pub fn authorization_url(&self, state: &str, nonce: &str) -> String {
        let scopes = self.scopes.join(" ");
        format!(
            "{}/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}&nonce={}",
            self.issuer_url.trim_end_matches("/v2.0"),
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state),
            urlencoding::encode(nonce),
        )
    }

    /// Get the token endpoint URL.
    pub fn token_url(&self) -> String {
        format!("{}/token", self.issuer_url.trim_end_matches("/v2.0"))
    }
}

/// OIDC token response.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub scope: Option<String>,
}

/// Claims from the OIDC ID token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcClaims {
    /// Subject (unique user identifier from the IdP).
    pub sub: String,
    /// Issuer.
    pub iss: String,
    /// Audience.
    pub aud: String,
    /// Expiration time.
    pub exp: u64,
    /// Issued at time.
    pub iat: u64,
    /// Email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Whether email is verified.
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// Full name.
    #[serde(default)]
    pub name: Option<String>,
    /// Given name.
    #[serde(default)]
    pub given_name: Option<String>,
    /// Family name.
    #[serde(default)]
    pub family_name: Option<String>,
    /// Preferred username.
    #[serde(default)]
    pub preferred_username: Option<String>,
    /// Picture URL.
    #[serde(default)]
    pub picture: Option<String>,
    /// Groups (if groups claim is configured).
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

impl OidcClaims {
    /// Get the username based on the configured claim.
    pub fn get_username(&self, claim: &str) -> Option<String> {
        match claim {
            "email" => self.email.clone(),
            "preferred_username" => self
                .preferred_username
                .clone()
                .or_else(|| self.email.clone()),
            "sub" => Some(self.sub.clone()),
            "name" => self.name.clone(),
            _ => None,
        }
    }
}

/// Result of an OIDC authentication flow.
#[derive(Debug, Clone)]
pub struct OidcAuthResult {
    /// Username derived from claims.
    pub username: String,
    /// Full claims from the ID token.
    pub claims: OidcClaims,
    /// Access token for API calls.
    pub access_token: String,
    /// Refresh token if available.
    pub refresh_token: Option<String>,
    /// When the tokens expire.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Identity provider manager configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityProviderConfig {
    /// Whether local authentication is enabled.
    #[serde(default = "default_true")]
    pub local_enabled: bool,
    /// List of OIDC providers.
    #[serde(default)]
    pub oidc_providers: Vec<OidcConfig>,
}

fn default_true() -> bool {
    true
}

impl IdentityProviderConfig {
    /// Create a new configuration with only local auth enabled.
    pub fn local_only() -> Self {
        Self {
            local_enabled: true,
            oidc_providers: Vec::new(),
        }
    }

    /// Add an OIDC provider.
    pub fn add_oidc_provider(&mut self, config: OidcConfig) {
        self.oidc_providers.push(config);
    }

    /// Get an OIDC provider by ID.
    pub fn get_provider(&self, id: &str) -> Option<&OidcConfig> {
        self.oidc_providers.iter().find(|p| p.id == id && p.enabled)
    }

    /// List all enabled OIDC providers.
    pub fn enabled_providers(&self) -> Vec<&OidcConfig> {
        self.oidc_providers.iter().filter(|p| p.enabled).collect()
    }
}

/// Login method used for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginMethod {
    /// Local username/password.
    Local { username: String },
    /// OIDC provider.
    Oidc {
        provider_id: String,
        subject: String,
    },
    /// Access key authentication (S3 API).
    AccessKey { access_key_id: String },
}

/// Session information for an authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: String,
    /// Username.
    pub username: String,
    /// How the user authenticated.
    pub login_method: LoginMethod,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the session expires.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Additional claims from OIDC (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_claims: Option<OidcClaims>,
}

impl Session {
    /// Create a new session for a local user.
    pub fn new_local(username: impl Into<String>, duration_hours: u64) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: ulid::Ulid::new().to_string(),
            username: username.into(),
            login_method: LoginMethod::Local {
                username: String::new(), // Will be filled
            },
            created_at: now,
            expires_at: now + chrono::Duration::hours(duration_hours as i64),
            oidc_claims: None,
        }
    }

    /// Create a new session from OIDC authentication.
    pub fn new_oidc(
        provider_id: impl Into<String>,
        auth_result: &OidcAuthResult,
        duration_hours: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: ulid::Ulid::new().to_string(),
            username: auth_result.username.clone(),
            login_method: LoginMethod::Oidc {
                provider_id: provider_id.into(),
                subject: auth_result.claims.sub.clone(),
            },
            created_at: now,
            expires_at: auth_result
                .expires_at
                .unwrap_or(now + chrono::Duration::hours(duration_hours as i64)),
            oidc_claims: Some(auth_result.claims.clone()),
        }
    }

    /// Check if the session is expired.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() >= self.expires_at
    }
}
