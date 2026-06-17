//! OIDC client implementation.
//!
//! Implements the OAuth2 Authorization Code flow with OpenID Connect:
//! discovery, authorization URL construction, token exchange, and ID-token
//! verification via JWKS (RS256).

use crate::error::{IamError, Result};
use crate::idp::{OidcAuthResult, OidcClaims, OidcConfig, OidcTokenResponse};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

/// OIDC provider metadata from the discovery document.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    /// Issuer identifier.
    pub issuer: String,
    /// Authorization endpoint URL.
    pub authorization_endpoint: String,
    /// Token endpoint URL.
    pub token_endpoint: String,
    /// JWKS (JSON Web Key Set) endpoint URL.
    pub jwks_uri: String,
}

/// A JSON Web Key Set.
#[derive(Debug, Clone, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// A single JSON Web Key (RSA).
#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    kty: Option<String>,
    #[serde(default)]
    alg: Option<String>,
    /// RSA modulus (base64url).
    #[serde(default)]
    n: Option<String>,
    /// RSA exponent (base64url).
    #[serde(default)]
    e: Option<String>,
}

/// OIDC client backed by a shared HTTP client.
#[derive(Debug, Clone)]
pub struct OidcClient {
    http: reqwest::Client,
}

impl Default for OidcClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OidcClient {
    /// Create a new OIDC client.
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Fetch the provider's discovery document.
    ///
    /// Requests `{issuer}/.well-known/openid-configuration`. The issuer is used
    /// verbatim (trailing slash trimmed) as the well-known base.
    pub async fn discover(&self, issuer: &str) -> Result<OidcDiscovery> {
        let base = issuer.trim_end_matches('/');
        let url = format!("{base}/.well-known/openid-configuration");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| IamError::Oidc(format!("discovery request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(IamError::Oidc(format!(
                "discovery returned status {}",
                resp.status()
            )));
        }
        resp.json::<OidcDiscovery>()
            .await
            .map_err(|e| IamError::Oidc(format!("invalid discovery document: {e}")))
    }

    /// Build the authorization URL the browser should be redirected to.
    pub fn authorization_url(
        &self,
        discovery: &OidcDiscovery,
        config: &OidcConfig,
        state: &str,
        nonce: &str,
    ) -> String {
        let scopes = config.scopes.join(" ");
        format!(
            "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}&nonce={}",
            discovery.authorization_endpoint,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(&config.redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state),
            urlencoding::encode(nonce),
        )
    }

    /// Exchange an authorization code for tokens and verify the ID token.
    ///
    /// Performs the token request, verifies the returned ID token against the
    /// provider JWKS, checks the nonce, and resolves the username from claims.
    pub async fn exchange_and_verify(
        &self,
        config: &OidcConfig,
        discovery: &OidcDiscovery,
        code: &str,
        expected_nonce: &str,
    ) -> Result<OidcAuthResult> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ];
        let resp = self
            .http
            .post(&discovery.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| IamError::Oidc(format!("token request failed: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(IamError::Oidc(format!(
                "token endpoint returned {status}: {body}"
            )));
        }
        let token: OidcTokenResponse = resp
            .json()
            .await
            .map_err(|e| IamError::Oidc(format!("invalid token response: {e}")))?;

        let id_token = token
            .id_token
            .as_ref()
            .ok_or_else(|| IamError::Oidc("token response missing id_token".to_string()))?;

        let claims = self.verify_id_token(id_token, config, discovery).await?;

        // Replay protection: the nonce in the ID token must match the one we issued.
        match &claims.nonce {
            Some(n) if n == expected_nonce => {}
            Some(_) => return Err(IamError::Oidc("nonce mismatch".to_string())),
            None => return Err(IamError::Oidc("id_token missing nonce".to_string())),
        }

        let username = claims.get_username(&config.username_claim).ok_or_else(|| {
            IamError::Oidc(format!(
                "claim '{}' not present in id_token",
                config.username_claim
            ))
        })?;

        let expires_at = token
            .expires_in
            .map(|secs| chrono::Utc::now() + chrono::Duration::seconds(secs as i64));

        Ok(OidcAuthResult {
            username,
            claims,
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at,
        })
    }

    /// Verify an ID token's signature and standard claims against the provider JWKS.
    async fn verify_id_token(
        &self,
        id_token: &str,
        config: &OidcConfig,
        discovery: &OidcDiscovery,
    ) -> Result<OidcClaims> {
        let jwks = self.fetch_jwks(&discovery.jwks_uri).await?;
        verify_id_token_with_jwks(id_token, &config.client_id, &discovery.issuer, &jwks)
    }

    /// Fetch and parse the provider JWKS.
    async fn fetch_jwks(&self, jwks_uri: &str) -> Result<Jwks> {
        let resp = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .map_err(|e| IamError::Oidc(format!("JWKS request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(IamError::Oidc(format!(
                "JWKS returned status {}",
                resp.status()
            )));
        }
        resp.json::<Jwks>()
            .await
            .map_err(|e| IamError::Oidc(format!("invalid JWKS document: {e}")))
    }
}

/// Verify an ID token's RS256 signature and standard claims against a JWKS.
///
/// Pure (no I/O) so it can be unit-tested with a static key set. Selects the
/// signing key by `kid` (falling back to the sole RSA key when the token header
/// omits `kid`), then validates signature, `aud` (== `client_id`), `iss`
/// (== `issuer`), and `exp`.
fn verify_id_token_with_jwks(
    id_token: &str,
    client_id: &str,
    issuer: &str,
    jwks: &Jwks,
) -> Result<OidcClaims> {
    let header = decode_header(id_token)
        .map_err(|e| IamError::Oidc(format!("malformed id_token header: {e}")))?;

    // Select the JWK: by kid when present, otherwise the only RSA key.
    let jwk = match &header.kid {
        Some(kid) => jwks
            .keys
            .iter()
            .find(|k| k.kid.as_deref() == Some(kid.as_str()))
            .ok_or_else(|| IamError::Oidc(format!("no JWKS key matches kid '{kid}'")))?,
        None => {
            let mut rsa_keys = jwks.keys.iter().filter(|k| k.kty.as_deref() == Some("RSA"));
            let first = rsa_keys
                .next()
                .ok_or_else(|| IamError::Oidc("JWKS contains no RSA keys".to_string()))?;
            if rsa_keys.next().is_some() {
                return Err(IamError::Oidc(
                    "id_token header missing kid and JWKS has multiple keys".to_string(),
                ));
            }
            first
        }
    };

    if let Some(kty) = &jwk.kty
        && kty != "RSA"
    {
        return Err(IamError::Oidc(format!("unsupported JWK key type '{kty}'")));
    }

    let n = jwk
        .n
        .as_deref()
        .ok_or_else(|| IamError::Oidc("JWK missing modulus 'n'".to_string()))?;
    let e = jwk
        .e
        .as_deref()
        .ok_or_else(|| IamError::Oidc("JWK missing exponent 'e'".to_string()))?;

    let key = DecodingKey::from_rsa_components(n, e)
        .map_err(|err| IamError::Oidc(format!("invalid RSA JWK components: {err}")))?;

    // Honor the JWK's declared algorithm; default to RS256.
    let alg = match jwk.alg.as_deref() {
        Some("RS384") => Algorithm::RS384,
        Some("RS512") => Algorithm::RS512,
        _ => Algorithm::RS256,
    };

    let mut validation = Validation::new(alg);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[issuer]);

    decode::<OidcClaims>(id_token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|err| IamError::Oidc(format!("id_token verification failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;

    // RSA-2048 test key (throwaway, generated for this test only).
    const TEST_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDnP/1E0ETh2soh\n\
mW5W6WWK1gYzDYat68LCOMHsXGT84CMYJqz8SfSRsUT9x5BVgYrTvrYMQ4GRdQQI\n\
qXdxpfvFvQ9SvWhYkRNtmq0FX9AZ8bWQPDKlplFfHBSemJuukPnZke1LV4cxbjHk\n\
lHm6dQJMWfqf+31dlDw2fQgxtfrJF/OQcTIV314L47xNHTWia3tlCTMxonLGc7Ju\n\
lXMhHDdChKlIyJVl1M8RMWPRsqPYzGNoPbaKfdpUCEdXadw9IQvlfHUPPQeKAtZl\n\
3VNYtYZ7LUSv32HHuBO6g4xEVWD63b+1KaG3TntcueafGRPECjVlTID/f0p1M3hU\n\
OlfcJUCPAgMBAAECggEAPp76Y68OaCbKQ8z7rzdm0vDgHuUTynckd1nNUcc3Za8A\n\
ceLPR0Zznxxk9WAcOrtor6xeOfXx2UTZwcq6WKE9C7AFvT8jSZCHtU+EeQcYIF5u\n\
708N3AOs11eQUd47kksaDYvRuWxLZNxVUHPQfuh+pdRb5QTCTxv8Ljkvhd6kud1p\n\
v3X6Jkc0Kfpx/LiN03c8hlUdoVQanaDQfNreyGHCu/s2iJay69sKeE/1jUp4ISLR\n\
mfTIAVD5n6OARKvdLQirO23dbOl63lFtHeeEQ4gGMyMrdv9G+s5Jerg3iK4uaB7j\n\
R8nXqWr8DndcFTty/vmtrq4p3k2gwojWlXM0p8kBuQKBgQD4GW9mE1ZkbDPi9+Z8\n\
365Cty1auqjFjg2CAJSlmiLttkIJBCMuTuaKXfB3p+MyFRn0pBuQCnnzq5gec0vn\n\
iLMBEQAGAXlEQRXs1wz9m+RQYcJo43R3KxonkL3C1/enQ+SC5Eh8cwOufO23H59W\n\
e6oeQOELSPoZnocKPfBH6o6slQKBgQDunTGiwDQ1I3qmcG0GG2xIc6xyFZTrRBd0\n\
vJmCxf0YBClRNS3euOjIPIembey8Rm5ohfwvNhoaFqwGU4xLcZKE6+zdOHMWTbIj\n\
gzxgIyTShXg1GeIrBKGrPHx2BQPqEBnrNEBYCfZzXFnq2vraoaRL7rfCj/+ZpeiU\n\
mPuLrmbLkwKBgDX14kLDRfEFj6t324uhYtdj29t16as+IDX8RlhWU+57y5UGb1ht\n\
FLtXfyunOkT0TfblkpEbljanRaipzwKGutgqiGTGAUgVF92xUEQAmgHZoV0Ky5P3\n\
rfKZCozMSDL7E0JcwF9A7LYQueswV4mJ0BBQcCHyN2NHFXvmyNH7dBiZAoGBAIz/\n\
akMnnDICQwlyyZmgPr4ZTD8lrZfP5qRehb+WytWUL+4CpJZFYZhg3C9mKUufusIc\n\
2kXzjDz6RLCAUhiKhe/xkUevgaIeSzNc6yJL4ghcQgnuv4x38ihDV7BNimCXHxmz\n\
CIp9aJoGakOzHiRu+6y65O8dNAZQ2Txloc6KQcftAoGAZPV9ZlwlVp7dJ9CwMHfF\n\
64WyUS7UDS1lXADllGNtoGLq/phZPrhX5F80VfP2WcKfZFa8GrA1IeetCWpCp0v0\n\
45oOyZY9S1vpPKAlYwcGdLuwCafF86JXQagdSl3b1uCYliV2tAIRgDlQoF3Y10St\n\
a9mbRDnfRYHvs15YK9lB29w=\n\
-----END PRIVATE KEY-----\n";

    // Public-key components (base64url) corresponding to TEST_PEM.
    const TEST_N: &str = "5z_9RNBE4drKIZluVullitYGMw2GrevCwjjB7Fxk_OAjGCas_En0kbFE_ceQVYGK0762DEOBkXUECKl3caX7xb0PUr1oWJETbZqtBV_QGfG1kDwypaZRXxwUnpibrpD52ZHtS1eHMW4x5JR5unUCTFn6n_t9XZQ8Nn0IMbX6yRfzkHEyFd9eC-O8TR01omt7ZQkzMaJyxnOybpVzIRw3QoSpSMiVZdTPETFj0bKj2MxjaD22in3aVAhHV2ncPSEL5Xx1Dz0HigLWZd1TWLWGey1Er99hx7gTuoOMRFVg-t2_tSmht057XLnmnxkTxAo1ZUyA_39KdTN4VDpX3CVAjw";
    const TEST_E: &str = "AQAB";

    fn jwks_with_kid(kid: Option<&str>) -> Jwks {
        Jwks {
            keys: vec![Jwk {
                kid: kid.map(|s| s.to_string()),
                kty: Some("RSA".to_string()),
                alg: Some("RS256".to_string()),
                n: Some(TEST_N.to_string()),
                e: Some(TEST_E.to_string()),
            }],
        }
    }

    fn sign_token(kid: Option<&str>, claims: &serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = kid.map(|s| s.to_string());
        let key = EncodingKey::from_rsa_pem(TEST_PEM.as_bytes()).expect("valid test PEM");
        encode(&header, claims, &key).expect("token signing succeeds")
    }

    fn valid_claims() -> serde_json::Value {
        let now = chrono::Utc::now().timestamp() as u64;
        json!({
            "sub": "user-123",
            "iss": "https://idp.example.com",
            "aud": "strix-console",
            "exp": now + 3600,
            "iat": now,
            "email": "alice@example.com",
            "preferred_username": "alice",
            "nonce": "test-nonce",
        })
    }

    #[test]
    fn verifies_valid_token() {
        let token = sign_token(Some("key-1"), &valid_claims());
        let jwks = jwks_with_kid(Some("key-1"));
        let claims =
            verify_id_token_with_jwks(&token, "strix-console", "https://idp.example.com", &jwks)
                .expect("valid token verifies");
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.preferred_username.as_deref(), Some("alice"));
        assert_eq!(claims.nonce.as_deref(), Some("test-nonce"));
    }

    #[test]
    fn rejects_wrong_audience() {
        let token = sign_token(Some("key-1"), &valid_claims());
        let jwks = jwks_with_kid(Some("key-1"));
        let err =
            verify_id_token_with_jwks(&token, "other-client", "https://idp.example.com", &jwks)
                .unwrap_err();
        assert!(matches!(err, IamError::Oidc(_)));
    }

    #[test]
    fn rejects_wrong_issuer() {
        let token = sign_token(Some("key-1"), &valid_claims());
        let jwks = jwks_with_kid(Some("key-1"));
        let err =
            verify_id_token_with_jwks(&token, "strix-console", "https://evil.example.com", &jwks)
                .unwrap_err();
        assert!(matches!(err, IamError::Oidc(_)));
    }

    #[test]
    fn rejects_unknown_kid() {
        let token = sign_token(Some("unknown-kid"), &valid_claims());
        let jwks = jwks_with_kid(Some("key-1"));
        let err =
            verify_id_token_with_jwks(&token, "strix-console", "https://idp.example.com", &jwks)
                .unwrap_err();
        assert!(matches!(err, IamError::Oidc(_)));
    }

    #[test]
    fn rejects_expired_token() {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = json!({
            "sub": "user-123",
            "iss": "https://idp.example.com",
            "aud": "strix-console",
            "exp": now - 3600,
            "iat": now - 7200,
        });
        let token = sign_token(Some("key-1"), &claims);
        let jwks = jwks_with_kid(Some("key-1"));
        let err =
            verify_id_token_with_jwks(&token, "strix-console", "https://idp.example.com", &jwks)
                .unwrap_err();
        assert!(matches!(err, IamError::Oidc(_)));
    }

    #[test]
    fn selects_sole_key_without_kid() {
        let token = sign_token(None, &valid_claims());
        let jwks = jwks_with_kid(None);
        let claims =
            verify_id_token_with_jwks(&token, "strix-console", "https://idp.example.com", &jwks)
                .expect("sole RSA key is selected when kid absent");
        assert_eq!(claims.sub, "user-123");
    }
}
