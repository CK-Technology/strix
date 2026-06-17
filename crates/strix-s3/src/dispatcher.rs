//! Notification dispatcher for S3 event delivery.
//!
//! Receives events from S3 operations via an mpsc channel, matches them
//! against stored notification rules, and delivers webhooks with retry.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use metrics::counter;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use strix_core::{
    DeliveryFailureAlerter, DeliveryStatus, NotificationDeliveryAttempt, NotificationDestination,
    NotificationRule, ObjectStore, S3EventRecord, S3EventType,
};

/// Outcome of attempting to deliver a single notification.
struct DeliveryOutcome {
    attempts: u32,
    status: DeliveryStatus,
    response_code: Option<u16>,
    last_error: Option<String>,
}

/// An event emitted by an S3 operation.
#[derive(Debug, Clone)]
pub struct S3Event {
    pub event_type: S3EventType,
    pub bucket: String,
    pub key: String,
    pub size: Option<u64>,
    pub etag: Option<String>,
    pub version_id: Option<String>,
    pub request_id: String,
    pub source_ip: Option<String>,
}

/// Handle for sending events from S3 operations. Cloneable and cheap.
#[derive(Clone)]
pub struct EventSender {
    tx: mpsc::Sender<S3Event>,
}

impl EventSender {
    /// Fire-and-forget event emission. Never blocks S3 operations.
    pub fn emit(&self, event: S3Event) {
        if let Err(e) = self.tx.try_send(event) {
            warn!("Notification channel full or closed: {}", e);
        }
    }
}

/// Dispatcher configuration.
pub struct DispatcherConfig {
    pub max_retries: u32,
    pub retry_base_delay: Duration,
    pub delivery_timeout: Duration,
    pub channel_capacity: usize,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_base_delay: Duration::from_secs(1),
            delivery_timeout: Duration::from_secs(10),
            channel_capacity: 1024,
        }
    }
}

/// Start the notification dispatcher as a background task.
///
/// Returns an `EventSender` that S3 operations use to emit events. When an
/// `alerter` is provided, failed or unsupported deliveries trigger a
/// best-effort out-of-band alert after the attempt has been persisted.
pub fn start_dispatcher(
    store: Arc<dyn ObjectStore>,
    config: DispatcherConfig,
    alerter: Option<Arc<dyn DeliveryFailureAlerter>>,
) -> EventSender {
    let (tx, mut rx) = mpsc::channel::<S3Event>(config.channel_capacity);

    let client = reqwest::Client::builder()
        .timeout(config.delivery_timeout)
        .user_agent("Strix-Webhook/1.0")
        .build()
        .expect("failed to build HTTP client");

    tokio::spawn(async move {
        info!("Notification dispatcher started");
        while let Some(event) = rx.recv().await {
            counter!("strix_notification_events_total").increment(1);

            let rules = match store.get_bucket_notification(&event.bucket).await {
                Ok(Some(config)) => config.rules,
                Ok(None) => continue,
                Err(e) => {
                    warn!(
                        bucket = %event.bucket,
                        "Failed to load notification config: {}", e
                    );
                    continue;
                }
            };

            for rule in &rules {
                if !matches_rule(rule, &event) {
                    continue;
                }

                let record = build_event_record(&event);
                let (destination_type, target, outcome) = match &rule.destination {
                    NotificationDestination::Webhook { url } => {
                        let outcome = deliver_webhook(
                            &client,
                            url,
                            &record,
                            config.max_retries,
                            config.retry_base_delay,
                        )
                        .await;
                        ("webhook", url.clone(), outcome)
                    }
                    other => {
                        let (dest_type, target) = describe_destination(other);
                        warn!(
                            bucket = %event.bucket,
                            rule = %rule.id,
                            destination = dest_type,
                            "Notification destination type is not supported for delivery; \
                             recording as unsupported"
                        );
                        counter!(
                            "strix_webhook_deliveries_total",
                            "status" => "unsupported"
                        )
                        .increment(1);
                        (
                            dest_type,
                            target,
                            DeliveryOutcome {
                                attempts: 0,
                                status: DeliveryStatus::Unsupported,
                                response_code: None,
                                last_error: Some(format!(
                                    "destination type '{dest_type}' is not yet supported for delivery"
                                )),
                            },
                        )
                    }
                };

                let attempt = NotificationDeliveryAttempt {
                    id: Uuid::new_v4().to_string(),
                    timestamp: Utc::now(),
                    bucket: event.bucket.clone(),
                    rule_id: rule.id.clone(),
                    destination_type: destination_type.to_string(),
                    target,
                    event_type: event.event_type.to_string(),
                    object_key: event.key.clone(),
                    attempts: outcome.attempts,
                    status: outcome.status,
                    response_code: outcome.response_code,
                    last_error: outcome.last_error,
                };
                let needs_alert = attempt.status != DeliveryStatus::Success;
                let alert_copy = if needs_alert {
                    Some(attempt.clone())
                } else {
                    None
                };

                if let Err(e) = store.log_notification_delivery(attempt).await {
                    warn!(
                        bucket = %event.bucket,
                        rule = %rule.id,
                        "Failed to persist notification delivery record: {}", e
                    );
                }

                if let (Some(alerter), Some(failed)) = (&alerter, &alert_copy) {
                    alerter.alert_delivery_failure(failed).await;
                }
            }
        }
        info!("Notification dispatcher shutting down");
    });

    EventSender { tx }
}

fn matches_rule(rule: &NotificationRule, event: &S3Event) -> bool {
    let event_matches = rule.events.iter().any(|re| {
        *re == event.event_type
            || matches!(
                (re, &event.event_type),
                (S3EventType::ObjectCreatedAll, S3EventType::ObjectCreatedPut)
                    | (
                        S3EventType::ObjectCreatedAll,
                        S3EventType::ObjectCreatedPost
                    )
                    | (
                        S3EventType::ObjectCreatedAll,
                        S3EventType::ObjectCreatedCopy
                    )
                    | (
                        S3EventType::ObjectCreatedAll,
                        S3EventType::ObjectCreatedCompleteMultipartUpload
                    )
                    | (
                        S3EventType::ObjectRemovedAll,
                        S3EventType::ObjectRemovedDelete
                    )
                    | (
                        S3EventType::ObjectRemovedAll,
                        S3EventType::ObjectRemovedDeleteMarkerCreated
                    ),
            )
    });
    if !event_matches {
        return false;
    }

    if let Some(prefix) = &rule.filter.prefix
        && !event.key.starts_with(prefix)
    {
        return false;
    }
    if let Some(suffix) = &rule.filter.suffix
        && !event.key.ends_with(suffix)
    {
        return false;
    }

    true
}

fn build_event_record(event: &S3Event) -> S3EventRecord {
    S3EventRecord {
        event_version: "2.1".to_string(),
        event_source: "strix:s3".to_string(),
        event_time: Utc::now(),
        event_name: event.event_type.to_string(),
        bucket_name: event.bucket.clone(),
        object_key: event.key.clone(),
        object_size: event.size,
        object_etag: event.etag.clone(),
        object_version_id: event.version_id.clone(),
        request_id: event.request_id.clone(),
        source_ip: event.source_ip.clone(),
    }
}

/// Describe a non-webhook destination as (type, target) for diagnostics.
fn describe_destination(dest: &NotificationDestination) -> (&'static str, String) {
    match dest {
        NotificationDestination::Webhook { url } => ("webhook", url.clone()),
        NotificationDestination::Amqp { url, .. } => ("amqp", url.clone()),
        NotificationDestination::Kafka { brokers, topic } => {
            ("kafka", format!("{}/{}", brokers.join(","), topic))
        }
        NotificationDestination::Redis { url, channel } => ("redis", format!("{url}/{channel}")),
    }
}

async fn deliver_webhook(
    client: &reqwest::Client,
    url: &str,
    record: &S3EventRecord,
    max_retries: u32,
    retry_base_delay: Duration,
) -> DeliveryOutcome {
    let payload = serde_json::json!({ "Records": [record] });
    let mut last_response_code: Option<u16> = None;
    let mut last_error: Option<String> = None;

    for attempt in 0..=max_retries {
        match client.post(url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                debug!(url, attempt = attempt + 1, "Webhook delivered");
                counter!("strix_webhook_deliveries_total", "status" => "success").increment(1);
                return DeliveryOutcome {
                    attempts: attempt + 1,
                    status: DeliveryStatus::Success,
                    response_code: Some(resp.status().as_u16()),
                    last_error: None,
                };
            }
            Ok(resp) => {
                let status = resp.status();
                last_response_code = Some(status.as_u16());
                last_error = Some(format!("non-success status {status}"));
                warn!(
                    url,
                    status = %status,
                    attempt = attempt + 1,
                    max = max_retries + 1,
                    "Webhook returned non-success status"
                );
            }
            Err(e) => {
                last_error = Some(e.to_string());
                warn!(
                    url,
                    error = %e,
                    attempt = attempt + 1,
                    max = max_retries + 1,
                    "Webhook delivery failed"
                );
            }
        }

        if attempt < max_retries {
            let delay = retry_base_delay * 2u32.pow(attempt);
            tokio::time::sleep(delay).await;
        }
    }

    counter!("strix_webhook_deliveries_total", "status" => "failed").increment(1);
    error!(
        url,
        attempts = max_retries + 1,
        "Webhook delivery failed after all retries"
    );
    DeliveryOutcome {
        attempts: max_retries + 1,
        status: DeliveryStatus::Failed,
        response_code: last_response_code,
        last_error,
    }
}
