# Observability

Strix provides comprehensive observability through metrics, structured logging, and distributed tracing.

## Metrics

Strix exposes Prometheus metrics on the metrics endpoint (default: `127.0.0.1:9090`).

By default, metrics are loopback-only to reduce accidental exposure. To expose metrics externally, set `STRIX_METRICS_ADDRESS` explicitly and place the endpoint behind appropriate network controls.

## Stress and Recovery Test Switches

Integration stress coverage includes large multipart data-path tests and restart/recovery durability tests.

- `STRIX_STRESS_TESTS=1` enables heavy multipart stress test cases (disabled by default for fast local runs).
- `STRIX_TEST_BIN` sets the Strix binary path used by process lifecycle recovery tests (default: `./target/debug/strix`).

### Endpoint

```bash
curl http://localhost:9090/metrics
```

### Available Metrics

#### S3 API Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `strix_s3_requests_total` | Counter | Total S3 API requests |
| `strix_s3_request_duration_seconds` | Histogram | Request duration |
| `strix_s3_request_size_bytes` | Histogram | Request body size |
| `strix_s3_response_size_bytes` | Histogram | Response body size |
| `strix_s3_errors_total` | Counter | S3 API errors by code |

#### Storage Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `strix_storage_objects_total` | Gauge | Total objects stored |
| `strix_storage_bytes_total` | Gauge | Total storage used |
| `strix_storage_buckets_total` | Gauge | Number of buckets |
| `strix_storage_multipart_uploads_active` | Gauge | In-progress uploads |

#### IAM Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `strix_iam_users_total` | Gauge | Number of users |
| `strix_iam_access_keys_total` | Gauge | Number of access keys |
| `strix_iam_auth_success_total` | Counter | Successful authentications |
| `strix_iam_auth_failures_total` | Counter | Failed authentications |

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'strix'
    static_configs:
      - targets: ['strix:9090']
    scrape_interval: 15s
```

### Grafana Dashboard

Import the Strix dashboard from `docs/assets/grafana-dashboard.json` or use dashboard ID `xxxxx` from Grafana.com.

## Structured Logging

### Configuration

```bash
# Enable JSON logs
STRIX_LOG_JSON=true

# Set log level
STRIX_LOG_LEVEL=info

# Fine-grained log levels
STRIX_LOG_LEVEL="strix=debug,tower_http=info,sqlx=warn"
```

### Log Format

JSON format includes:

```json
{
  "timestamp": "2024-01-15T10:30:00.123Z",
  "level": "INFO",
  "message": "S3 request completed",
  "target": "strix_s3::service",
  "fields": {
    "method": "GET",
    "bucket": "my-bucket",
    "key": "path/to/object",
    "status": 200,
    "duration_ms": 15,
    "request_id": "abc123"
  },
  "span": {
    "name": "s3_request",
    "request_id": "abc123"
  }
}
```

### Log Levels

| Level | Description |
|-------|-------------|
| `error` | Failures requiring attention |
| `warn` | Recoverable issues |
| `info` | Normal operations |
| `debug` | Detailed debugging info |
| `trace` | Very detailed tracing |

## Distributed Tracing (OpenTelemetry)

### Enabling OpenTelemetry

Build with the `otel` feature:

```bash
cargo build --release --features otel
```

Configure the OTLP endpoint:

```bash
# Send traces to Jaeger/Tempo/etc.
STRIX_OTLP_ENDPOINT=http://localhost:4317
STRIX_SERVICE_NAME=strix-prod
```

### Trace Attributes

Each span includes:

- `service.name`: Service identifier
- `http.method`: HTTP method
- `http.url`: Request URL
- `http.status_code`: Response status
- `s3.bucket`: Bucket name (when applicable)
- `s3.key`: Object key (when applicable)
- `s3.operation`: S3 operation name

### Jaeger Configuration

```yaml
# docker-compose.yml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP gRPC
      - "4318:4318"    # OTLP HTTP

  strix:
    image: strix:latest
    environment:
      STRIX_OTLP_ENDPOINT: http://jaeger:4317
      STRIX_SERVICE_NAME: strix
```

### Grafana Tempo Configuration

```yaml
# tempo.yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

# datasources.yaml
datasources:
  - name: Tempo
    type: tempo
    url: http://tempo:3200
```

## Health Checks

### Endpoints

| Endpoint | Description |
|----------|-------------|
| `/health/live` | Liveness probe |
| `/health/ready` | Readiness probe |
| `/minio/health/live` | MinIO-compatible liveness |
| `/minio/health/ready` | MinIO-compatible readiness |

### Kubernetes Probes

```yaml
# deployment.yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 9000
  initialDelaySeconds: 5
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /health/ready
    port: 9000
  initialDelaySeconds: 5
  periodSeconds: 5
```

## Alert Examples

### Prometheus Alerting Rules

```yaml
# alerts.yaml
groups:
  - name: strix
    rules:
      - alert: StrixDown
        expr: up{job="strix"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: Strix server is down

      - alert: StrixHighErrorRate
        expr: |
          rate(strix_s3_errors_total[5m])
          / rate(strix_s3_requests_total[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: High error rate on S3 API

      - alert: StrixHighLatency
        expr: |
          histogram_quantile(0.95,
            rate(strix_s3_request_duration_seconds_bucket[5m])
          ) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: High P95 latency on S3 API

      - alert: StrixStorageNearFull
        expr: |
          strix_storage_bytes_total
          / strix_storage_capacity_bytes > 0.9
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: Storage is 90% full

      - alert: StrixAuthFailures
        expr: rate(strix_iam_auth_failures_total[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: High rate of authentication failures
```

## Log Aggregation

### Vector Configuration

```toml
# vector.toml
[sources.strix_logs]
type = "file"
include = ["/var/log/strix/*.log"]

[transforms.parse_json]
type = "remap"
inputs = ["strix_logs"]
source = '''
. = parse_json!(.message)
'''

[sinks.loki]
type = "loki"
inputs = ["parse_json"]
endpoint = "http://loki:3100"
labels.job = "strix"
labels.level = "{{ level }}"
```

### Fluent Bit Configuration

```ini
[INPUT]
    Name   tail
    Path   /var/log/strix/*.log
    Parser json

[OUTPUT]
    Name   loki
    Match  *
    Host   loki
    Port   3100
    Labels job=strix
```
