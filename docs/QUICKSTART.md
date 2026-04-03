# Strix Quick Start Guide

This guide will help you get Strix up and running in minutes.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/ghostkellz/strix.git
cd strix

# Build in release mode
cargo build --release

# The binary will be at target/release/strix
```

### Using Cargo

```bash
cargo install strix
```

## Running Strix

### Basic Usage

```bash
# Start with default settings
strix

# Or specify options
strix --data-dir ./data --s3-address 0.0.0.0:9000 --console-address 0.0.0.0:9001
```

### Environment Variables

```bash
# Set root credentials (required)
export STRIX_ROOT_USER=admin
export STRIX_ROOT_PASSWORD=adminpass

# Start the server
strix --data-dir /var/lib/strix
```

### Command Line Options

```
Options:
  --data-dir <PATH>        Data directory [default: ./data]
  --s3-address <ADDR>      S3 API address [default: 0.0.0.0:9000]
  --console-address <ADDR> Admin console address [default: 0.0.0.0:9001]
  --metrics-address <ADDR> Metrics endpoint address [default: 0.0.0.0:9002]
  --region <REGION>        AWS region [default: us-east-1]
  --log-level <LEVEL>      Log level [default: info]
  -h, --help               Print help
  -V, --version            Print version
```

## First Steps

### 1. Create a Bucket

Using the AWS CLI:

```bash
# Configure credentials
export AWS_ACCESS_KEY_ID=admin
export AWS_SECRET_ACCESS_KEY=adminpass
export AWS_ENDPOINT_URL=http://localhost:9000

# Create a bucket
aws s3 mb s3://my-bucket

# List buckets
aws s3 ls
```

### 2. Upload an Object

```bash
# Upload a file
aws s3 cp myfile.txt s3://my-bucket/

# Upload with metadata
aws s3 cp myfile.txt s3://my-bucket/ --metadata "author=alice,version=1.0"
```

### 3. Download an Object

```bash
# Download a file
aws s3 cp s3://my-bucket/myfile.txt ./downloaded.txt

# Download with progress
aws s3 cp s3://my-bucket/largefile.zip ./ --no-progress
```

### 4. List Objects

```bash
# List all objects
aws s3 ls s3://my-bucket/

# List with prefix
aws s3 ls s3://my-bucket/uploads/

# Recursive listing
aws s3 ls s3://my-bucket/ --recursive
```

## User Management

### Create a User

Using the Admin API:

```bash
# Login to get a token
TOKEN=$(curl -s -X POST http://localhost:9001/api/v1/login \
  -H "Content-Type: application/json" \
  -d '{"access_key_id":"admin","secret_access_key":"adminpass"}' \
  | jq -r '.token')

# Create a user
curl -X POST http://localhost:9001/api/v1/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username":"alice"}'
```

### Create Access Keys

```bash
# Create access key for user
curl -X POST http://localhost:9001/api/v1/users/alice/keys \
  -H "Authorization: Bearer $TOKEN"
```

Save the returned `access_key_id` and `secret_access_key` - the secret is only shown once.

## Enable Versioning

```bash
# Enable versioning on a bucket
aws s3api put-bucket-versioning \
  --bucket my-bucket \
  --versioning-configuration Status=Enabled

# Check versioning status
aws s3api get-bucket-versioning --bucket my-bucket
```

## Server-Side Encryption

### SSE-S3 (Server-Managed Keys)

```bash
# Upload with SSE-S3
aws s3 cp myfile.txt s3://my-bucket/ \
  --sse AES256
```

### SSE-C (Customer-Provided Keys)

```bash
# Generate a 256-bit key
KEY=$(openssl rand -base64 32)
KEY_MD5=$(echo -n "$KEY" | base64 -d | openssl md5 -binary | base64)

# Upload with SSE-C
aws s3 cp myfile.txt s3://my-bucket/ \
  --sse-c AES256 \
  --sse-c-key "$KEY"

# Download with SSE-C (same key required)
aws s3 cp s3://my-bucket/myfile.txt ./decrypted.txt \
  --sse-c AES256 \
  --sse-c-key "$KEY"
```

## Multipart Upload

For large files (>5GB), use multipart upload:

```bash
# AWS CLI handles this automatically for large files
aws s3 cp largefile.zip s3://my-bucket/

# Or use the low-level API
aws s3api create-multipart-upload --bucket my-bucket --key largefile.zip
# ... upload parts ...
aws s3api complete-multipart-upload --bucket my-bucket --key largefile.zip --upload-id <id>
```

## Pre-signed URLs

Generate temporary URLs for sharing:

```bash
# Generate a download URL (valid for 1 hour)
curl -X POST http://localhost:9001/api/v1/presign \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "bucket": "my-bucket",
    "key": "myfile.txt",
    "method": "GET",
    "expires_in": 3600
  }'
```

## Monitoring

### Prometheus Metrics

Strix exposes Prometheus metrics at `http://localhost:9002/metrics`:

```
# Request metrics
strix_s3_requests_total{method="GET",operation="GetObject",status="200"} 1234

# Storage metrics
strix_storage_objects_total{bucket="my-bucket"} 100
strix_storage_bytes_total{bucket="my-bucket"} 1073741824
```

### Health Checks

```bash
# Liveness probe (for Kubernetes)
curl http://localhost:9000/minio/health/live

# Readiness probe
curl http://localhost:9001/api/v1/health
```

## Web Console

Strix includes a web-based admin console at `http://localhost:9001`.

Features:
- Dashboard with storage statistics
- Bucket browser with folder navigation
- Object management (upload, download, delete)
- User and access key management

## Using with Docker

```dockerfile
FROM rust:1.85 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/strix /usr/local/bin/
EXPOSE 9000 9001 9002
VOLUME /data
ENV STRIX_ROOT_USER=admin
ENV STRIX_ROOT_PASSWORD=adminpass
CMD ["strix", "--data-dir", "/data"]
```

```bash
# Build and run
docker build -t strix .
docker run -d -p 9000:9000 -p 9001:9001 -v strix-data:/data strix
```

## Troubleshooting

### Common Issues

**Cannot connect to S3 endpoint:**
- Check that the server is running: `curl http://localhost:9000/minio/health/live`
- Verify the endpoint URL in your client configuration
- Ensure `force_path_style` is enabled (not virtual-hosted style)

**Authentication failures:**
- Verify your access key and secret are correct
- Check that the user has the necessary permissions
- Ensure the access key is in "Active" status

**Bucket operations fail:**
- Check bucket name validity (3-63 chars, lowercase, no special chars)
- Ensure the bucket doesn't already exist (for create)
- Ensure the bucket is empty (for delete)

### Logs

Strix logs to stderr with configurable levels:

```bash
# Run with debug logging
strix --log-level debug

# JSON format for log aggregation
RUST_LOG=strix=debug,tower_http=debug strix
```

## Next Steps

- Read the [S3 Compatibility Guide](s3-compatibility.md) for API details
- Run [Tool Compatibility Testing](tool-compatibility-testing.md) for AWS CLI and restic validation
- See the [Admin API Reference](ADMIN_API.md) for management operations
- Check the [Architecture Overview](ARCHITECTURE.md) for internals
