# Strix Architecture

This document describes the internal architecture of Strix, a modern S3-compatible object storage server written in Rust.

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                          Strix Server                            │
├──────────────┬──────────────┬──────────────┬───────────────────┤
│   S3 API     │  Admin API   │  Metrics     │  Web Console      │
│   (s3s)      │  (axum)      │  (prometheus)│  (leptos)         │
│   :9000      │  :9001       │  :9002       │  :9001            │
├──────────────┴──────────────┴──────────────┴───────────────────┤
│                      Authentication Layer                        │
│              AWS Sig V4  │  JWT Sessions  │  Rate Limiting      │
├─────────────────────────────────────────────────────────────────┤
│                      Authorization Layer                         │
│                IAM Policies  │  Bucket Policies                  │
├──────────────┬──────────────┬───────────────────────────────────┤
│   IAM Store  │   Storage    │          Crypto                   │
│   (SQLite)   │   (LocalFS)  │   (AES-256-GCM, Argon2id)        │
└──────────────┴──────────────┴───────────────────────────────────┘
```

## Crate Structure

### strix (main binary)

The main entry point that wires together all components:
- Configuration parsing (CLI + environment)
- Service initialization
- HTTP server setup (Axum)
- S3 service mounting (s3s)
- Background task spawning

### strix-core

Shared types and traits used across all crates:
- `ObjectStore` trait - Storage abstraction
- Data types (BucketInfo, ObjectInfo, etc.)
- Error types (Error enum)
- Result type alias

### strix-storage

Storage backend implementation:
- `LocalFsStore` - Local filesystem storage
- SQLite metadata database
- Blob storage with ULID-based naming
- Multipart upload handling
- Versioning support
- Encryption (SSE-S3, SSE-C)

### strix-s3

S3 API implementation using the `s3s` crate:
- `StrixS3Service` - s3s::S3 trait implementation
- AWS Signature V4 authentication
- Request validation
- Error conversion to S3 format

### strix-iam

Identity and Access Management:
- User management
- Group management
- Access key generation
- Policy evaluation
- Password hashing (Argon2id)
- Secret encryption (AES-256-GCM)

### strix-admin

Admin REST API:
- User/group/policy CRUD
- Access key management
- Bucket management via admin API
- Pre-signed URL generation
- Audit log queries
- JWT session management

### strix-crypto

Cryptographic utilities:
- AWS Signature V4 signing/verification
- AES-256-GCM encryption/decryption
- Key derivation
- Hash functions (SHA-256, MD5)

### strix-gui

Web console (Leptos WASM):
- Login page
- Dashboard
- Bucket browser
- User management UI

## Data Flow

### S3 Request Flow

```
Client Request
      │
      ▼
┌─────────────┐
│   Axum      │  HTTP Server
└─────────────┘
      │
      ▼
┌─────────────┐
│    s3s      │  S3 Protocol Layer
│  (routing)  │  - Parse S3 request
└─────────────┘  - Extract bucket/key
      │
      ▼
┌─────────────┐
│ Auth Layer  │  Authentication
│  (Sig V4)   │  - Verify signature
└─────────────┘  - Look up credentials
      │
      ▼
┌─────────────┐
│ IAM Policy  │  Authorization
│  Evaluator  │  - Check user policies
└─────────────┘  - Check bucket policies
      │
      ▼
┌─────────────┐
│  Storage    │  Operation
│  Backend    │  - Execute request
└─────────────┘  - Return response
      │
      ▼
   Response
```

### Storage Architecture

```
data/
├── .strix/
│   └── strix.db           # SQLite metadata database
├── blobs/
│   └── {first-2-chars}/
│       └── {ulid}.blob    # Object data files
└── multipart/
    └── {upload-id}/
        └── part.{n}       # Part files (during upload)
```

**Database Schema:**

- `buckets` - Bucket metadata (name, region, versioning)
- `objects` - Object metadata (key, size, etag, encryption)
- `multipart_uploads` - In-progress uploads
- `parts` - Uploaded parts
- `bucket_cors` - CORS configurations
- `bucket_lifecycle` - Lifecycle rules
- `bucket_notification` - Event notification rules
- `bucket_object_lock` - Object lock configurations
- `audit_log` - Request audit trail

### IAM Architecture

```
┌─────────────────────────────────────────┐
│              IAM Store (SQLite)          │
├─────────────────────────────────────────┤
│  users          │  access_keys          │
│  - username     │  - access_key_id      │
│  - arn          │  - secret_key (enc)   │
│  - password_hash│  - username           │
│  - status       │  - status             │
├─────────────────┼───────────────────────┤
│  groups         │  policies             │
│  - name         │  - name               │
│  - arn          │  - document (JSON)    │
│  - members      │  - description        │
├─────────────────┴───────────────────────┤
│  user_policies  │  group_policies       │
│  (attachments)  │  (attachments)        │
├─────────────────┴───────────────────────┤
│           bucket_policies               │
│  - bucket -> policy document            │
└─────────────────────────────────────────┘
```

**Policy Evaluation:**

1. Check user's attached policies
2. Check groups' attached policies (for each group user is in)
3. Check bucket policy (for bucket operations)
4. Deny by default unless explicitly allowed
5. Explicit Deny always wins

## Security

### Authentication

**AWS Signature V4:**
- Supports both header and query string authentication
- Validates signature against stored secret key
- Verifies request timestamp (±15 minutes)

**JWT Sessions (Admin API):**
- HS256 signed tokens
- 24-hour expiration
- Contains username and access key ID

### Authorization

**IAM Policies:**
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:ListBucket"],
      "Resource": ["arn:aws:s3:::bucket/*", "arn:aws:s3:::bucket"]
    }
  ]
}
```

**Bucket Policies:**
- Applied to all principals accessing the bucket
- Supports Principal, Action, Resource, Condition

**Condition Support:**
- `IpAddress` / `NotIpAddress`
- `StringEquals` / `StringNotEquals`
- `StringLike` / `StringNotLike`
- `Bool`
- `NumericEquals` / `NumericLessThan` / etc.

### Encryption

**At Rest:**
- SSE-S3: Server-managed keys (AES-256-GCM)
  - Key derived from: master key + bucket + key + version
- SSE-C: Customer-provided keys (AES-256-GCM)
  - Key validated via MD5 hash

**Secrets:**
- Access key secrets encrypted with AES-256-GCM
- Passwords hashed with Argon2id
- Encryption key derived from server master key

## Performance

### Concurrency

- Async I/O with Tokio runtime
- SQLite with WAL mode for concurrent reads
- Connection pooling via tokio-rusqlite

### Caching

- Object metadata cached in SQLite (fast lookups)
- No in-memory object caching (relies on OS page cache)

### Blob Storage

- ULID-based naming (time-sorted, unique)
- Two-level directory structure (first 2 chars)
- Direct file access (no additional indirection)

## Scalability

### Current Limitations (v0.1)

- **Single Node:** No distributed mode
- **Single Disk:** No erasure coding or replication
- **SQLite:** Limited concurrent write throughput

### Future Plans

- Distributed mode with Raft consensus
- Erasure coding for data protection
- Sharded metadata storage
- Read replicas

## Observability

### Metrics (Prometheus)

```
# Request metrics
strix_s3_requests_total{method,operation,status}
strix_s3_request_duration_seconds{method,operation}

# Storage metrics
strix_storage_objects_total{bucket}
strix_storage_bytes_total{bucket}
strix_storage_buckets_total

# System metrics
strix_uptime_seconds
strix_info{version,commit}
```

### Logging

- Structured logging with `tracing`
- Request tracing with span context
- Configurable log levels (error/warn/info/debug/trace)

### Audit Logging

- All S3 operations logged to SQLite
- Includes: operation, bucket, key, principal, status, duration
- Queryable via Admin API

## Testing

### Unit Tests

```bash
cargo test --workspace
```

### Integration Tests

```bash
# Start server
STRIX_ROOT_USER=admin STRIX_ROOT_PASSWORD=pass cargo run &

# Run integration tests
STRIX_TEST_ENDPOINT=http://localhost:9000 \
STRIX_TEST_ACCESS_KEY=admin \
STRIX_TEST_SECRET_KEY=pass \
cargo test -p strix-integration-tests
```

### Conformance Testing

The integration tests verify S3 API conformance:
- Basic operations (bucket, object CRUD)
- Multipart uploads
- Versioning
- Authentication
- Error codes
