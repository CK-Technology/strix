//! Multipart upload integration tests.
//!
//! Tests for S3 multipart upload API compatibility.

mod fixtures;

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use base64::Engine;
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
        "test-mpu-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

/// Generate a unique object key.
fn unique_key() -> String {
    format!(
        "multipart-{}",
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

/// Generate test data of specified size.
fn generate_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

fn should_run_stress() -> bool {
    env::var("STRIX_STRESS_TESTS")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

// ============================================================================
// Basic Multipart Upload
// ============================================================================

#[tokio::test]
async fn test_create_multipart_upload() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .content_type("application/octet-stream")
        .send()
        .await;

    assert!(
        result.is_ok(),
        "Failed to create multipart upload: {:?}",
        result.err()
    );

    let upload = result.unwrap();
    assert!(upload.upload_id().is_some());
    assert_eq!(upload.bucket(), Some(bucket.as_str()));
    assert_eq!(upload.key(), Some(key.as_str()));

    // Abort the upload
    let upload_id = upload.upload_id().unwrap();
    let _ = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(upload_id)
        .send()
        .await;

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_complete_multipart_upload() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    // Upload parts (minimum 5MB each except last)
    let part1_data = generate_data(5 * 1024 * 1024); // 5MB
    let part2_data = generate_data(1024); // 1KB (last part can be smaller)

    let part1_result = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(part1_data.clone()))
        .send()
        .await
        .unwrap();

    let part2_result = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(2)
        .body(ByteStream::from(part2_data.clone()))
        .send()
        .await
        .unwrap();

    // Complete multipart upload
    let parts = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(part1_result.e_tag().unwrap())
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(part2_result.e_tag().unwrap())
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(parts)
        .send()
        .await;

    assert!(
        complete_result.is_ok(),
        "Failed to complete upload: {:?}",
        complete_result.err()
    );

    // Verify the object exists and has correct size
    let head = client
        .head_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    let expected_size = (part1_data.len() + part2_data.len()) as i64;
    assert_eq!(head.content_length(), Some(expected_size));

    // Verify ETag has multipart format (contains '-')
    let etag = head.e_tag().unwrap();
    assert!(
        etag.contains('-'),
        "Multipart ETag should contain '-': {}",
        etag
    );

    // Verify content
    let get = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    let body = get.body.collect().await.unwrap().into_bytes();

    let mut expected = part1_data;
    expected.extend(part2_data);
    assert_eq!(body.len(), expected.len());
    assert_eq!(&body[..], &expected[..]);

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
async fn test_upload_part_copy() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let source_key = unique_key();
    let dest_key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let source_data = generate_data(6 * 1024 * 1024);
    client
        .put_object()
        .bucket(&bucket)
        .key(&source_key)
        .body(ByteStream::from(source_data.clone()))
        .send()
        .await
        .unwrap();

    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&dest_key)
        .send()
        .await
        .unwrap();
    let upload_id = create_result.upload_id().unwrap().to_string();

    let copy_part = client
        .upload_part_copy()
        .bucket(&bucket)
        .key(&dest_key)
        .upload_id(&upload_id)
        .part_number(1)
        .copy_source(format!("{}/{}", bucket, source_key))
        .send()
        .await
        .unwrap();

    let copied_etag = copy_part
        .copy_part_result()
        .and_then(|r| r.e_tag())
        .expect("copy part should return ETag")
        .to_string();

    let parts = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(copied_etag)
                .build(),
        )
        .build();

    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&dest_key)
        .upload_id(&upload_id)
        .multipart_upload(parts)
        .send()
        .await
        .unwrap();

    let got = client
        .get_object()
        .bucket(&bucket)
        .key(&dest_key)
        .send()
        .await
        .unwrap();
    let got_body = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(&got_body[..], &source_data[..]);

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&dest_key)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&source_key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_abort_multipart_upload() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    // Upload a part
    client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from_static(b"test data"))
        .send()
        .await
        .unwrap();

    // Abort the upload
    let abort_result = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;

    assert!(
        abort_result.is_ok(),
        "Failed to abort upload: {:?}",
        abort_result.err()
    );

    // Verify upload is gone (listing should not include it)
    let list = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    let uploads = list.uploads();
    assert!(
        uploads.is_empty() || !uploads.iter().any(|u| u.upload_id() == Some(&upload_id)),
        "Aborted upload should not be in list"
    );

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_multipart_uploads() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multiple multipart uploads
    let mut upload_ids = Vec::new();
    for i in 0..3 {
        let key = format!("multipart-{}", i);
        let result = client
            .create_multipart_upload()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
            .unwrap();
        upload_ids.push((key, result.upload_id().unwrap().to_string()));
    }

    // List multipart uploads
    let list = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();

    let uploads = list.uploads();
    assert_eq!(uploads.len(), 3, "Should have 3 uploads");

    // Cleanup: abort all uploads
    for (key, upload_id) in upload_ids {
        let _ = client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(&key)
            .upload_id(&upload_id)
            .send()
            .await;
    }
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_large_multipart_workflow_stress() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    if !should_run_stress() {
        eprintln!("Skipping stress test: set STRIX_STRESS_TESTS=1 to enable");
        return;
    }

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let create = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    let upload_id = create.upload_id().unwrap().to_string();

    // ~128 MiB total payload (8 x 16 MiB parts)
    let part_size = 16 * 1024 * 1024;
    let part_count = 8;
    let mut parts = Vec::new();

    for i in 0..part_count {
        let data = vec![(i as u8) + 1; part_size];
        let out = client
            .upload_part()
            .bucket(&bucket)
            .key(&key)
            .upload_id(&upload_id)
            .part_number(i + 1)
            .body(ByteStream::from(data))
            .send()
            .await
            .unwrap();

        parts.push(
            CompletedPart::builder()
                .part_number(i + 1)
                .e_tag(out.e_tag().unwrap())
                .build(),
        );
    }

    let mut complete_builder = CompletedMultipartUpload::builder();
    for part in parts {
        complete_builder = complete_builder.parts(part);
    }

    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(complete_builder.build())
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    assert_eq!(
        head.content_length(),
        Some((part_size * part_count as usize) as i64)
    );

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_list_parts() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    // Upload multiple parts
    for i in 1..=3 {
        let data = generate_data(1024 * i); // Increasing sizes
        client
            .upload_part()
            .bucket(&bucket)
            .key(&key)
            .upload_id(&upload_id)
            .part_number(i as i32)
            .body(ByteStream::from(data))
            .send()
            .await
            .unwrap();
    }

    // List parts
    let list = client
        .list_parts()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await
        .unwrap();

    let parts = list.parts();
    assert_eq!(parts.len(), 3, "Should have 3 parts");

    // Verify parts are in order
    for (i, part) in parts.iter().enumerate() {
        assert_eq!(part.part_number(), Some((i + 1) as i32));
        assert!(part.e_tag().is_some());
        assert!(part.size().is_some());
    }

    // Cleanup
    let _ = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

// ============================================================================
// Error Cases
// ============================================================================

#[tokio::test]
async fn test_complete_with_invalid_etag() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    // Upload a part
    client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from_static(b"test data"))
        .send()
        .await
        .unwrap();

    // Try to complete with wrong ETag
    let parts = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag("\"invalid-etag\"")
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(parts)
        .send()
        .await;

    assert!(
        complete_result.is_err(),
        "Complete with invalid ETag should fail"
    );

    // Cleanup
    let _ = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_upload_part_to_nonexistent_upload() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Try to upload part to nonexistent upload
    let result = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id("nonexistent-upload-id")
        .part_number(1)
        .body(ByteStream::from_static(b"test data"))
        .send()
        .await;

    assert!(result.is_err(), "Upload to nonexistent upload should fail");

    // Cleanup
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_complete_with_parts_out_of_order() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    // Setup
    client.create_bucket().bucket(&bucket).send().await.unwrap();

    // Create multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    // Upload two parts
    let part1 = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from_static(b"part 1"))
        .send()
        .await
        .unwrap();

    let part2 = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(2)
        .body(ByteStream::from_static(b"part 2"))
        .send()
        .await
        .unwrap();

    // Try to complete with parts out of order (2, 1 instead of 1, 2)
    let parts = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(part2.e_tag().unwrap())
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(part1.e_tag().unwrap())
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(parts)
        .send()
        .await;

    assert!(
        complete_result.is_err(),
        "Complete with out-of-order parts should fail"
    );

    // Cleanup
    let _ = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}

#[tokio::test]
async fn test_complete_multipart_with_sse_c_fails_without_key_context() {
    skip_if_not_configured!();
    fixtures::init_tracing();

    let client = create_client().await;
    let bucket = unique_bucket();
    let key = unique_key();

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let create_result = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .sse_customer_algorithm("AES256")
        .sse_customer_key(base64::engine::general_purpose::STANDARD.encode([7u8; 32]))
        .sse_customer_key_md5(
            base64::engine::general_purpose::STANDARD.encode(md5::compute([7u8; 32]).0),
        )
        .send()
        .await
        .unwrap();

    let upload_id = create_result.upload_id().unwrap().to_string();

    let part = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from_static(b"test part"))
        .send()
        .await
        .unwrap();

    let parts = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(part.e_tag().unwrap())
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(parts)
        .send()
        .await;

    assert!(
        complete_result.is_err(),
        "SSE-C multipart completion without key material should fail"
    );

    let _ = client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
}
