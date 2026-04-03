#![allow(dead_code)]

//! Prometheus metrics for Strix.
//!
//! This module provides metric definitions and initialization for
//! the Prometheus metrics exporter.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize the Prometheus metrics exporter and register metric descriptions.
///
/// Returns a handle that can be used to render metrics.
pub fn init_metrics() -> PrometheusHandle {
    // Build the Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    // Register metric descriptions
    register_metrics();

    handle
}

/// Register all metric descriptions.
fn register_metrics() {
    // Server info
    describe_gauge!(
        "strix_info",
        "Strix server information (always 1, labels contain version)"
    );

    // S3 API request metrics
    describe_counter!("strix_s3_requests_total", "Total number of S3 API requests");
    describe_counter!(
        "strix_s3_request_errors_total",
        "Total number of S3 API request errors"
    );
    describe_histogram!(
        "strix_s3_request_duration_seconds",
        "S3 API request duration in seconds"
    );

    // Object operations
    describe_counter!(
        "strix_s3_objects_created_total",
        "Total number of objects created (PutObject)"
    );
    describe_counter!(
        "strix_s3_objects_deleted_total",
        "Total number of objects deleted"
    );
    describe_counter!(
        "strix_s3_objects_retrieved_total",
        "Total number of objects retrieved (GetObject)"
    );

    // Bytes transferred
    describe_counter!(
        "strix_s3_bytes_received_total",
        "Total bytes received (uploads)"
    );
    describe_counter!("strix_s3_bytes_sent_total", "Total bytes sent (downloads)");

    // Bucket operations
    describe_counter!(
        "strix_s3_buckets_created_total",
        "Total number of buckets created"
    );
    describe_counter!(
        "strix_s3_buckets_deleted_total",
        "Total number of buckets deleted"
    );

    // Multipart uploads
    describe_counter!(
        "strix_s3_multipart_uploads_started_total",
        "Total number of multipart uploads started"
    );
    describe_counter!(
        "strix_s3_multipart_uploads_completed_total",
        "Total number of multipart uploads completed"
    );
    describe_counter!(
        "strix_s3_multipart_uploads_aborted_total",
        "Total number of multipart uploads aborted"
    );

    // Authentication
    describe_counter!("strix_auth_attempts_total", "Total authentication attempts");
    describe_counter!("strix_auth_failures_total", "Total authentication failures");

    // Current state gauges
    describe_gauge!("strix_buckets_count", "Current number of buckets");
    describe_gauge!("strix_storage_bytes_used", "Current storage bytes used");
}

/// Record an S3 request.
pub fn record_s3_request(operation: &str) {
    counter!("strix_s3_requests_total", "operation" => operation.to_string()).increment(1);
}

/// Record an S3 request error.
pub fn record_s3_error(operation: &str, error_code: &str) {
    counter!(
        "strix_s3_request_errors_total",
        "operation" => operation.to_string(),
        "error_code" => error_code.to_string()
    )
    .increment(1);
}

/// Record S3 request duration.
pub fn record_s3_duration(operation: &str, duration: Duration) {
    histogram!(
        "strix_s3_request_duration_seconds",
        "operation" => operation.to_string()
    )
    .record(duration.as_secs_f64());
}

/// Record bytes received (upload).
pub fn record_bytes_received(bytes: u64) {
    counter!("strix_s3_bytes_received_total").increment(bytes);
}

/// Record bytes sent (download).
pub fn record_bytes_sent(bytes: u64) {
    counter!("strix_s3_bytes_sent_total").increment(bytes);
}

/// Record object created.
pub fn record_object_created() {
    counter!("strix_s3_objects_created_total").increment(1);
}

/// Record object deleted.
pub fn record_object_deleted() {
    counter!("strix_s3_objects_deleted_total").increment(1);
}

/// Record object retrieved.
pub fn record_object_retrieved() {
    counter!("strix_s3_objects_retrieved_total").increment(1);
}

/// Record bucket created.
pub fn record_bucket_created() {
    counter!("strix_s3_buckets_created_total").increment(1);
}

/// Record bucket deleted.
pub fn record_bucket_deleted() {
    counter!("strix_s3_buckets_deleted_total").increment(1);
}

/// Record multipart upload started.
pub fn record_multipart_started() {
    counter!("strix_s3_multipart_uploads_started_total").increment(1);
}

/// Record multipart upload completed.
pub fn record_multipart_completed() {
    counter!("strix_s3_multipart_uploads_completed_total").increment(1);
}

/// Record multipart upload aborted.
pub fn record_multipart_aborted() {
    counter!("strix_s3_multipart_uploads_aborted_total").increment(1);
}

/// Record authentication attempt.
pub fn record_auth_attempt(success: bool) {
    counter!("strix_auth_attempts_total").increment(1);
    if !success {
        counter!("strix_auth_failures_total").increment(1);
    }
}

/// Update current bucket count gauge.
pub fn set_bucket_count(count: u64) {
    gauge!("strix_buckets_count").set(count as f64);
}

/// Update current storage usage gauge.
pub fn set_storage_bytes(bytes: u64) {
    gauge!("strix_storage_bytes_used").set(bytes as f64);
}

/// Set server info metric (call once at startup).
pub fn set_server_info(version: &str) {
    gauge!("strix_info", "version" => version.to_string()).set(1.0);
}
