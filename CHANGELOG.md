# Changelog

All notable changes to this project are documented in this file.
## [0.0.0] - 2026-06-16

### Added

#### Single Sign-On (OIDC)
- End-to-end OIDC login via the OAuth2 Authorization Code flow with ID-token
  verification (JWKS, RS256), CSRF `state`/`nonce` protection, and optional
  auto-provisioning of users on first login.
- Database-backed OIDC provider management with root-only CRUD endpoints
  (`/admin/oidc/providers`). Client secrets are encrypted at rest and never
  returned by the API; provider changes hot-reload without a restart.
- `STRIX_OIDC_*` environment variables seed a provider on first boot; the
  console is the source of truth thereafter.
- Console UI for managing providers (Azure AD, Google, generic) with
  provider-type presets and a "Test discovery" check, plus "Sign in with SSO"
  buttons on the login page.

#### Email (SMTP relay)
- SMTP relay integration (e.g. SMTP2Go) for outbound mail, with the password
  encrypted at rest and write-only across the admin API.
- Three notification triggers: notification delivery-failure alerts,
  security/audit-event alerts (denied requests and privileged changes), and
  scheduled storage usage reports (daily or weekly).
- Root-only configuration endpoints (`/admin/smtp`) and a "Send test email"
  action, surfaced in the console Settings page.
- `STRIX_SMTP_*` environment variables seed the configuration on first boot.

### Changed

- Documentation reorganized under `docs/` with a single index, mermaid diagrams,
  and new guides for SSO/OIDC, email alerts, reverse proxying (nginx), and using
  Strix as an S3 backup target.

### Security

- Cleared all outstanding dependency advisories: bumped `indicatif`
  (drops unmaintained `number_prefix`) and switched the AWS SDK client to the
  modern `default-https-client` TLS stack (rustls 0.23 / webpki 0.103, newer
  `lru`). `cargo audit` and `cargo deny check` now pass with an empty ignore
  list. Accepted/resolved advisories are tracked under `docs/advisories/`.

## [0.0.0] - 2026-04-03

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
- Dependencies audited with `cargo audit`
- See SECURITY.md for current advisory status and accepted warnings

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
