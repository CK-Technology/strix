//! Admin API client.

#![allow(dead_code)]

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::config::Alias;

/// Admin API client.
pub struct AdminClient {
    client: Client,
    base_url: String,
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
        }
    }

    /// Make a GET request.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
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

    /// Make a POST request with JSON body.
    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
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

    /// Make a POST request without expecting response body.
    pub async fn post_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
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

    /// Make a DELETE request.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .delete(&url)
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

    pub async fn get_server_info(&self) -> Result<ServerInfo> {
        self.get("/info").await
    }

    pub async fn get_storage_usage(&self) -> Result<StorageUsage> {
        self.get("/usage").await
    }

    pub async fn list_users(&self) -> Result<ListUsersResponse> {
        self.get("/users").await
    }

    pub async fn create_user(&self, username: &str) -> Result<CreateUserResponse> {
        self.post(
            "/users",
            &CreateUserRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    pub async fn get_user(&self, username: &str) -> Result<UserInfo> {
        self.get(&format!("/users/{}", username)).await
    }

    pub async fn delete_user(&self, username: &str) -> Result<()> {
        self.delete(&format!("/users/{}", username)).await
    }

    pub async fn list_access_keys(&self, username: &str) -> Result<ListAccessKeysResponse> {
        self.get(&format!("/users/{}/access-keys", username)).await
    }

    pub async fn create_access_key(&self, username: &str) -> Result<AccessKeyResponse> {
        self.post(&format!("/users/{}/access-keys", username), &())
            .await
    }

    pub async fn delete_access_key(&self, access_key_id: &str) -> Result<()> {
        self.delete(&format!("/access-keys/{}", access_key_id))
            .await
    }

    // === Group Methods ===

    pub async fn list_groups(&self) -> Result<ListGroupsResponse> {
        self.get("/groups").await
    }

    pub async fn create_group(&self, name: &str) -> Result<()> {
        self.post_empty(
            "/groups",
            &CreateGroupRequest {
                name: name.to_string(),
            },
        )
        .await
    }

    pub async fn get_group(&self, name: &str) -> Result<GroupInfo> {
        self.get(&format!("/groups/{}", name)).await
    }

    pub async fn delete_group(&self, name: &str) -> Result<()> {
        self.delete(&format!("/groups/{}", name)).await
    }

    pub async fn add_user_to_group(&self, group: &str, username: &str) -> Result<()> {
        self.post_empty(
            &format!("/groups/{}/members", group),
            &AddMemberRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    pub async fn remove_user_from_group(&self, group: &str, username: &str) -> Result<()> {
        self.delete(&format!("/groups/{}/members/{}", group, username))
            .await
    }

    pub async fn attach_policy_to_group(&self, group: &str, policy: &str) -> Result<()> {
        self.post_empty(
            &format!("/groups/{}/policies", group),
            &AttachPolicyRequest {
                policy_name: policy.to_string(),
            },
        )
        .await
    }

    pub async fn detach_policy_from_group(&self, group: &str, policy: &str) -> Result<()> {
        self.delete(&format!("/groups/{}/policies/{}", group, policy))
            .await
    }

    // === Policy Methods ===

    pub async fn list_policies(&self) -> Result<ListPoliciesResponse> {
        self.get("/policies").await
    }

    pub async fn create_policy(
        &self,
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

    pub async fn get_policy(&self, name: &str) -> Result<PolicyInfo> {
        self.get(&format!("/policies/{}", name)).await
    }

    pub async fn delete_policy(&self, name: &str) -> Result<()> {
        self.delete(&format!("/policies/{}", name)).await
    }

    pub async fn attach_policy_to_user(&self, username: &str, policy: &str) -> Result<()> {
        self.post_empty(
            &format!("/users/{}/policies", username),
            &AttachPolicyRequest {
                policy_name: policy.to_string(),
            },
        )
        .await
    }

    pub async fn detach_policy_from_user(&self, username: &str, policy: &str) -> Result<()> {
        self.delete(&format!("/users/{}/policies/{}", username, policy))
            .await
    }

    // === Event/Notification Methods ===

    pub async fn get_bucket_notifications(&self, bucket: &str) -> Result<BucketNotifications> {
        self.get(&format!("/buckets/{}/notifications", bucket))
            .await
    }

    pub async fn create_bucket_notification(
        &self,
        bucket: &str,
        arn: &str,
        events: &[String],
        id: Option<&str>,
        prefix: Option<&str>,
        suffix: Option<&str>,
    ) -> Result<()> {
        self.post_empty(
            &format!("/buckets/{}/notifications", bucket),
            &CreateNotificationRequest {
                arn: arn.to_string(),
                events: events.to_vec(),
                id: id.map(|s| s.to_string()),
                prefix: prefix.map(|s| s.to_string()),
                suffix: suffix.map(|s| s.to_string()),
            },
        )
        .await
    }

    pub async fn delete_bucket_notification(&self, bucket: &str, id: &str) -> Result<()> {
        self.delete(&format!("/buckets/{}/notifications/{}", bucket, id))
            .await
    }

    // === Config Methods ===

    pub async fn get_config(&self) -> Result<serde_json::Value> {
        self.get("/config").await
    }

    pub async fn set_config(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.post_empty(&format!("/config/{}", key), value).await
    }
}

// API types

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
    arn: String,
    events: Vec<String>,
    id: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BucketNotifications {
    #[serde(default)]
    pub queue_configurations: Vec<QueueConfiguration>,
    #[serde(default)]
    pub topic_configurations: Vec<TopicConfiguration>,
    #[serde(default)]
    pub lambda_configurations: Vec<LambdaConfiguration>,
}

#[derive(Debug, Deserialize)]
pub struct QueueConfiguration {
    pub id: Option<String>,
    pub queue_arn: String,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TopicConfiguration {
    pub id: Option<String>,
    pub topic_arn: String,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LambdaConfiguration {
    pub id: Option<String>,
    pub lambda_arn: String,
    pub events: Vec<String>,
}
