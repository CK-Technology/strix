//! Integration tests for S3 event notification delivery.
//!
//! Proves that object events trigger matching webhook rules, that prefix/suffix
//! filters are honored, that non-2xx responses are retried and recorded, and
//! that unsupported destinations are persisted as `unsupported` rather than
//! silently dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use strix_core::{
    CreateBucketOpts, DeliveryQueryOpts, DeliveryStatus, NotificationConfiguration,
    NotificationDestination, NotificationFilter, NotificationRule, ObjectStore, S3EventType,
};
use strix_s3::{DispatcherConfig, EventSender, S3Event, start_dispatcher};
use strix_storage::LocalFsStore;

/// A minimal webhook receiver that counts requests and replies with a fixed
/// status code. Returns the listening URL and a shared hit counter.
async fn spawn_webhook(status_line: &'static str) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            hits_clone.fetch_add(1, Ordering::SeqCst);
            // Drain whatever is readily available, then respond.
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf).await;
            let body = "ok";
            let response = format!(
                "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    (format!("http://{addr}/hook"), hits)
}

async fn test_store() -> (TempDir, Arc<LocalFsStore>) {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let store = Arc::new(LocalFsStore::new(&data_dir).await.unwrap());
    (temp_dir, store)
}

fn fast_config() -> DispatcherConfig {
    DispatcherConfig {
        max_retries: 2,
        retry_base_delay: Duration::from_millis(10),
        delivery_timeout: Duration::from_secs(2),
        channel_capacity: 64,
    }
}

fn put_event(bucket: &str, key: &str) -> S3Event {
    S3Event {
        event_type: S3EventType::ObjectCreatedPut,
        bucket: bucket.to_string(),
        key: key.to_string(),
        size: Some(123),
        etag: Some("\"abc\"".to_string()),
        version_id: None,
        request_id: "test-req".to_string(),
        source_ip: None,
    }
}

/// Wait until at least `n` delivery records exist (or time out).
async fn wait_for_deliveries(
    store: &Arc<LocalFsStore>,
    n: usize,
) -> Vec<strix_core::NotificationDeliveryAttempt> {
    for _ in 0..100 {
        let records = store
            .query_notification_deliveries(DeliveryQueryOpts::default())
            .await
            .unwrap();
        if records.len() >= n {
            return records;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    store
        .query_notification_deliveries(DeliveryQueryOpts::default())
        .await
        .unwrap()
}

async fn setup_rule(
    store: &Arc<LocalFsStore>,
    bucket: &str,
    rule: NotificationRule,
) -> EventSender {
    store
        .create_bucket(
            bucket,
            CreateBucketOpts {
                region: None,
                tenant_slug: None,
            },
        )
        .await
        .unwrap();
    store
        .put_bucket_notification(bucket, NotificationConfiguration { rules: vec![rule] })
        .await
        .unwrap();
    start_dispatcher(store.clone(), fast_config(), None)
}

#[tokio::test]
async fn webhook_fires_on_object_created() {
    let (_tmp, store) = test_store().await;
    let (url, hits) = spawn_webhook("HTTP/1.1 200 OK").await;

    let sender = setup_rule(
        &store,
        "events-bucket",
        NotificationRule {
            id: "rule-1".to_string(),
            events: vec![S3EventType::ObjectCreatedAll],
            filter: NotificationFilter::default(),
            destination: NotificationDestination::Webhook { url },
        },
    )
    .await;

    sender.emit(put_event("events-bucket", "data/file.txt"));

    let records = wait_for_deliveries(&store, 1).await;
    assert_eq!(records.len(), 1, "expected one delivery record");
    let r = &records[0];
    assert_eq!(r.status, DeliveryStatus::Success);
    assert_eq!(r.rule_id, "rule-1");
    assert_eq!(r.attempts, 1);
    assert_eq!(r.response_code, Some(200));
    assert!(hits.load(Ordering::SeqCst) >= 1, "webhook was not called");
}

#[tokio::test]
async fn prefix_filter_suppresses_non_matching_keys() {
    let (_tmp, store) = test_store().await;
    let (url, hits) = spawn_webhook("HTTP/1.1 200 OK").await;

    let sender = setup_rule(
        &store,
        "filtered-bucket",
        NotificationRule {
            id: "prefix-rule".to_string(),
            events: vec![S3EventType::ObjectCreatedAll],
            filter: NotificationFilter {
                prefix: Some("logs/".to_string()),
                suffix: Some(".json".to_string()),
            },
            destination: NotificationDestination::Webhook { url },
        },
    )
    .await;

    // Does not match prefix -> no delivery.
    sender.emit(put_event("filtered-bucket", "images/cat.png"));
    // Matches prefix but not suffix -> no delivery.
    sender.emit(put_event("filtered-bucket", "logs/app.txt"));
    // Matches both -> delivered.
    sender.emit(put_event("filtered-bucket", "logs/app.json"));

    let records = wait_for_deliveries(&store, 1).await;
    assert_eq!(
        records.len(),
        1,
        "only the matching key should be delivered"
    );
    assert_eq!(records[0].object_key, "logs/app.json");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn non_2xx_response_is_retried_and_recorded_failed() {
    let (_tmp, store) = test_store().await;
    let (url, hits) = spawn_webhook("HTTP/1.1 500 Internal Server Error").await;

    let sender = setup_rule(
        &store,
        "failing-bucket",
        NotificationRule {
            id: "retry-rule".to_string(),
            events: vec![S3EventType::ObjectCreatedAll],
            filter: NotificationFilter::default(),
            destination: NotificationDestination::Webhook { url },
        },
    )
    .await;

    sender.emit(put_event("failing-bucket", "obj"));

    let records = wait_for_deliveries(&store, 1).await;
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r.status, DeliveryStatus::Failed);
    // max_retries = 2 -> 3 total attempts.
    assert_eq!(r.attempts, 3);
    assert_eq!(r.response_code, Some(500));
    assert!(r.last_error.is_some());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        3,
        "all attempts should hit the server"
    );
}

#[tokio::test]
async fn unsupported_destination_is_recorded_not_dropped() {
    let (_tmp, store) = test_store().await;

    let sender = setup_rule(
        &store,
        "kafka-bucket",
        NotificationRule {
            id: "kafka-rule".to_string(),
            events: vec![S3EventType::ObjectCreatedAll],
            filter: NotificationFilter::default(),
            destination: NotificationDestination::Kafka {
                brokers: vec!["broker:9092".to_string()],
                topic: "events".to_string(),
            },
        },
    )
    .await;

    sender.emit(put_event("kafka-bucket", "obj"));

    let records = wait_for_deliveries(&store, 1).await;
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r.status, DeliveryStatus::Unsupported);
    assert_eq!(r.destination_type, "kafka");
    assert_eq!(r.attempts, 0);
    assert!(r.last_error.is_some());
}
