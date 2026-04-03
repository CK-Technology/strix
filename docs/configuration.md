# Configuration Guide

Strix can be configured through command-line flags, environment variables, or a configuration file.

## Configuration Methods

Configuration is applied in the following order of precedence (highest to lowest):
1. Command-line flags
2. Environment variables
3. Configuration file
4. Default values

## Command-Line Flags

```bash
strix [OPTIONS]

Options:
  --address <ADDRESS>
      S3 API listen address [default: 0.0.0.0:9000]

  --console-address <ADDRESS>
      Web console and Admin API listen address [default: 0.0.0.0:9001]

  --metrics-address <ADDRESS>
      Prometheus metrics listen address [default: 127.0.0.1:9090]

  --data-dir <PATH>
      Data storage directory [default: /var/lib/strix]

  --root-user <USER>
      Root access key ID (required)

  --root-password <PASSWORD>
      Root secret access key (required)

  --jwt-secret <BASE64>
      JWT signing secret in base64 (must decode to at least 32 bytes)

  --log-level <LEVEL>
      Log level: trace, debug, info, warn, error [default: info]

  --region <REGION>
      S3 region name [default: us-east-1]

  --config <PATH>
      Path to configuration file

  -h, --help
      Print help information

  -V, --version
      Print version information
```

## Environment Variables

All configuration options can be set via environment variables with the `STRIX_` prefix:

| Variable | Description | Default |
|----------|-------------|---------|
| `STRIX_ADDRESS` | S3 API listen address | `0.0.0.0:9000` |
| `STRIX_CONSOLE_ADDRESS` | Web console address | `0.0.0.0:9001` |
| `STRIX_METRICS_ADDRESS` | Prometheus metrics address | `127.0.0.1:9090` |
| `STRIX_DATA_DIR` | Data storage directory | `/var/lib/strix` |
| `STRIX_ROOT_USER` | Root access key ID | (required) |
| `STRIX_ROOT_PASSWORD` | Root secret access key | (required) |
| `STRIX_JWT_SECRET` | JWT signing secret (base64, >=32 decoded bytes) | (unset) |
| `STRIX_LOG_LEVEL` | Log level | `info` |
| `STRIX_LOG_JSON` | Enable JSON log format | `false` |
| `STRIX_S3_RATE_LIMIT` | Max S3 requests per minute per IP (0=disabled) | `1000` |
| `STRIX_MULTIPART_EXPIRY_HOURS` | Hours before stale multipart uploads are cleaned | `24` |

### Example

```bash
export STRIX_ROOT_USER=admin
export STRIX_ROOT_PASSWORD=supersecretpassword
export STRIX_DATA_DIR=/data/strix
export STRIX_LOG_LEVEL=debug
export STRIX_JWT_SECRET=$(openssl rand -base64 32)

./strix
```

### Session Security Behavior

- `STRIX_JWT_SECRET` configured: admin JWT sessions remain valid across process restarts.
- `STRIX_JWT_SECRET` unset: a random key is generated on boot; all admin sessions are invalidated on restart.

## Configuration File

Strix supports TOML configuration files:

```toml
# /etc/strix/config.toml

# Server settings
address = "0.0.0.0:9000"
console_address = "0.0.0.0:9001"
metrics_address = "0.0.0.0:9090"

# Storage
data_dir = "/var/lib/strix"

# Authentication (can also use environment variables)
# root_user = "admin"
# root_password = "password"

# Logging
log_level = "info"

# S3 settings
region = "us-east-1"
```

Load with:
```bash
strix --config /etc/strix/config.toml
```

## Data Directory Structure

The data directory contains all persistent data:

```
/var/lib/strix/
├── .strix/
│   ├── iam.db              # SQLite database for IAM
│   └── config.json         # Runtime configuration
└── buckets/
    └── {bucket-name}/
        ├── .bucket.meta    # Bucket metadata (MessagePack)
        └── objects/
            └── {key}/
                ├── xl.meta # Object metadata (MessagePack)
                └── part.1  # Object data
```

## TLS Configuration

For production deployments, place Strix behind a reverse proxy (nginx, Caddy, Traefik) that handles TLS termination.

### Example: Nginx

```nginx
server {
    listen 443 ssl http2;
    server_name s3.example.com;

    ssl_certificate /etc/ssl/certs/s3.example.com.crt;
    ssl_certificate_key /etc/ssl/private/s3.example.com.key;

    # S3 API
    location / {
        proxy_pass http://127.0.0.1:9000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # For large uploads
        client_max_body_size 5G;
        proxy_request_buffering off;
    }
}

server {
    listen 443 ssl http2;
    server_name console.s3.example.com;

    ssl_certificate /etc/ssl/certs/s3.example.com.crt;
    ssl_certificate_key /etc/ssl/private/s3.example.com.key;

    # Web console
    location / {
        proxy_pass http://127.0.0.1:9001;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Example: Caddy

```caddyfile
s3.example.com {
    reverse_proxy localhost:9000
}

console.s3.example.com {
    reverse_proxy localhost:9001
}
```

## Docker Configuration

### Docker Run

```bash
docker run -d \
  --name strix \
  -p 9000:9000 \
  -p 9001:9001 \
  -p 9090:9090 \
  -e STRIX_ROOT_USER=admin \
  -e STRIX_ROOT_PASSWORD=password123 \
  -v strix-data:/var/lib/strix \
  ghcr.io/strix-storage/strix:latest
```

### Docker Compose

```yaml
version: '3.8'

services:
  strix:
    image: ghcr.io/strix-storage/strix:latest
    container_name: strix
    restart: unless-stopped
    ports:
      - "9000:9000"   # S3 API
      - "9001:9001"   # Web console
      - "9090:9090"   # Metrics
    environment:
      STRIX_ROOT_USER: admin
      STRIX_ROOT_PASSWORD: ${STRIX_PASSWORD:-changeme}
      STRIX_LOG_LEVEL: info
    volumes:
      - strix-data:/var/lib/strix
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9001/api/v1/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  strix-data:
```

## Kubernetes Configuration

### ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: strix-config
data:
  STRIX_ADDRESS: "0.0.0.0:9000"
  STRIX_CONSOLE_ADDRESS: "0.0.0.0:9001"
  STRIX_LOG_LEVEL: "info"
  STRIX_REGION: "us-east-1"
```

### Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: strix-credentials
type: Opaque
stringData:
  STRIX_ROOT_USER: admin
  STRIX_ROOT_PASSWORD: supersecretpassword
```

### Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: strix
spec:
  replicas: 1
  selector:
    matchLabels:
      app: strix
  template:
    metadata:
      labels:
        app: strix
    spec:
      containers:
      - name: strix
        image: ghcr.io/strix-storage/strix:latest
        ports:
        - containerPort: 9000
          name: s3
        - containerPort: 9001
          name: console
        - containerPort: 9090
          name: metrics
        envFrom:
        - configMapRef:
            name: strix-config
        - secretRef:
            name: strix-credentials
        volumeMounts:
        - name: data
          mountPath: /var/lib/strix
        livenessProbe:
          httpGet:
            path: /api/v1/health
            port: 9001
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /api/v1/health
            port: 9001
          initialDelaySeconds: 5
          periodSeconds: 10
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: strix-pvc
```

### Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: strix
spec:
  selector:
    app: strix
  ports:
  - name: s3
    port: 9000
    targetPort: 9000
  - name: console
    port: 9001
    targetPort: 9001
  - name: metrics
    port: 9090
    targetPort: 9090
```

## Logging

Strix uses structured logging with configurable levels:

- `trace` - Very detailed debugging information
- `debug` - Debugging information
- `info` - General operational information (default)
- `warn` - Warning messages
- `error` - Error messages

Logs are written to stdout in JSON format for easy parsing:

```json
{"timestamp":"2024-01-01T00:00:00.000Z","level":"INFO","target":"strix","message":"Server started on 0.0.0.0:9000"}
```

## Monitoring

### Prometheus Metrics

Strix exposes Prometheus metrics on the configured metrics address (default: `:9090`):

```
# HELP strix_requests_total Total number of requests
# TYPE strix_requests_total counter
strix_requests_total{method="GET",endpoint="GetObject"} 1234

# HELP strix_request_duration_seconds Request duration in seconds
# TYPE strix_request_duration_seconds histogram
strix_request_duration_seconds_bucket{method="GET",le="0.01"} 100

# HELP strix_objects_total Total number of objects
# TYPE strix_objects_total gauge
strix_objects_total 5000

# HELP strix_storage_bytes_total Total storage used in bytes
# TYPE strix_storage_bytes_total gauge
strix_storage_bytes_total 1073741824
```

### Grafana Dashboard

Import the Strix Grafana dashboard from `docs/grafana-dashboard.json` (coming soon).

## Security Recommendations

1. **Use strong credentials**: Generate random access keys and secrets
2. **Enable TLS**: Always use HTTPS in production
3. **Network isolation**: Run Strix in a private network
4. **Regular backups**: Back up the data directory regularly
5. **Least privilege**: Create IAM users with minimal required permissions
6. **Audit logging**: Enable audit logging for compliance (coming soon)
