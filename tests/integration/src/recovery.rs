//! Restart and durability integration tests.
//!
//! These tests start/stop a real Strix process and verify state recovery.

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{BucketVersioningStatus, VersioningConfiguration};
use std::env;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tempfile::TempDir;

fn get_bin_path() -> String {
    env::var("STRIX_TEST_BIN").unwrap_or_else(|_| "./target/debug/strix".to_string())
}

fn recovery_tests_enabled() -> bool {
    let bin = get_bin_path();
    Path::new(&bin).exists()
}

fn unique_name(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        uuid::Uuid::new_v4().to_string()[..8].to_lowercase()
    )
}

fn start_strix(data_dir: &Path, root_user: &str, root_password: &str) -> Child {
    let bin = get_bin_path();
    let mut cmd = Command::new(bin);
    cmd.env("STRIX_ROOT_USER", root_user)
        .env("STRIX_ROOT_PASSWORD", root_password)
        .env("STRIX_DATA_DIR", data_dir)
        .env("STRIX_LOG_LEVEL", "info")
        .env("STRIX_ADDRESS", "127.0.0.1:9000")
        .env("STRIX_CONSOLE_ADDRESS", "127.0.0.1:9001")
        .env("STRIX_METRICS_ADDRESS", "127.0.0.1:9090")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().expect("failed to spawn strix process")
}

async fn create_client() -> aws_sdk_s3::Client {
    let creds = aws_credential_types::Credentials::new("admin", "testpass123", None, None, "test");

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url("http://127.0.0.1:9000")
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    aws_sdk_s3::Client::from_conf(config)
}

async fn wait_until_up(client: &aws_sdk_s3::Client) {
    for _ in 0..40 {
        let ping = client.list_buckets().send().await;
        if ping.is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("Strix did not become ready in time");
}

fn stop_strix(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn test_recovery_persists_completed_multipart_object() {
    if !recovery_tests_enabled() {
        eprintln!("Skipping recovery test: set STRIX_TEST_BIN to a built strix binary");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let mut proc = start_strix(temp.path(), "admin", "testpass123");
    let client = create_client().await;
    wait_until_up(&client).await;

    let bucket = unique_name("recover-mpu");
    let key = unique_name("obj");

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    let create = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    let upload_id = create.upload_id().unwrap().to_string();

    let p1 = vec![b'A'; 5 * 1024 * 1024];
    let p2 = vec![b'B'; 1024 * 1024];

    let part1 = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(p1.clone()))
        .send()
        .await
        .unwrap();

    let part2 = client
        .upload_part()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .part_number(2)
        .body(ByteStream::from(p2.clone()))
        .send()
        .await
        .unwrap();

    let complete = aws_sdk_s3::types::CompletedMultipartUpload::builder()
        .parts(
            aws_sdk_s3::types::CompletedPart::builder()
                .part_number(1)
                .e_tag(part1.e_tag().unwrap())
                .build(),
        )
        .parts(
            aws_sdk_s3::types::CompletedPart::builder()
                .part_number(2)
                .e_tag(part2.e_tag().unwrap())
                .build(),
        )
        .build();

    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&key)
        .upload_id(&upload_id)
        .multipart_upload(complete)
        .send()
        .await
        .unwrap();

    stop_strix(&mut proc);

    let mut proc = start_strix(temp.path(), "admin", "testpass123");
    let client = create_client().await;
    wait_until_up(&client).await;

    let got = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .unwrap();
    let body = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(body.len(), p1.len() + p2.len());
    assert_eq!(&body[..p1.len()], &p1[..]);

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
    stop_strix(&mut proc);
}

#[tokio::test]
async fn test_recovery_persists_versioning_state_and_versions() {
    if !recovery_tests_enabled() {
        eprintln!("Skipping recovery test: set STRIX_TEST_BIN to a built strix binary");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let mut proc = start_strix(temp.path(), "admin", "testpass123");
    let client = create_client().await;
    wait_until_up(&client).await;

    let bucket = unique_name("recover-ver");
    let key = unique_name("obj");

    client.create_bucket().bucket(&bucket).send().await.unwrap();

    client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(
            VersioningConfiguration::builder()
                .status(BucketVersioningStatus::Enabled)
                .build(),
        )
        .send()
        .await
        .unwrap();

    let v1 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"v1"))
        .send()
        .await
        .unwrap()
        .version_id()
        .unwrap()
        .to_string();

    let v2 = client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from_static(b"v2"))
        .send()
        .await
        .unwrap()
        .version_id()
        .unwrap()
        .to_string();

    stop_strix(&mut proc);

    let mut proc = start_strix(temp.path(), "admin", "testpass123");
    let client = create_client().await;
    wait_until_up(&client).await;

    let status = client
        .get_bucket_versioning()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), Some(&BucketVersioningStatus::Enabled));

    let list = client
        .list_object_versions()
        .bucket(&bucket)
        .send()
        .await
        .unwrap();
    let version_ids: Vec<String> = list
        .versions()
        .iter()
        .filter_map(|v| v.version_id().map(ToString::to_string))
        .collect();
    assert!(version_ids.contains(&v1));
    assert!(version_ids.contains(&v2));

    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&v1)
        .send()
        .await;
    let _ = client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .version_id(&v2)
        .send()
        .await;
    let _ = client.delete_bucket().bucket(&bucket).send().await;
    stop_strix(&mut proc);
}
