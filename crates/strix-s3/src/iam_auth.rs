//! IAM-based authentication for S3 requests.

use std::sync::Arc;

use async_trait::async_trait;
use s3s::S3Result;
use s3s::auth::{S3Auth, SecretKey};
use s3s::s3_error;

use strix_iam::{AccessKeyStatus, IamProvider, UserStatus};

/// IAM-based S3 authentication provider.
///
/// This authenticator looks up credentials from the IAM store,
/// supporting both the root user and IAM users with access keys.
pub struct IamAuth {
    iam: Arc<dyn IamProvider>,
    root_access_key: String,
    root_secret_key: String,
}

impl IamAuth {
    /// Create a new IAM authenticator.
    pub fn new(
        iam: Arc<dyn IamProvider>,
        root_access_key: String,
        root_secret_key: String,
    ) -> Self {
        Self {
            iam,
            root_access_key,
            root_secret_key,
        }
    }
}

#[async_trait]
impl S3Auth for IamAuth {
    async fn get_secret_key(&self, access_key: &str) -> S3Result<SecretKey> {
        // Check if this is the root user
        if access_key == self.root_access_key {
            return Ok(SecretKey::from(self.root_secret_key.clone()));
        }

        // Check if this is a temporary credential (ASIA prefix)
        if access_key.starts_with("ASIA") {
            return self.get_temp_secret_key(access_key).await;
        }

        // Look up in IAM store (permanent credentials)
        match self.iam.get_credentials(access_key).await {
            Ok(Some((key, user))) => {
                // Check if the key is active
                if key.status != AccessKeyStatus::Active {
                    return Err(s3_error!(InvalidAccessKeyId, "Access key is not active"));
                }

                // Check if the user is active
                if user.status != UserStatus::Active {
                    return Err(s3_error!(InvalidAccessKeyId, "User account is not active"));
                }

                // Update last_used timestamp (fire and forget - don't block auth on this)
                let iam = self.iam.clone();
                let key_id = access_key.to_string();
                tokio::spawn(async move {
                    if let Err(e) = iam.update_access_key_last_used(&key_id).await {
                        tracing::warn!("Failed to update access key last_used: {}", e);
                    }
                });

                // Return the secret key (should always be present from get_credentials)
                match key.secret_access_key {
                    Some(secret) => Ok(SecretKey::from(secret)),
                    None => {
                        tracing::error!("Access key {} missing secret key", access_key);
                        Err(s3_error!(InternalError, "Authentication failed"))
                    }
                }
            }
            Ok(None) => Err(s3_error!(
                InvalidAccessKeyId,
                "The access key ID does not exist"
            )),
            Err(e) => {
                tracing::error!("IAM credential lookup failed: {}", e);
                Err(s3_error!(InternalError, "Authentication failed"))
            }
        }
    }
}

impl IamAuth {
    /// Get secret key for temporary (STS) credentials.
    async fn get_temp_secret_key(&self, access_key: &str) -> S3Result<SecretKey> {
        use chrono::Utc;

        match self.iam.get_temp_credentials(access_key).await {
            Ok(Some((cred, user))) => {
                // Check expiration
                if cred.expiration < Utc::now() {
                    return Err(s3_error!(ExpiredToken, "The provided token has expired"));
                }

                // Check user is active
                if user.status != UserStatus::Active {
                    return Err(s3_error!(InvalidAccessKeyId, "User account is not active"));
                }

                // Return the secret key
                match cred.secret_access_key {
                    Some(secret) => Ok(SecretKey::from(secret)),
                    None => {
                        tracing::error!("Temporary credential {} missing secret key", access_key);
                        Err(s3_error!(InternalError, "Authentication failed"))
                    }
                }
            }
            Ok(None) => Err(s3_error!(
                InvalidAccessKeyId,
                "The access key ID does not exist"
            )),
            Err(e) => {
                tracing::error!("Temporary credential lookup failed: {}", e);
                Err(s3_error!(InternalError, "Authentication failed"))
            }
        }
    }
}
