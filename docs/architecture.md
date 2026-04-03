# Architecture

This document describes the internal architecture of Strix.

## High-Level Overview

```
                                    ┌──────────────────────────────────────────────────┐
                                    │                    Strix                          │
                                    │                                                   │
  ┌─────────────┐                   │  ┌─────────────┐  ┌─────────────┐  ┌───────────┐ │
  │  S3 Client  │──────────────────────▶│   S3 API    │  │ Admin API   │  │    GUI    │ │
  │ (AWS SDK)   │       :9000       │  │   (s3s)     │  │  (Axum)     │  │ (Leptos)  │ │
  └─────────────┘                   │  └──────┬──────┘  └──────┬──────┘  └─────┬─────┘ │
                                    │         │                │               │       │
  ┌─────────────┐                   │         └────────────────┼───────────────┘       │
  │   Browser   │──────────────────────────────────────────────┼───────────────────────│
  │             │       :9001       │                          │                       │
  └─────────────┘                   │                          ▼                       │
                                    │  ┌───────────────────────────────────────────┐   │
                                    │  │              Core Services                 │   │
                                    │  │                                           │   │
                                    │  │  ┌───────────┐ ┌──────────┐ ┌──────────┐  │   │
                                    │  │  │    IAM    │ │  Policy  │ │  Crypto  │  │   │
                                    │  │  │  Store    │ │  Engine  │ │ (Sig V4) │  │   │
                                    │  │  │ (SQLite)  │ │          │ │          │  │   │
                                    │  │  └───────────┘ └──────────┘ └──────────┘  │   │
                                    │  └───────────────────────────────────────────┘   │
                                    │                          │                       │
                                    │                          ▼                       │
                                    │  ┌───────────────────────────────────────────┐   │
                                    │  │             Storage Layer                  │   │
                                    │  │                                           │   │
                                    │  │  ┌─────────────────────────────────────┐  │   │
                                    │  │  │      Local Filesystem Backend       │  │   │
                                    │  │  │   (MessagePack metadata + files)    │  │   │
                                    │  │  └─────────────────────────────────────┘  │   │
                                    │  └───────────────────────────────────────────┘   │
                                    │                          │                       │
                                    └──────────────────────────┼───────────────────────┘
                                                               │
                                                               ▼
                                                    ┌─────────────────────┐
                                                    │    Filesystem       │
                                                    │   /var/lib/strix    │
                                                    └─────────────────────┘
```

## Crate Structure

Strix is organized as a Cargo workspace with multiple crates:

```
strix/
├── strix/                    # Main binary (glue code)
├── crates/
│   ├── strix-core/           # Shared types, traits, errors
│   ├── strix-s3/             # S3 API implementation
│   ├── strix-storage/        # Storage backend abstraction
│   ├── strix-iam/            # IAM: users, policies, access keys
│   ├── strix-crypto/         # Cryptographic operations
│   ├── strix-admin/          # Admin REST API
│   ├── strix-gui/            # Leptos web console
│   └── strix-cli/            # CLI tool (sx)
└── xtask/                    # Build automation
```

### Dependency Graph

```
strix (binary)
├── strix-s3
│   ├── s3s (S3 protocol)
│   ├── strix-storage
│   ├── strix-iam
│   └── strix-crypto
├── strix-admin
│   ├── axum
│   ├── strix-storage
│   └── strix-iam
├── strix-gui
│   ├── leptos
│   └── rust-embed
└── strix-core
    └── (base types)
```

## Components

### strix-core

Foundational types and traits shared across crates.

**Key Types:**
- `ObjectStore` trait - Storage backend interface
- `StrixError` - Common error types
- `BucketInfo`, `ObjectInfo` - Metadata structures
- `ObjectData` - Streaming object data wrapper

### strix-s3

S3 API implementation using the `s3s` crate.

**Components:**
- `StrixS3Service` - Implements `s3s::S3` trait
- Request handlers for all S3 operations
- XML serialization/deserialization
- Multipart upload management

**Request Flow:**
```
HTTP Request
    │
    ▼
┌─────────────────┐
│  s3s Router     │  Parse S3 request, extract operation
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Auth Layer     │  Verify AWS Signature V4
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Policy Check   │  Evaluate IAM + bucket policies
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Handler        │  Execute operation (get/put/list/etc)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Storage Layer  │  Interact with filesystem
└────────┬────────┘
         │
         ▼
HTTP Response (XML)
```

### strix-storage

Filesystem-based storage backend.

**Components:**
- `LocalStorage` - Implements `ObjectStore` trait
- Bucket management (create, delete, list)
- Object operations (get, put, delete, copy)
- Multipart upload handling

**Directory Layout:**
```
/var/lib/strix/
├── .strix/
│   ├── iam.db              # SQLite for IAM data
│   └── config.json         # Runtime config
└── buckets/
    └── {bucket}/
        ├── .bucket.meta    # Bucket metadata (MessagePack)
        └── objects/
            └── {key-hash}/
                ├── xl.meta # Object metadata (MessagePack)
                └── part.1  # Object data
```

**Metadata Format (MessagePack):**
```rust
struct ObjectMeta {
    key: String,
    size: u64,
    etag: String,
    content_type: String,
    last_modified: DateTime<Utc>,
    user_metadata: HashMap<String, String>,
    // Multipart info if applicable
    parts: Option<Vec<PartInfo>>,
}
```

### strix-iam

Identity and Access Management.

**Components:**
- `IamStore` - SQLite-backed storage for IAM data
- User management (CRUD operations)
- Access key management
- Policy storage and evaluation

**Database Schema:**
```sql
CREATE TABLE users (
    username TEXT PRIMARY KEY,
    arn TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'active' or 'inactive'
    created_at TEXT NOT NULL
);

CREATE TABLE access_keys (
    access_key_id TEXT PRIMARY KEY,
    secret_access_key TEXT NOT NULL,
    username TEXT NOT NULL REFERENCES users(username),
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE user_policies (
    username TEXT NOT NULL REFERENCES users(username),
    policy_name TEXT NOT NULL,
    policy_document TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (username, policy_name)
);

CREATE TABLE bucket_policies (
    bucket TEXT PRIMARY KEY,
    policy_document TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

**Policy Evaluation:**
```rust
// Simplified evaluation logic
fn is_authorized(user: &str, action: &str, resource: &str) -> bool {
    // 1. Root user bypasses all checks
    if is_root_user(user) {
        return true;
    }

    // 2. Check for explicit deny in IAM policies
    if iam_policies_deny(user, action, resource) {
        return false;
    }

    // 3. Check for explicit deny in bucket policy
    if bucket_policy_denies(resource, user, action) {
        return false;
    }

    // 4. Check for explicit allow in IAM policies
    if iam_policies_allow(user, action, resource) {
        return true;
    }

    // 5. Check for explicit allow in bucket policy
    if bucket_policy_allows(resource, user, action) {
        return true;
    }

    // 6. Default deny
    false
}
```

### strix-crypto

Cryptographic operations.

**Components:**
- AWS Signature V4 verification
- SHA256 hashing (for content verification)
- HMAC-SHA256 (for signature calculation)
- Pre-signed URL generation

**Signature V4 Flow:**
```
1. Extract auth headers from request
2. Parse credential scope (date/region/service/aws4_request)
3. Create canonical request string
4. Create string to sign
5. Calculate signing key: HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), "s3"), "aws4_request")
6. Calculate signature: HMAC(signing_key, string_to_sign)
7. Compare with provided signature
```

### strix-admin

Admin REST API for server management.

**Endpoints:**
- `/api/v1/info` - Server information
- `/api/v1/health` - Health check
- `/api/v1/users/*` - User management
- `/api/v1/buckets/*` - Bucket management
- `/api/v1/presign` - Pre-signed URL generation

**Authentication:**
Currently uses session-based auth (cookie) for web console.
API requests can use the root credentials.

### strix-gui

Leptos-based web console compiled to WebAssembly.

**Pages:**
- Login - Authentication
- Dashboard - Overview and stats
- Buckets - Bucket listing and management
- Object Browser - File browser with upload/download
- Users - User management
- Settings - Server configuration

**Build Process:**
```bash
# Built with Trunk
cd crates/strix-gui
trunk build --release

# Output embedded in binary via rust-embed
# Served from /console/* routes
```

**State Management:**
- Uses Leptos signals for reactive state
- API client talks to Admin API
- LocalStorage for session persistence

### strix-cli (sx)

Command-line interface for Strix.

**Architecture:**
- Uses `clap` for argument parsing
- AWS SDK for Rust for S3 operations
- Direct HTTP for admin operations

**Configuration:**
```json
// ~/.config/sx/config.json
{
  "aliases": {
    "local": {
      "endpoint": "http://localhost:9000",
      "access_key": "admin",
      "secret_key": "password"
    }
  }
}
```

## Async Runtime

Strix uses Tokio as its async runtime:

```rust
#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::init();

    // Start servers concurrently
    tokio::select! {
        _ = start_s3_server(config) => {},
        _ = start_admin_server(config) => {},
        _ = start_metrics_server(config) => {},
    }
}
```

## Error Handling

Errors flow through a unified error type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum StrixError {
    #[error("Bucket not found: {0}")]
    BucketNotFound(String),

    #[error("Object not found: {bucket}/{key}")]
    ObjectNotFound { bucket: String, key: String },

    #[error("Access denied")]
    AccessDenied,

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("IAM error: {0}")]
    Iam(#[from] IamError),
}

// Converted to S3 error codes for S3 API
impl From<StrixError> for s3s::S3Error {
    fn from(err: StrixError) -> Self {
        match err {
            StrixError::BucketNotFound(b) => S3Error::NoSuchBucket(b),
            StrixError::ObjectNotFound { .. } => S3Error::NoSuchKey,
            StrixError::AccessDenied => S3Error::AccessDenied,
            _ => S3Error::InternalError,
        }
    }
}
```

## Concurrency

Strix handles concurrent access through:

1. **Tokio tasks** - Each request runs in its own task
2. **Arc<RwLock>** - Shared state protected by async locks
3. **SQLite WAL mode** - Concurrent reads with serialized writes
4. **Atomic file operations** - Write to temp, then rename

```rust
// Example: Concurrent-safe storage operation
pub async fn put_object(&self, bucket: &str, key: &str, data: ObjectData) -> Result<()> {
    // 1. Write to temporary file
    let temp_path = self.temp_path(bucket, key);
    write_to_file(&temp_path, data).await?;

    // 2. Write metadata
    let meta_path = self.meta_path(bucket, key);
    write_metadata(&meta_path, metadata).await?;

    // 3. Atomic rename to final location
    let final_path = self.object_path(bucket, key);
    tokio::fs::rename(&temp_path, &final_path).await?;

    Ok(())
}
```

## Future Architecture (Distributed Mode)

Planned architecture for distributed deployments:

```
                    ┌─────────────────┐
                    │   Load Balancer │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│   Strix Node  │   │   Strix Node  │   │   Strix Node  │
│      (1)      │   │      (2)      │   │      (3)      │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            │
                    ┌───────▼───────┐
                    │  Distributed  │
                    │   Storage     │
                    │ (Erasure Code)│
                    └───────────────┘
```

**Planned Features:**
- Raft consensus for metadata
- Erasure coding for data redundancy
- Consistent hashing for object placement
- Automatic rebalancing
- Node failure recovery
