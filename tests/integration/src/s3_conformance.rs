//! S3 API conformance tests.
//!
//! These tests verify basic S3 API compatibility by testing:
//! - Bucket operations (create, delete, list, head)
//! - Object operations (put, get, head, delete, copy)
//! - List operations (ListObjectsV2, common prefixes)
//!
//! Note: These tests require a running Strix server. They can be run with:
//! ```
//! STRIX_TEST_ENDPOINT=http://localhost:9000 \
//! STRIX_TEST_ACCESS_KEY=admin \
//! STRIX_TEST_SECRET_KEY=adminpass \
//! cargo test -p strix-integration-tests
//! ```

mod fixtures;

use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{ObjectLockLegalHoldStatus, ObjectLockRetentionMode};
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
        "test-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

/// Generate a unique object key.
fn unique_key() -> String {
    format!(
        "test-object-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

// ============================================================================
// Bucket Operations
// ============================================================================

#[tokio::test]
async fn test_create_bucket() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Create bucket
    let result = client.create_bucket().bucket(&bucket).send().await;
    assert!(
        result.is_ok(),
        "Failed to create bucket: {:?}",
        result.err()
    );

    // Verify bucket exists
    let head = client.head_bucket().bucket(&bucket).send().await;
    assert!(head.is_ok(), "Bucket should exist");

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_buckets() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Create a bucket
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // List buckets
    let result = client.list_buckets().send().await;
    assert!(result.is_ok(), "Failed to list buckets");

    let list_result = result.unwrap();
    let buckets = list_result.buckets();
    assert!(
        buckets.iter().any(|b| b.name() == Some(&bucket)),
        "Created bucket should be in list"
    );

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_delete_bucket() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Create and then delete bucket
    client.create_bucket().bucket(&bucket).send().await.unwrap();
    let result = client.delete_bucket().bucket(&bucket).send().await;
    assert!(result.is_ok(), "Failed to delete bucket");

    // Verify bucket no longer exists
    let head = client.head_bucket().bucket(&bucket).send().await;
    assert!(head.is_err(), "Bucket should not exist after deletion");
}

#[tokio::test]
async fn test_delete_non_empty_bucket_fails() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Create bucket with an object
    client.create_bucket().bucket(&bucket).send().await.unwrap();
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"test data"))
        .send()
        .await
        .unwrap();

    // Try to delete non-empty bucket
    let result = client.delete_bucket().bucket(&bucket).send().await;
    assert!(result.is_err(), "Deleting non-empty bucket should fail");

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_create_existing_bucket_returns_already_owned_by_you() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let duplicate = client.create_bucket().bucket(&bucket).send().await;
    assert!(
        duplicate.is_err(),
        "Creating existing bucket should fail with BucketAlreadyOwnedByYou"
    );

    let err = duplicate.err().unwrap();
    let code = err.into_service_error().code().map(ToString::to_string);
    assert_eq!(code.as_deref(), Some("BucketAlreadyOwnedByYou"));

    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Object Operations
// ============================================================================

#[tokio::test]
async fn test_put_and_get_object() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();
    let data = b"Hello, Strix!";

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Put object
    let put_result = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(data))
        .content_type("text/plain")
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "Failed to put object: {:?}",
        put_result.err()
    );

    let etag = put_result.unwrap().e_tag().unwrap().to_string();
    assert!(!etag.is_empty(), "ETag should not be empty");

    // Get object
    let get_result = client.get_object().bucket(&bucket).key(&key).send().await;
    assert!(get_result.is_ok(), "Failed to get object");

    let response = get_result.unwrap();
    assert_eq!(response.content_type(), Some("text/plain"));
    assert_eq!(response.content_length(), Some(data.len() as i64));

    let body = response.body.collect().await.unwrap().into_bytes();
    assert_eq!(&body[..], data);

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_object_lock_blocks_delete_when_retention_active() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(
            aws_sdk_s3::types::VersioningConfiguration::builder()
                .status(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
                .build(),
        )
        .send()
        .await
        .unwrap();

    let put = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"locked"))
        .send()
        .await
        .unwrap();
    let version_id = put.version_id().unwrap().to_string();

    let retain_until = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    client
        .put_object_retention()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version_id)
        .retention(
            aws_sdk_s3::types::ObjectLockRetention::builder()
                .mode(ObjectLockRetentionMode::Governance)
                .retain_until_date(aws_sdk_s3::primitives::DateTime::from(retain_until))
                .build(),
        )
        .send()
        .await
        .unwrap();

    let delete = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version_id)
        .send()
        .await;
    assert!(
        delete.is_err(),
        "delete should fail while retention is active"
    );

    let _ = client
        .put_object_legal_hold()
        .bucket(&bucket)
        .key(&key)
        .version_id(&version_id)
        .legal_hold(
            aws_sdk_s3::types::ObjectLockLegalHold::builder()
                .status(ObjectLockLegalHoldStatus::Off)
                .build(),
        )
        .send()
        .await;
}

#[tokio::test]
async fn test_head_object() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();
    let data = b"test data for head";

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(data))
        .content_type("application/octet-stream")
        .send()
        .await
        .unwrap();

    // Head object
    let result = client.head_object().bucket(&bucket).key(&key).send().await;
    assert!(result.is_ok(), "Failed to head object");

    let response = result.unwrap();
    assert_eq!(response.content_length(), Some(data.len() as i64));
    assert_eq!(response.content_type(), Some("application/octet-stream"));
    assert!(response.e_tag().is_some());
    assert!(response.last_modified().is_some());

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_delete_object() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"to be deleted"))
        .send()
        .await
        .unwrap();

    // Delete object
    let result = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    assert!(result.is_ok(), "Failed to delete object");

    // Verify object no longer exists
    let get = client.get_object().bucket(&bucket).key(&key).send().await;
    assert!(get.is_err(), "Object should not exist after deletion");

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_copy_object() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let src_key = unique_key();
    let dst_key = format!("{}-copy", src_key);
    let data = b"data to copy";

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();
    client
        .put_object()
        .bucket(&bucket)
        .key(&src_key)
        .body(ByteStream::from_static(data))
        .send()
        .await
        .unwrap();

    // Copy object
    let copy_source = format!("{}/{}", bucket, src_key);
    let result = client
        .copy_object()
        .bucket(&bucket)
        .key(&dst_key)
        .copy_source(&copy_source)
        .send()
        .await;
    assert!(result.is_ok(), "Failed to copy object: {:?}", result.err());

    // Verify copy exists
    let get = client
        .get_object()
        .bucket(&bucket)
        .key(&dst_key)
        .send()
        .await;
    assert!(get.is_ok(), "Copied object should exist");

    let body = get.unwrap().body.collect().await.unwrap().into_bytes();
    assert_eq!(&body[..], data);

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&src_key)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&dst_key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_get_nonexistent_object() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Try to get nonexistent object
    let result = client
        .get_object()
        .bucket(&bucket)
        .key("does-not-exist")
        .send()
        .await;
    assert!(result.is_err(), "Getting nonexistent object should fail");

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// List Operations
// ============================================================================

#[tokio::test]
async fn test_list_objects_v2() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup: Create bucket with multiple objects
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    for i in 0..5 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("object-{}", i))
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .unwrap();
    }

    // List objects
    let result = client.list_objects_v2().bucket(&bucket).send().await;
    assert!(result.is_ok(), "Failed to list objects");

    let response = result.unwrap();
    assert_eq!(response.key_count(), Some(5));
    assert!(!response.is_truncated().unwrap_or(false));

    let contents = response.contents();
    assert_eq!(contents.len(), 5);

    // Verify objects are sorted
    let keys: Vec<_> = contents.iter().filter_map(|o| o.key()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "Objects should be sorted by key");

    // Cleanup
    for i in 0..5 {
        let _ = client
            .delete_object()
            .bucket(&bucket)
            .key(format!("object-{}", i))
            .send()
            .await;
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_objects_with_prefix() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create objects with different prefixes
    for prefix in ["logs/", "data/", "config/"] {
        for i in 0..2 {
            client
                .put_object()
                .bucket(&bucket)
                .key(format!("{}{}", prefix, i))
                .body(ByteStream::from_static(b"data"))
                .send()
                .await
                .unwrap();
        }
    }

    // List with prefix
    let result = client
        .list_objects_v2()
        .bucket(&bucket)
        .prefix("logs/")
        .send()
        .await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert_eq!(response.key_count(), Some(2));

    let keys: Vec<_> = response.contents().iter().filter_map(|o| o.key()).collect();
    assert!(keys.iter().all(|k| k.starts_with("logs/")));

    // Cleanup
    for prefix in ["logs/", "data/", "config/"] {
        for i in 0..2 {
            let _ = client
                .delete_object()
                .bucket(&bucket)
                .key(format!("{}{}", prefix, i))
                .send()
                .await;
        }
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_objects_with_delimiter() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create hierarchical structure
    let keys = [
        "root.txt",
        "folder1/file1.txt",
        "folder1/file2.txt",
        "folder2/file1.txt",
        "folder2/subfolder/file.txt",
    ];

    for key in keys {
        client
            .put_object()
            .bucket(&bucket)
            .key(key)
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .unwrap();
    }

    // List with delimiter (root level)
    let result = client
        .list_objects_v2()
        .bucket(&bucket)
        .delimiter("/")
        .send()
        .await;
    assert!(result.is_ok());

    let response = result.unwrap();

    // Should have 1 object (root.txt) and 2 common prefixes (folder1/, folder2/)
    assert_eq!(response.contents().len(), 1);
    assert_eq!(response.common_prefixes().len(), 2);

    let prefixes: Vec<_> = response
        .common_prefixes()
        .iter()
        .filter_map(|p| p.prefix())
        .collect();
    assert!(prefixes.contains(&"folder1/"));
    assert!(prefixes.contains(&"folder2/"));

    // Cleanup
    for key in keys {
        let _ = client.delete_object().bucket(&bucket).key(key).send().await;
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_objects_pagination() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create 10 objects
    for i in 0..10 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("object-{:02}", i))
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .unwrap();
    }

    // List with max_keys=3 (paginated)
    let mut all_keys = Vec::new();
    let mut continuation_token = None;
    let mut page_count = 0;

    loop {
        let mut request = client.list_objects_v2().bucket(&bucket).max_keys(3);

        if let Some(token) = &continuation_token {
            request = request.continuation_token(token);
        }

        let response = request.send().await.unwrap();
        page_count += 1;

        for obj in response.contents() {
            if let Some(key) = obj.key() {
                all_keys.push(key.to_string());
            }
        }

        if response.is_truncated().unwrap_or(false) {
            continuation_token = response.next_continuation_token().map(|s| s.to_string());
        } else {
            break;
        }
    }

    assert_eq!(all_keys.len(), 10, "Should have all 10 objects");
    assert!(page_count >= 4, "Should have at least 4 pages");

    // Cleanup
    for i in 0..10 {
        let _ = client
            .delete_object()
            .bucket(&bucket)
            .key(format!("object-{:02}", i))
            .send()
            .await;
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Metadata Operations
// ============================================================================

#[tokio::test]
async fn test_object_with_metadata() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Put object with custom metadata
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"data with metadata"))
        .content_type("text/plain")
        .metadata("x-custom-header", "custom-value")
        .metadata("x-another-header", "another-value")
        .send()
        .await
        .unwrap();

    // Get object and verify metadata
    let response = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    assert_eq!(response.content_type(), Some("text/plain"));

    let metadata = response.metadata();
    assert!(metadata.is_some());
    let meta = metadata.unwrap();
    assert_eq!(
        meta.get("x-custom-header"),
        Some(&"custom-value".to_string())
    );
    assert_eq!(
        meta.get("x-another-header"),
        Some(&"another-value".to_string())
    );

    // Cleanup
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_object_tagging_roundtrip() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"tagged object"))
        .send()
        .await
        .unwrap();

    let tag_set = vec![
        aws_sdk_s3::types::Tag::builder()
            .key("project")
            .value("strix")
            .build()
            .unwrap(),
        aws_sdk_s3::types::Tag::builder()
            .key("env")
            .value("test")
            .build()
            .unwrap(),
    ];

    client
        .put_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .tagging(
            aws_sdk_s3::types::Tagging::builder()
                .set_tag_set(Some(tag_set))
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    let got = client
        .get_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let tags = got.tag_set();
    assert_eq!(tags.len(), 2);
    assert!(
        tags.iter()
            .any(|t| t.key() == "project" && t.value() == "strix")
    );
    assert!(tags.iter().any(|t| t.key() == "env" && t.value() == "test"));

    client
        .delete_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let got_after_delete = client
        .get_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    assert!(got_after_delete.tag_set().is_empty());

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_object_tagging_overwrite_and_empty_set() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"tag overwrite"))
        .send()
        .await
        .unwrap();

    let initial_tags = vec![
        aws_sdk_s3::types::Tag::builder()
            .key("project")
            .value("strix")
            .build()
            .unwrap(),
        aws_sdk_s3::types::Tag::builder()
            .key("env")
            .value("dev")
            .build()
            .unwrap(),
    ];

    client
        .put_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .tagging(
            aws_sdk_s3::types::Tagging::builder()
                .set_tag_set(Some(initial_tags))
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    let replacement_tags = vec![
        aws_sdk_s3::types::Tag::builder()
            .key("team")
            .value("core-platform")
            .build()
            .unwrap(),
    ];

    client
        .put_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .tagging(
            aws_sdk_s3::types::Tagging::builder()
                .set_tag_set(Some(replacement_tags))
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    let after_replace = client
        .get_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    assert_eq!(after_replace.tag_set().len(), 1);
    assert_eq!(after_replace.tag_set()[0].key(), "team");
    assert_eq!(after_replace.tag_set()[0].value(), "core-platform");

    client
        .put_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .tagging(
            aws_sdk_s3::types::Tagging::builder()
                .set_tag_set(Some(Vec::new()))
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    let after_empty = client
        .get_object_tagging()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    assert!(after_empty.tag_set().is_empty());

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_get_object_range_returns_exact_headers_and_body() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();
    let data = b"abcdefghijklmnopqrstuvwxyz0123456789";

    client.create_bucket().bucket(&bucket).send().await.unwrap();
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(data))
        .send()
        .await
        .unwrap();

    let response = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .range("bytes=0-10")
        .send()
        .await
        .unwrap();

    assert_eq!(response.content_length(), Some(11));
    assert_eq!(response.content_range(), Some("bytes 0-10/36"));

    let body = response.body.collect().await.unwrap().into_bytes();
    assert_eq!(body.len(), 11);
    assert_eq!(&body[..], &data[..11]);

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Delete Objects (Batch)
// ============================================================================

#[tokio::test]
async fn test_delete_objects() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multiple objects
    let keys: Vec<String> = (0..5).map(|i| format!("batch-delete-{}", i)).collect();
    for key in &keys {
        client
            .put_object()
            .bucket(&bucket)
            .key(key)
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .unwrap();
    }

    // Batch delete
    let delete = aws_sdk_s3::types::Delete::builder()
        .set_objects(Some(
            keys.iter()
                .map(|k| {
                    aws_sdk_s3::types::ObjectIdentifier::builder()
                        .key(k)
                        .build()
                        .unwrap()
                })
                .collect(),
        ))
        .build()
        .unwrap();

    let result = client
        .delete_objects()
        .bucket(&bucket)
        .delete(delete)
        .send()
        .await;
    assert!(result.is_ok(), "Failed to batch delete: {:?}", result.err());

    let response = result.unwrap();
    assert_eq!(response.deleted().len(), 5);

    // Verify objects are deleted
    let list = client
        .list_objects_v2()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();
    assert_eq!(list.key_count(), Some(0));

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Bucket Tagging Operations
// ============================================================================

#[tokio::test]
async fn test_bucket_tagging() {
    skip_if_not_configured!();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("Failed to create bucket");

    // Initially no tags - should return NoSuchTagSet error
    let result = client.get_bucket_tagging().bucket(&bucket).send().await;
    assert!(result.is_err(), "Expected NoSuchTagSet error");

    // Put tags
    let tagging = aws_sdk_s3::types::Tagging::builder()
        .tag_set(
            aws_sdk_s3::types::Tag::builder()
                .key("Environment")
                .value("Test")
                .build()
                .unwrap(),
        )
        .tag_set(
            aws_sdk_s3::types::Tag::builder()
                .key("Project")
                .value("Strix")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    client
        .put_bucket_tagging()
        .bucket(&bucket)
        .tagging(tagging)
        .send()
        .await
        .expect("Failed to put bucket tagging");

    // Get tags
    let tags = client
        .get_bucket_tagging()
        .bucket(&bucket)
        .send()
        .await
        .expect("Failed to get bucket tagging");

    assert_eq!(tags.tag_set().len(), 2);

    assert!(
        tags.tag_set()
            .iter()
            .any(|t| t.key() == "Environment" && t.value() == "Test"),
        "Environment=Test tag not found"
    );
    assert!(
        tags.tag_set()
            .iter()
            .any(|t| t.key() == "Project" && t.value() == "Strix"),
        "Project=Strix tag not found"
    );

    // Delete tags
    client
        .delete_bucket_tagging()
        .bucket(&bucket)
        .send()
        .await
        .expect("Failed to delete bucket tagging");

    // Verify deleted - should return NoSuchTagSet error again
    let result = client.get_bucket_tagging().bucket(&bucket).send().await;
    assert!(result.is_err(), "Expected NoSuchTagSet error after delete");

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}
