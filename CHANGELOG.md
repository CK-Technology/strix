# Changelog

All notable changes to this project are documented in this file.

## [0.1.0] - 2026-04-03

Initial release of Strix, an S3-compatible object storage server.

### Features

#### S3 API Compatibility
- Full AWS Signature v4 authentication
- Bucket operations: Create, Delete, Head, List, Location
- Object operations: Put, Get, Head, Delete, Copy, List (v1 and v2)
- Multipart uploads: Create, Upload Part, Upload Part Copy, Complete, Abort, List Parts
- Pre-signed URLs for GET, PUT, DELETE operations
- Range requests with precise Content-Range headers
- Conditional requests (If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since)
- Custom metadata (X-Amz-Meta-*)

#### Versioning
- Bucket versioning (Enabled, Suspended)
- Version-aware operations (Get, Delete, List)
- Delete markers

#### Bucket Configuration
- Bucket policies with IAM-compatible policy language
- CORS configuration
- Lifecycle rules with expiration and transitions
- Bucket tagging (max 50 tags per bucket)
- Bucket encryption (SSE-S3, SSE-C)

#### Object Features
- Object tagging
- Server-side encryption (SSE-S3 with AES-256-GCM, SSE-C)
- Object Lock (WORM compliance with Governance and Compliance modes)
- Legal holds
- Retention policies

#### IAM and Access Control
- User management with access keys
- Group-based access control
- IAM policies with resource-level permissions
- Managed policies
- STS temporary credentials (AssumeRole via Admin API)
- OIDC/SSO integration (Azure AD, Google, custom providers)

#### Admin API
- RESTful administration endpoints on separate port
- JWT authentication with rate limiting
- User/Group/Policy management
- Bucket and object administration
- Storage usage statistics
- Audit logging with request correlation
- Pre-signed URL generation
- STS assume-role for temporary credentials

#### Multi-tenancy
- Tenant isolation
- Per-tenant storage quotas
- Tenant-scoped IAM

#### Event Notifications
- Webhook destinations
- Event filtering by prefix/suffix
- S3-compatible event format

#### Observability
- Prometheus metrics endpoint
- Structured logging with tracing
- Request ID correlation
- Audit trail with source IP tracking

### Security

#### STS Temporary Credentials
- Session token enforcement: `X-Amz-Security-Token` header required for ASIA-prefixed credentials
- Hash-only token storage: session tokens stored as SHA-256 hash, never in plaintext
- Proper identity mapping: temporary credentials resolve to assumed user identity for authorization

#### Dependency Audit
- All dependencies audited with `cargo audit` (0 vulnerabilities)
- 3 advisory warnings accepted for v0.1.0 (see SECURITY.md for details)

### SDK and Tool Compatibility

Verified compatible with:
- AWS CLI v2
- boto3 (AWS SDK for Python)
- restic backup
- rclone (with `use_unsigned_payload = true`)
- s3cmd

### Added
- Integration test `test_get_object_range_returns_exact_headers_and_body` for ranged GET header/body correctness
- Integration test `test_object_tagging_overwrite_and_empty_set` for object tagging semantics
- Integration test `test_create_existing_bucket_returns_already_owned_by_you` for duplicate bucket create error parity
- Integration test `test_bucket_tagging` for bucket tagging operations
- STS integration tests for session token validation (valid/missing/wrong token scenarios)
- Practical smoke workflows in `docs/tool-compatibility-testing.md` for AWS CLI, restic, rclone, s3cmd, and boto3
- boto3 smoke test script with comprehensive S3 operations

### Changed
- Improved S3 ranged GET compatibility with precise Content-Length/Content-Range for partial responses
- Implemented S3 object tagging operations (GetObjectTagging, PutObjectTagging, DeleteObjectTagging)
- Implemented bucket tagging operations (GetBucketTagging, PutBucketTagging, DeleteBucketTagging)
- Implemented UploadPartCopy for multipart copy operations
- Corrected duplicate bucket-create error mapping to BucketAlreadyOwnedByYou
- Updated compatibility matrix with verified tool status

### Fixed
- Resolved restic prune instability caused by ranged GET response mismatches
- Restored expected SSE-C multipart completion failure semantics
