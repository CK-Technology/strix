#![allow(dead_code)]

//! IAM and Admin API integration tests.
//!
//! Tests for the admin API endpoints including user management,
//! access keys, and policies.

mod fixtures;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;

/// Get test configuration from environment variables.
fn get_admin_config() -> Option<(String, String, String)> {
    let endpoint = env::var("STRIX_TEST_ADMIN_ENDPOINT").ok()?;
    let access_key = env::var("STRIX_TEST_ACCESS_KEY").ok()?;
    let secret_key = env::var("STRIX_TEST_SECRET_KEY").ok()?;
    Some((endpoint, access_key, secret_key))
}

/// Skip test if not configured.
macro_rules! skip_if_not_configured {
    () => {
        if get_admin_config().is_none() {
            eprintln!("Skipping test: STRIX_TEST_ADMIN_ENDPOINT not set");
            return;
        }
    };
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Serialize)]
struct LoginRequest {
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
    expires_at: String,
    username: String,
}

#[derive(Debug, Serialize)]
struct CreateUserRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    username: String,
    arn: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct CreateUserResponse {
    user: UserInfo,
    access_key: Option<AccessKeyResponse>,
}

#[derive(Debug, Deserialize)]
struct AccessKeyResponse {
    access_key_id: String,
    secret_access_key: String,
    username: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ListUsersResponse {
    items: Vec<UserInfo>,
    has_more: bool,
}

#[derive(Debug, Serialize)]
struct CreateTenantRequest {
    name: String,
    slug: String,
    owner: String,
}

#[derive(Debug, Deserialize)]
struct TenantInfo {
    id: String,
    name: String,
    slug: String,
    owner: String,
}

#[derive(Debug, Deserialize)]
struct ListTenantsResponse {
    items: Vec<TenantInfo>,
}

#[derive(Debug, Serialize)]
struct CreateBucketRequest {
    name: String,
    tenant_slug: Option<String>,
    versioning: bool,
    object_locking: bool,
}

#[derive(Debug, Deserialize)]
struct BucketInfo {
    name: String,
    tenant_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListBucketsResponse {
    items: Vec<BucketInfo>,
}

#[derive(Debug, Deserialize)]
struct ServerInfo {
    version: String,
    mode: String,
    region: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    code: String,
    error: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn create_admin_client() -> (reqwest::Client, String) {
    let (endpoint, access_key, secret_key) = get_admin_config().expect("Admin config not set");

    let client = reqwest::Client::new();

    // Login to get token
    let login_req = LoginRequest {
        access_key_id: access_key,
        secret_access_key: secret_key,
    };

    let response = client
        .post(format!("{}/api/v1/login", endpoint))
        .json(&login_req)
        .send()
        .await
        .expect("Failed to login");

    if !response.status().is_success() {
        panic!("Login failed: {:?}", response.text().await);
    }

    let login_resp: LoginResponse = response
        .json()
        .await
        .expect("Failed to parse login response");

    (client, login_resp.token)
}

fn unique_username() -> String {
    format!(
        "testuser-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

// ============================================================================
// Server Info Tests
// ============================================================================

#[tokio::test]
async fn test_get_server_info() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();

    let response = client
        .get(format!("{}/api/v1/info", endpoint))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Should get server info");

    let info: ServerInfo = response.json().await.expect("Failed to parse");
    assert!(!info.version.is_empty());
    assert!(!info.region.is_empty());
}

#[tokio::test]
async fn test_admin_request_id_header_echo() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let request_id = format!("itest-{}", uuid::Uuid::new_v4());

    let response = client
        .get(format!("{}/api/v1/users", endpoint))
        .header("X-Request-Id", &request_id)
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success());
    let returned = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert_eq!(returned, request_id, "server should echo X-Request-Id");
}

#[tokio::test]
async fn test_health_endpoint() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (endpoint, _, _) = get_admin_config().unwrap();
    let client = reqwest::Client::new();

    // Health endpoint should work without auth
    let response = client
        .get(format!("{}/api/v1/health", endpoint))
        .send()
        .await
        .expect("Request failed");

    assert!(
        response.status().is_success(),
        "Health check should succeed"
    );
}

// ============================================================================
// User Management Tests
// ============================================================================

#[tokio::test]
async fn test_create_user() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    let response = client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Request failed");

    assert!(
        response.status().is_success(),
        "Should create user: {:?}",
        response.text().await
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_list_users() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user first
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create failed");

    // List users
    let response = client
        .get(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success());

    let users: ListUsersResponse = response.json().await.expect("Failed to parse");
    assert!(
        users.items.iter().any(|u| u.username == username),
        "Created user should be in list"
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_get_user() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create failed");

    // Get user
    let response = client
        .get(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success());

    let user: UserInfo = response.json().await.expect("Failed to parse");
    assert_eq!(user.username, username);

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_delete_user() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create failed");

    // Delete user
    let response = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Should delete user");

    // Verify user is gone
    let get_response = client
        .get(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_duplicate_user_fails() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create failed");

    // Try to create same user again
    let response = client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Request failed");

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "Should fail for duplicate"
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

// ============================================================================
// Access Key Tests
// ============================================================================

#[tokio::test]
async fn test_create_access_key() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    // Create access key
    let response = client
        .post(format!(
            "{}/api/v1/users/{}/access-keys",
            endpoint, username
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success(), "Should create access key");

    let key: AccessKeyResponse = response.json().await.expect("Failed to parse");
    assert!(!key.access_key_id.is_empty());
    assert!(!key.secret_access_key.is_empty());
    assert_eq!(key.username, username);
    assert_eq!(key.status, "Active");

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_list_access_keys() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user and access key
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    client
        .post(format!(
            "{}/api/v1/users/{}/access-keys",
            endpoint, username
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Create key failed");

    // List access keys
    let response = client
        .get(format!(
            "{}/api/v1/users/{}/access-keys",
            endpoint, username
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success());

    #[derive(Deserialize)]
    struct ListKeysResponse {
        access_keys: Vec<serde_json::Value>,
    }

    let keys: ListKeysResponse = response.json().await.expect("Failed to parse");
    assert_eq!(keys.access_keys.len(), 1, "Should have 1 access key");

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

// ============================================================================
// Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_unauthorized_without_token() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (endpoint, _, _) = get_admin_config().unwrap();
    let client = reqwest::Client::new();

    // Try to access protected endpoint without token
    let response = client
        .get(format!("{}/api/v1/users", endpoint))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_token() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (endpoint, _, _) = get_admin_config().unwrap();
    let client = reqwest::Client::new();

    // Try to access with invalid token
    let response = client
        .get(format!("{}/api/v1/users", endpoint))
        .bearer_auth("invalid-token")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_login() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (endpoint, _, _) = get_admin_config().unwrap();
    let client = reqwest::Client::new();

    let login_req = LoginRequest {
        access_key_id: "invalid".to_string(),
        secret_access_key: "credentials".to_string(),
    };

    let response = client
        .post(format!("{}/api/v1/login", endpoint))
        .json(&login_req)
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_non_root_admin_access_denied_without_policy() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, root_token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Root creates a non-admin user and captures initial access key.
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    let create_resp = client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&root_token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    assert!(create_resp.status().is_success());
    let created: CreateUserResponse = create_resp
        .json()
        .await
        .expect("Failed to parse create user response");

    let access_key = created
        .access_key
        .expect("Create user should return initial access key");

    // Login as non-root user.
    let login_req = LoginRequest {
        access_key_id: access_key.access_key_id,
        secret_access_key: access_key.secret_access_key,
    };

    let login_resp = client
        .post(format!("{}/api/v1/login", endpoint))
        .json(&login_req)
        .send()
        .await
        .expect("Non-root login request failed");

    assert!(login_resp.status().is_success());
    let login: LoginResponse = login_resp
        .json()
        .await
        .expect("Failed to parse non-root login response");

    // Non-root user should be forbidden from admin API by default (no attached admin policy).
    let users_resp = client
        .get(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&login.token)
        .send()
        .await
        .expect("Non-root list users request failed");

    assert_eq!(
        users_resp.status(),
        StatusCode::FORBIDDEN,
        "Non-root user without admin policy should be forbidden"
    );

    // Cleanup with root token.
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&root_token)
        .send()
        .await;
}

#[tokio::test]
async fn test_tenant_scope_filters_and_bucket_isolation() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();

    let tenant_slug = format!("tenant-{}", &uuid::Uuid::new_v4().to_string()[..6]);
    let bucket_name = format!("{}-bucket", tenant_slug);

    let tenant_req = CreateTenantRequest {
        name: "Tenant A".to_string(),
        slug: tenant_slug.clone(),
        owner: "ops@example.com".to_string(),
    };

    let create_tenant_resp = client
        .post(format!("{}/api/v1/tenants", endpoint))
        .bearer_auth(&token)
        .json(&tenant_req)
        .send()
        .await
        .expect("create tenant failed");
    assert!(create_tenant_resp.status().is_success());

    let _created_tenant: TenantInfo = create_tenant_resp
        .json()
        .await
        .expect("parse tenant response");

    let bucket_req = CreateBucketRequest {
        name: bucket_name.clone(),
        tenant_slug: Some(tenant_slug.clone()),
        versioning: false,
        object_locking: false,
    };

    let create_bucket_resp = client
        .post(format!("{}/api/v1/buckets", endpoint))
        .bearer_auth(&token)
        .json(&bucket_req)
        .send()
        .await
        .expect("create bucket failed");
    assert!(create_bucket_resp.status().is_success());

    // tenant filtered bucket listing should include bucket
    let list_resp = client
        .get(format!(
            "{}/api/v1/buckets?tenant_slug={}",
            endpoint, tenant_slug
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list buckets failed");
    assert!(list_resp.status().is_success());
    let list: ListBucketsResponse = list_resp.json().await.expect("parse list buckets");
    assert!(
        list.items.iter().any(|b| b.name == bucket_name),
        "bucket should be visible in tenant scope"
    );

    // using a mismatched tenant scope should deny bucket object listing
    let denied = client
        .get(format!(
            "{}/api/v1/buckets/{}/objects?tenant_slug=other-tenant",
            endpoint, bucket_name
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list objects request failed");
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    // cleanup
    let _ = client
        .delete(format!("{}/api/v1/buckets/{}", endpoint, bucket_name))
        .bearer_auth(&token)
        .query(&[("tenant_slug", tenant_slug.as_str())])
        .send()
        .await;

    let _ = client
        .delete(format!("{}/api/v1/tenants/{}", endpoint, tenant_slug))
        .bearer_auth(&token)
        .send()
        .await;
}

// ============================================================================
// Pagination Tests
// ============================================================================

// ============================================================================
// STS Tests
// ============================================================================

#[derive(Debug, Serialize)]
struct AssumeRoleRequest {
    username: String,
    session_name: Option<String>,
    duration_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AssumeRoleResponse {
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
    expiration: String,
}

/// Get S3 endpoint from environment variables.
fn get_s3_config() -> Option<String> {
    env::var("STRIX_TEST_S3_ENDPOINT").ok()
}

#[tokio::test]
async fn test_sts_assume_role() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user first
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    // Assume role
    let assume_req = AssumeRoleRequest {
        username: username.clone(),
        session_name: Some("test-session".to_string()),
        duration_seconds: Some(3600),
    };

    let response = client
        .post(format!("{}/api/v1/sts/assume-role", endpoint))
        .bearer_auth(&token)
        .json(&assume_req)
        .send()
        .await
        .expect("Request failed");

    assert!(
        response.status().is_success(),
        "Should assume role: {:?}",
        response.text().await
    );

    let creds: AssumeRoleResponse = client
        .post(format!("{}/api/v1/sts/assume-role", endpoint))
        .bearer_auth(&token)
        .json(&assume_req)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .expect("Failed to parse");

    // Verify credentials have correct format
    assert!(
        creds.access_key_id.starts_with("ASIA"),
        "Temp access key should have ASIA prefix"
    );
    assert!(!creds.secret_access_key.is_empty());
    assert!(!creds.session_token.is_empty());
    assert!(!creds.expiration.is_empty());

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_sts_temp_credentials_require_session_token() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let s3_endpoint = match get_s3_config() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: STRIX_TEST_S3_ENDPOINT not set");
            return;
        }
    };

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    // Get temporary credentials
    let assume_req = AssumeRoleRequest {
        username: username.clone(),
        session_name: Some("test-session".to_string()),
        duration_seconds: Some(3600),
    };

    let creds: AssumeRoleResponse = client
        .post(format!("{}/api/v1/sts/assume-role", endpoint))
        .bearer_auth(&token)
        .json(&assume_req)
        .send()
        .await
        .expect("Assume role failed")
        .json()
        .await
        .expect("Failed to parse credentials");

    // Try S3 operation WITHOUT session token - should fail
    let s3_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_credential_types::Credentials::new(
            &creds.access_key_id,
            &creds.secret_access_key,
            None, // No session token
            None,
            "test",
        ))
        .endpoint_url(&s3_endpoint)
        .load()
        .await;

    let s3_client = aws_sdk_s3::Client::new(&s3_config);

    // This should fail because session token is missing
    let result = s3_client.list_buckets().send().await;
    assert!(
        result.is_err(),
        "S3 request with temp creds but no session token should fail"
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_sts_temp_credentials_with_valid_session_token() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let s3_endpoint = match get_s3_config() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: STRIX_TEST_S3_ENDPOINT not set");
            return;
        }
    };

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    // Attach admin policy to allow S3 operations
    let policy_req = serde_json::json!({
        "policy": {
            "name": "AllowAll",
            "version": "2012-10-17",
            "statements": [{
                "effect": "Allow",
                "action": ["s3:*"],
                "resource": "*"
            }]
        }
    });

    client
        .post(format!("{}/api/v1/users/{}/policies", endpoint, username))
        .bearer_auth(&token)
        .json(&policy_req)
        .send()
        .await
        .expect("Attach policy failed");

    // Get temporary credentials
    let assume_req = AssumeRoleRequest {
        username: username.clone(),
        session_name: Some("test-session".to_string()),
        duration_seconds: Some(3600),
    };

    let creds: AssumeRoleResponse = client
        .post(format!("{}/api/v1/sts/assume-role", endpoint))
        .bearer_auth(&token)
        .json(&assume_req)
        .send()
        .await
        .expect("Assume role failed")
        .json()
        .await
        .expect("Failed to parse credentials");

    // Try S3 operation WITH session token - should succeed
    let s3_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_credential_types::Credentials::new(
            &creds.access_key_id,
            &creds.secret_access_key,
            Some(creds.session_token.clone()),
            None,
            "test",
        ))
        .endpoint_url(&s3_endpoint)
        .load()
        .await;

    let s3_client = aws_sdk_s3::Client::new(&s3_config);

    // This should succeed
    let result = s3_client.list_buckets().send().await;
    assert!(
        result.is_ok(),
        "S3 request with valid temp creds should succeed: {:?}",
        result.err()
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_sts_temp_credentials_wrong_session_token() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let s3_endpoint = match get_s3_config() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: STRIX_TEST_S3_ENDPOINT not set");
            return;
        }
    };

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();
    let username = unique_username();

    // Create a user
    let create_req = CreateUserRequest {
        username: username.clone(),
    };

    client
        .post(format!("{}/api/v1/users", endpoint))
        .bearer_auth(&token)
        .json(&create_req)
        .send()
        .await
        .expect("Create user failed");

    // Get temporary credentials
    let assume_req = AssumeRoleRequest {
        username: username.clone(),
        session_name: Some("test-session".to_string()),
        duration_seconds: Some(3600),
    };

    let creds: AssumeRoleResponse = client
        .post(format!("{}/api/v1/sts/assume-role", endpoint))
        .bearer_auth(&token)
        .json(&assume_req)
        .send()
        .await
        .expect("Assume role failed")
        .json()
        .await
        .expect("Failed to parse credentials");

    // Try S3 operation WITH WRONG session token - should fail
    let s3_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_credential_types::Credentials::new(
            &creds.access_key_id,
            &creds.secret_access_key,
            Some("wrong-session-token".to_string()),
            None,
            "test",
        ))
        .endpoint_url(&s3_endpoint)
        .load()
        .await;

    let s3_client = aws_sdk_s3::Client::new(&s3_config);

    // This should fail because session token is wrong
    let result = s3_client.list_buckets().send().await;
    assert!(
        result.is_err(),
        "S3 request with wrong session token should fail"
    );

    // Cleanup
    let _ = client
        .delete(format!("{}/api/v1/users/{}", endpoint, username))
        .bearer_auth(&token)
        .send()
        .await;
}

#[tokio::test]
async fn test_list_users_pagination() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let (client, token) = create_admin_client().await;
    let (endpoint, _, _) = get_admin_config().unwrap();

    // Create multiple users
    let mut usernames = Vec::new();
    for _ in 0..5 {
        let username = unique_username();
        let create_req = CreateUserRequest {
            username: username.clone(),
        };

        client
            .post(format!("{}/api/v1/users", endpoint))
            .bearer_auth(&token)
            .json(&create_req)
            .send()
            .await
            .expect("Create failed");

        usernames.push(username);
    }

    // List with limit
    let response = client
        .get(format!("{}/api/v1/users?limit=2", endpoint))
        .bearer_auth(&token)
        .send()
        .await
        .expect("Request failed");

    assert!(response.status().is_success());

    #[derive(Deserialize)]
    struct PaginatedUsers {
        items: Vec<UserInfo>,
        has_more: bool,
    }

    let page: PaginatedUsers = response.json().await.expect("Failed to parse");
    // Note: has_more might be true if there are more users than just our test users
    assert!(!page.items.is_empty());

    // Cleanup
    for username in usernames {
        let _ = client
            .delete(format!("{}/api/v1/users/{}", endpoint, username))
            .bearer_auth(&token)
            .send()
            .await;
    }
}
