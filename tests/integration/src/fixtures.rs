#![allow(dead_code)]

//! Test fixtures for integration tests.
//!
//! Provides utilities for setting up test servers and S3 clients.

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Region;
use std::net::SocketAddr;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_rusqlite::Connection;

use strix_iam::IamStore;
use strix_storage::LocalFsStore;

/// Test context containing all resources needed for a test.
pub struct TestContext {
    /// Temporary directory for test data
    pub temp_dir: TempDir,
    /// S3 client configured to talk to the test server
    pub s3_client: Client,
    /// Address the server is listening on
    pub server_addr: SocketAddr,
    /// Storage backend
    pub storage: Arc<LocalFsStore>,
    /// IAM store
    pub iam: Arc<IamStore>,
    /// Root access key ID
    pub access_key_id: String,
    /// Root secret access key
    pub secret_access_key: String,
}

impl TestContext {
    /// Create a new test context with an S3 server running.
    pub async fn new() -> Self {
        // Create temp directory
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("Failed to create data dir");

        // Generate test credentials
        let access_key_id = format!(
            "AKIA{}",
            uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_uppercase()
        );
        let secret_access_key = uuid::Uuid::new_v4().to_string();

        // Create storage backend
        let storage = Arc::new(
            LocalFsStore::new(&data_dir)
                .await
                .expect("Failed to create storage"),
        );

        // Create IAM store with root credentials
        let iam_path = temp_dir.path().join("iam.db");
        let iam_db = Connection::open(&iam_path)
            .await
            .expect("Failed to open IAM database");
        let iam = Arc::new(
            IamStore::new(iam_db, access_key_id.clone(), secret_access_key.clone())
                .await
                .expect("Failed to create IAM store"),
        );

        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind");
        let server_addr = listener.local_addr().expect("Failed to get address");
        drop(listener);

        // Create S3 client
        let s3_client = create_s3_client(&server_addr, &access_key_id, &secret_access_key).await;

        Self {
            temp_dir,
            s3_client,
            server_addr,
            storage,
            iam,
            access_key_id,
            secret_access_key,
        }
    }

    /// Generate a unique bucket name for testing.
    pub fn unique_bucket_name(&self) -> String {
        format!(
            "test-bucket-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
        )
    }

    /// Generate a unique object key for testing.
    pub fn unique_key(&self) -> String {
        format!(
            "test-object-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
        )
    }
}

/// Create an S3 client configured to talk to a local server.
pub async fn create_s3_client(
    addr: &SocketAddr,
    access_key_id: &str,
    secret_access_key: &str,
) -> Client {
    let creds = Credentials::new(access_key_id, secret_access_key, None, None, "test");

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(format!("http://{}", addr))
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}

/// Initialize tracing for tests.
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("strix=debug,integration=debug")
        .with_test_writer()
        .try_init();
}
