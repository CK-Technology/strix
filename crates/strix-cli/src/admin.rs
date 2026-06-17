//! Admin API client with JWT authentication.

#![allow(dead_code)]

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::config::Alias;

/// Admin API client with automatic JWT authentication.
pub struct AdminClient {
    client: Client,
    base_url: String,
    token: Option<String>,
    access_key: String,
    secret_key: String,
}

impl AdminClient {
    /// Create a new admin client.
    pub fn new(alias: &Alias) -> Self {
        let base_url = alias.admin_url.clone().unwrap_or_else(|| {
            // Default: replace port with 9001
            alias.url.replace(":9000", ":9001").replace(":443", ":9001")
        });

        Self {
            client: Client::new(),
            base_url: format!("{}/api/v1", base_url.trim_end_matches('/')),
            token: None,
            access_key: alias.access_key.clone(),
            secret_key: alias.secret_key.clone(),
        }
    }

    /// Login and obtain a JWT token.
    pub async fn login(&mut self) -> Result<()> {
        let url = format!("{}/login", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&LoginRequest {
                access_key_id: self.access_key.clone(),
                secret_access_key: self.secret_key.clone(),
            })
            .send()
            .await
            .with_context(|| "Failed to connect to admin API for login")?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Authentication failed".to_string(),
            });
            anyhow::bail!("Login failed: {}", error.error);
        }

        let login_response: LoginResponse = response
            .json()
            .await
            .context("Failed to parse login response")?;

        self.token = Some(login_response.token);
        Ok(())
    }

    /// Ensure we have a valid token, logging in if needed.
    async fn ensure_authenticated(&mut self) -> Result<()> {
        if self.token.is_none() {
            self.login().await?;
        }
        Ok(())
    }

    /// Make a GET request with authentication.
    pub async fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        self.ensure_authenticated().await?;

        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.get(&url);

        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            anyhow::bail!("{}", error.error);
        }

        response.json().await.context("Failed to parse response")
    }

    /// Make a POST request with JSON body and authentication.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.ensure_authenticated().await?;

        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(body);

        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            anyhow::bail!("{}", error.error);
        }

        response.json().await.context("Failed to parse response")
    }

    /// Make a POST request without expecting response body, with authentication.
    pub async fn post_empty<B: Serialize>(&mut self, path: &str, body: &B) -> Result<()> {
        self.ensure_authenticated().await?;

        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(body);

        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            anyhow::bail!("{}", error.error);
        }

        Ok(())
    }

    /// Make a DELETE request with authentication.
    pub async fn delete(&mut self, path: &str) -> Result<()> {
        self.ensure_authenticated().await?;

        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.delete(&url);

        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
                error: "Unknown error".to_string(),
            });
            anyhow::bail!("{}", error.error);
        }

        Ok(())
    }

    // === API Methods ===

    pub async fn get_server_info(&mut self) -> Result<ServerInfo> {
        self.get("/info").await
    }

    pub async fn get_storage_usage(&mut self) -> Result<StorageUsage> {
        self.get("/usage").await
    }

    pub async fn list_users(&mut self) -> Result<ListUsersResponse> {
        self.get("/users").await
    }

    pub async fn create_user(&mut self, username: &str) -> Result<CreateUserResponse> {
        self.post(
            "/users",
            &CreateUserRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    pub async fn get_user(&mut self, username: &str) -> Result<UserInfo> {
        self.get(&format!("/users/{}", username)).await
    }

    pub async fn delete_user(&mut self, username: &str) -> Result<()> {
        self.delete(&format!("/users/{}", username)).await
    }

    pub async fn list_access_keys(&mut self, username: &str) -> Result<ListAccessKeysResponse> {
        self.get(&format!("/users/{}/access-keys", username)).await
    }

    pub async fn create_access_key(&mut self, username: &str) -> Result<AccessKeyResponse> {
        self.post(&format!("/users/{}/access-keys", username), &())
            .await
    }

    pub async fn delete_access_key(&mut self, access_key_id: &str) -> Result<()> {
        self.delete(&format!("/access-keys/{}", access_key_id))
            .await
    }

    // === Group Methods ===

    pub async fn list_groups(&mut self) -> Result<ListGroupsResponse> {
        self.get("/groups").await
    }

    pub async fn create_group(&mut self, name: &str) -> Result<()> {
        self.post_empty(
            "/groups",
            &CreateGroupRequest {
                name: name.to_string(),
            },
        )
        .await
    }

    pub async fn get_group(&mut self, name: &str) -> Result<GroupInfo> {
        self.get(&format!("/groups/{}", name)).await
    }

    pub async fn delete_group(&mut self, name: &str) -> Result<()> {
        self.delete(&format!("/groups/{}", name)).await
    }

    pub async fn add_user_to_group(&mut self, group: &str, username: &str) -> Result<()> {
        self.post_empty(
            &format!("/groups/{}/members", group),
            &AddMemberRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    pub async fn remove_user_from_group(&mut self, group: &str, username: &str) -> Result<()> {
        self.delete(&format!("/groups/{}/members/{}", group, username))
            .await
    }

    pub async fn attach_policy_to_group(&mut self, group: &str, policy: &str) -> Result<()> {
        self.post_empty(
            &format!("/groups/{}/policies", group),
            &AttachPolicyRequest {
                policy_name: policy.to_string(),
            },
        )
        .await
    }

    pub async fn detach_policy_from_group(&mut self, group: &str, policy: &str) -> Result<()> {
        self.delete(&format!("/groups/{}/policies/{}", group, policy))
            .await
    }

    // === Policy Methods ===

    pub async fn list_policies(&mut self) -> Result<ListPoliciesResponse> {
        self.get("/policies").await
    }

    pub async fn create_policy(
        &mut self,
        name: &str,
        document: &str,
        description: Option<&str>,
    ) -> Result<()> {
        self.post_empty(
            "/policies",
            &CreatePolicyRequest {
                name: name.to_string(),
                document: document.to_string(),
                description: description.map(|s| s.to_string()),
            },
        )
        .await
    }

    pub async fn get_policy(&mut self, name: &str) -> Result<PolicyInfo> {
        self.get(&format!("/policies/{}", name)).await
    }

    pub async fn delete_policy(&mut self, name: &str) -> Result<()> {
        self.delete(&format!("/policies/{}", name)).await
    }

    pub async fn attach_policy_to_user(&mut self, username: &str, policy: &str) -> Result<()> {
        self.post_empty(
            &format!("/users/{}/policies", username),
            &AttachPolicyRequest {
                policy_name: policy.to_string(),
            },
        )
        .await
    }

    pub async fn detach_policy_from_user(&mut self, username: &str, policy: &str) -> Result<()> {
        self.delete(&format!("/users/{}/policies/{}", username, policy))
            .await
    }

    // === Event/Notification Methods ===

    pub async fn get_bucket_notifications(&mut self, bucket: &str) -> Result<BucketNotifications> {
        self.get(&format!("/buckets/{}/notifications", bucket))
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_bucket_notification(
        &mut self,
        bucket: &str,
        destination_type: &str,
        destination_url: &str,
        events: &[String],
        id: Option<&str>,
        prefix: Option<&str>,
        suffix: Option<&str>,
    ) -> Result<()> {
        self.post_empty(
            &format!("/buckets/{}/notifications", bucket),
            &CreateNotificationRequest {
                id: id.map(|s| s.to_string()),
                events: events.to_vec(),
                prefix: prefix.map(|s| s.to_string()),
                suffix: suffix.map(|s| s.to_string()),
                destination_type: destination_type.to_string(),
                destination_url: destination_url.to_string(),
            },
        )
        .await
    }

    pub async fn delete_bucket_notification(&mut self, bucket: &str, id: &str) -> Result<()> {
        self.delete(&format!("/buckets/{}/notifications/{}", bucket, id))
            .await
    }

    pub async fn query_notification_deliveries(
        &mut self,
        bucket: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> Result<ListDeliveriesResponse> {
        let mut query = vec![format!("limit={limit}")];
        if let Some(b) = bucket {
            query.push(format!("bucket={b}"));
        }
        if let Some(s) = status {
            query.push(format!("status={s}"));
        }
        self.get(&format!("/notifications/deliveries?{}", query.join("&")))
            .await
    }

    // === Config Methods ===

    pub async fn get_config(&mut self) -> Result<serde_json::Value> {
        self.get("/config").await
    }

    pub async fn set_config(&mut self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.post_empty(&format!("/config/{}", key), value).await
    }
}

// API types

#[derive(Debug, Deserialize)]
pub struct ListDeliveriesResponse {
    pub entries: Vec<DeliveryAttempt>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeliveryAttempt {
    pub timestamp: String,
    pub bucket: String,
    pub rule_id: String,
    pub destination_type: String,
    pub target: String,
    pub event_type: String,
    pub object_key: String,
    pub attempts: u32,
    pub status: String,
    pub response_code: Option<u16>,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
struct LoginRequest {
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub commit: Option<String>,
    pub mode: String,
    pub uptime: u64,
    pub region: String,
}

#[derive(Debug, Deserialize)]
pub struct StorageUsage {
    pub buckets: Vec<BucketUsage>,
    pub total_buckets: u64,
    pub total_objects: u64,
    pub total_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct BucketUsage {
    pub name: String,
    pub created_at: String,
    pub object_count: u64,
    pub total_size: u64,
}

#[derive(Serialize)]
struct CreateUserRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserResponse {
    pub user: User,
    pub access_key: Option<AccessKey>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub username: String,
    pub arn: String,
    pub created_at: String,
    pub status: String,
    pub is_root: bool,
}

#[derive(Debug, Deserialize)]
pub struct AccessKey {
    pub access_key_id: String,
    pub secret_access_key: Option<String>,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub arn: String,
    pub created_at: String,
    pub status: String,
    pub policies: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListAccessKeysResponse {
    pub access_keys: Vec<AccessKeyInfo>,
}

#[derive(Debug, Deserialize)]
pub struct AccessKeyInfo {
    pub access_key_id: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct AccessKeyResponse {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub username: String,
    pub created_at: String,
    pub status: String,
}

// === Group Types ===

#[derive(Serialize)]
struct CreateGroupRequest {
    name: String,
}

#[derive(Serialize)]
struct AddMemberRequest {
    username: String,
}

#[derive(Serialize)]
struct AttachPolicyRequest {
    policy_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ListGroupsResponse {
    pub groups: Vec<GroupSummary>,
}

#[derive(Debug, Deserialize)]
pub struct GroupSummary {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub member_count: u32,
}

#[derive(Debug, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub members: Vec<String>,
    pub policies: Vec<String>,
}

// === Policy Types ===

#[derive(Serialize)]
struct CreatePolicyRequest {
    name: String,
    document: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListPoliciesResponse {
    pub policies: Vec<PolicySummary>,
}

#[derive(Debug, Deserialize)]
pub struct PolicySummary {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyInfo {
    pub name: String,
    pub arn: String,
    pub created_at: String,
    pub description: Option<String>,
    pub document: String,
}

// === Notification Types ===

#[derive(Serialize)]
struct CreateNotificationRequest {
    id: Option<String>,
    events: Vec<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    destination_type: String,
    destination_url: String,
}

#[derive(Debug, Deserialize)]
pub struct BucketNotifications {
    #[serde(default)]
    pub rules: Vec<NotificationRuleInfo>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationRuleInfo {
    pub id: String,
    pub events: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub destination_type: String,
    pub destination_url: String,
}
