//! Versioning integration tests.
//!
//! Tests for S3 bucket versioning API compatibility.

mod fixtures;

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{BucketVersioningStatus, VersioningConfiguration};
use std::env;

/// Get test configuration from environment variables.
fn get_test_config() -> Option<(String, String, String)> {
    let endpoint = env::var("STRIX_TEST_ENDPOINT").ok()?;
    let access_key = env::var("STRIX_TEST_ACCESS_KEY").ok()?;
    let secret_key = env::var("STRIX_TEST_SECRET_KEY").ok()?;
    Some((endpoint, access_key, secret_key))
}

/// Skip test if not configured.
macro_rules! skip_if_not_configured {
    () => {
        if get_test_config().is_none() {
            eprintln!("Skipping test: STRIX_TEST_ENDPOINT not set");
            return;
        }
    };
}

/// Create an S3 client from environment configuration.
async fn create_client() -> aws_sdk_s3::Client {
    let (endpoint, access_key, secret_key) = get_test_config().expect("Test not configured");

    let creds =
        aws_credential_types::Credentials::new(&access_key, &secret_key, None, None, "test");

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url(&endpoint)
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    aws_sdk_s3::Client::from_conf(config)
}

/// Generate a unique bucket name.
fn unique_bucket() -> String {
    format!(
        "test-ver-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

/// Generate a unique object key.
fn unique_key() -> String {
    format!(
        "versioned-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

// ============================================================================
// Versioning Configuration
// ============================================================================

#[tokio::test]
async fn test_enable_versioning() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Check initial versioning status (should be unversioned)
    let get_result = client
        .get_bucket_versioning()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    // Initially should be empty/unversioned
    assert!(
        get_result.status().is_none()
            || get_result.status() == Some(&BucketVersioningStatus::Suspended),
        "Initial versioning should be unset"
    );

    // Enable versioning
    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();

    let put_result = client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await;

    assert!(
        put_result.is_ok(),
        "Failed to enable versioning: {:?}",
        put_result.err()
    );

    // Verify versioning is enabled
    let get_result = client
        .get_bucket_versioning()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    assert_eq!(get_result.status(), Some(&BucketVersioningStatus::Enabled));

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_suspend_versioning() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Suspend versioning
    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Suspended)
        .build();

    let result = client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await;

    assert!(result.is_ok(), "Failed to suspend versioning");

    // Verify versioning is suspended
    let get_result = client
        .get_bucket_versioning()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    assert_eq!(
        get_result.status(),
        Some(&BucketVersioningStatus::Suspended)
    );

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Object Versioning
// ============================================================================

#[tokio::test]
async fn test_put_object_returns_version_id() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Put object
    let put_result = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"version 1"))
        .send()
        .await
        .unwrap();

    // Should have a version ID
    assert!(
        put_result.version_id().is_some(),
        "PUT should return version ID"
    );
    let version1 = put_result.version_id().unwrap().to_string();

    // Put another version
    let put_result2 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"version 2"))
        .send()
        .await
        .unwrap();

    let version2 = put_result2.version_id().unwrap().to_string();
    assert_ne!(
        version1, version2,
        "Different versions should have different IDs"
    );

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version1)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version2)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_get_specific_version() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Put two versions
    let v1_data = b"version 1 data";
    let v2_data = b"version 2 data - different";

    let put1 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(v1_data))
        .send()
        .await
        .unwrap();

    let version1 = put1.version_id().unwrap().to_string();

    let put2 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(v2_data))
        .send()
        .await
        .unwrap();

    let version2 = put2.version_id().unwrap().to_string();

    // Get latest (should be version 2)
    let get_latest = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let latest_body = get_latest.body.collect().await.unwrap().into_bytes();
    assert_eq!(&latest_body[..], v2_data);

    // Get specific version (version 1)
    let get_v1 = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version1)
        .send()
        .await
        .unwrap();

    let v1_body = get_v1.body.collect().await.unwrap().into_bytes();
    assert_eq!(&v1_body[..], v1_data);

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version1)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version2)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_object_versions() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Create multiple versions
    let mut version_ids = Vec::new();
    for i in 0..3 {
        let put = client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(ByteStream::from(format!("version {}", i).into_bytes()))
            .send()
            .await
            .unwrap();
        version_ids.push(put.version_id().unwrap().to_string());
    }

    // List versions
    let list = client
        .list_object_versions()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    let versions = list.versions();
    assert_eq!(versions.len(), 3, "Should have 3 versions");

    // Latest version should be marked as is_latest
    let latest = versions.iter().find(|v| v.is_latest() == Some(true));
    assert!(latest.is_some(), "One version should be marked as latest");
    assert_eq!(
        latest.unwrap().version_id(),
        Some(version_ids.last().unwrap().as_str()),
        "Latest should be the most recent version"
    );

    // Cleanup
    for vid in version_ids {
        let _ = client
            .delete_object()
            .bucket(&bucket)
            .key(&key)
            .version_id(&vid)
            .send()
            .await;
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_delete_creates_delete_marker() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Put an object
    let put = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"data"))
        .send()
        .await
        .unwrap();

    let version_id = put.version_id().unwrap().to_string();

    // Delete without version ID (creates delete marker)
    let delete = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    // Should return delete marker info
    assert!(
        delete.delete_marker().unwrap_or(false),
        "Should create delete marker"
    );
    let delete_marker_version = delete.version_id().unwrap().to_string();

    // GET should now fail (object appears deleted)
    let get = client.get_object().bucket(&bucket).key(&key).send().await;
    assert!(get.is_err(), "GET should fail after delete marker");

    // But we can still get the old version
    let get_version = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version_id)
        .send()
        .await;
    assert!(get_version.is_ok(), "Should be able to get old version");

    // List versions should show delete marker
    let list = client
        .list_object_versions()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    let delete_markers = list.delete_markers();
    assert_eq!(delete_markers.len(), 1, "Should have 1 delete marker");
    assert!(delete_markers[0].is_latest() == Some(true));

    // Cleanup: delete the actual versions
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version_id)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&delete_marker_version)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_delete_specific_version() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Put two versions
    let put1 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"version 1"))
        .send()
        .await
        .unwrap();

    let version1 = put1.version_id().unwrap().to_string();

    let put2 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"version 2"))
        .send()
        .await
        .unwrap();

    let version2 = put2.version_id().unwrap().to_string();

    // Delete version 1 specifically (hard delete, not delete marker)
    let delete = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version1)
        .send()
        .await
        .unwrap();

    // Should NOT create delete marker when deleting specific version
    assert!(!delete.delete_marker().unwrap_or(false));

    // Version 2 should still be accessible
    let get = client.get_object().bucket(&bucket).key(&key).send().await;
    assert!(get.is_ok(), "Current version should still be accessible");

    let body = get.unwrap().body.collect().await.unwrap().into_bytes();
    assert_eq!(&body[..], b"version 2");

    // Version 1 should be gone
    let get_v1 = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version1)
        .send()
        .await;
    assert!(get_v1.is_err(), "Deleted version should not be accessible");

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version2)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_copy_preserves_version() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let src_key = unique_key();
    let dst_key = format!("{}-copy", src_key);

    // Setup: create bucket and enable versioning
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let config = VersioningConfiguration::builder()
        .status(BucketVersioningStatus::Enabled)
        .build();
    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(config)
        .send()
        .await
        .unwrap();

    // Put source object
    client
        .put_object()
        .bucket(&bucket)
        .key(&src_key)
        .body(ByteStream::from_static(b"source data"))
        .send()
        .await
        .unwrap();

    // Copy object
    let copy_source = format!("{}/{}", bucket, src_key);
    let copy = client
        .copy_object()
        .bucket(&bucket)
        .key(&dst_key)
        .copy_source(&copy_source)
        .send()
        .await
        .unwrap();

    // Copy should have its own version ID
    assert!(copy.version_id().is_some(), "Copy should have version ID");

    // Cleanup
    let list = client
        .list_object_versions()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();
    for version in list.versions() {
        if let (Some(key), Some(vid)) = (version.key(), version.version_id()) {
            let _ = client
                .delete_object()
                .bucket(&bucket)
                .key(key)
                .version_id(vid)
                .send()
                .await;
        }
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}
