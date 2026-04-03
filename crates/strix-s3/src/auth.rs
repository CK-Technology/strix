//! Authentication providers for S3 requests.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;

/// Credentials for an access key.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_key: String,
    pub secret_key: String,
    pub is_root: bool,
}

/// Trait for authentication providers.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Look up credentials by access key.
    async fn get_credentials(&self, access_key: &str) -> Option<Credentials>;
}

/// Simple in-memory authentication provider.
pub struct SimpleAuthProvider {
    credentials: RwLock<HashMap<String, Credentials>>,
}

impl SimpleAuthProvider {
    /// Create a new provider with root credentials.
    pub fn new(root_access_key: String, root_secret_key: String) -> Self {
        let mut credentials = HashMap::new();
        credentials.insert(
            root_access_key.clone(),
            Credentials {
                access_key: root_access_key,
                secret_key: root_secret_key,
                is_root: true,
            },
        );
        Self {
            credentials: RwLock::new(credentials),
        }
    }

    /// Add credentials.
    pub fn add_credentials(&self, access_key: String, secret_key: String, is_root: bool) {
        let mut creds = self.credentials.write();
        creds.insert(
            access_key.clone(),
            Credentials {
                access_key,
                secret_key,
                is_root,
            },
        );
    }
}

#[async_trait]
impl AuthProvider for SimpleAuthProvider {
    async fn get_credentials(&self, access_key: &str) -> Option<Credentials> {
        let creds = self.credentials.read();
        creds.get(access_key).cloned()
    }
}
