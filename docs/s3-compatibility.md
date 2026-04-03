# S3 Compatibility Matrix

This document tracks Strix's compatibility with the Amazon S3 API.

Status values in this matrix are evidence-based:
- "verified" means covered by repository integration tests and/or documented manual smoke tests.
- "planned" or "partial" means implementation exists but broader client validation is still in progress.

## Legend

- :white_check_mark: Fully implemented
- :construction: Partially implemented
- :x: Not implemented
- :no_entry: Not planned

## Bucket Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| CreateBucket | :white_check_mark: | |
| DeleteBucket | :white_check_mark: | |
| HeadBucket | :white_check_mark: | |
| ListBuckets | :white_check_mark: | |
| GetBucketLocation | :white_check_mark: | |
| GetBucketVersioning | :white_check_mark: | |
| PutBucketVersioning | :white_check_mark: | |
| ListObjectVersions | :white_check_mark: | |
| GetBucketPolicy | :white_check_mark: | |
| PutBucketPolicy | :white_check_mark: | |
| DeleteBucketPolicy | :white_check_mark: | |
| GetBucketAcl | :no_entry: | Use bucket policies instead |
| PutBucketAcl | :no_entry: | Use bucket policies instead |
| GetBucketCors | :white_check_mark: | |
| PutBucketCors | :white_check_mark: | |
| DeleteBucketCors | :white_check_mark: | |
| GetBucketLifecycle | :white_check_mark: | |
| PutBucketLifecycle | :white_check_mark: | |
| DeleteBucketLifecycle | :white_check_mark: | |
| GetBucketNotification | :white_check_mark: | Via Admin API |
| PutBucketNotification | :white_check_mark: | Via Admin API |
| GetBucketLogging | :x: | Planned |
| PutBucketLogging | :x: | Planned |
| GetBucketTagging | :white_check_mark: | |
| PutBucketTagging | :white_check_mark: | |
| DeleteBucketTagging | :white_check_mark: | |
| GetBucketReplication | :x: | Planned for distributed mode |
| PutBucketReplication | :x: | Planned for distributed mode |
| DeleteBucketReplication | :x: | Planned for distributed mode |
| GetBucketEncryption | :white_check_mark: | SSE-S3, SSE-C |
| PutBucketEncryption | :white_check_mark: | SSE-S3, SSE-C |
| DeleteBucketEncryption | :white_check_mark: | |
| GetObjectLockConfiguration | :white_check_mark: | WORM compliance |
| PutObjectLockConfiguration | :white_check_mark: | WORM compliance |

## Object Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| PutObject | :white_check_mark: | |
| GetObject | :white_check_mark: | Includes Range requests |
| HeadObject | :white_check_mark: | |
| DeleteObject | :white_check_mark: | |
| DeleteObjects | :white_check_mark: | Batch delete |
| CopyObject | :white_check_mark: | |
| ListObjects | :white_check_mark: | V1 API |
| ListObjectsV2 | :white_check_mark: | |
| GetObjectAcl | :no_entry: | Use policies instead |
| PutObjectAcl | :no_entry: | Use policies instead |
| GetObjectTagging | :white_check_mark: | Implemented via object metadata-backed tags; integration coverage in `tests/integration/src/s3_conformance.rs` |
| PutObjectTagging | :white_check_mark: | Implemented via metadata rewrite copy path; integration coverage in `tests/integration/src/s3_conformance.rs` |
| DeleteObjectTagging | :white_check_mark: | Implemented via metadata rewrite copy path; integration coverage in `tests/integration/src/s3_conformance.rs` |
| GetObjectRetention | :white_check_mark: | Object Lock |
| PutObjectRetention | :white_check_mark: | Object Lock |
| GetObjectLegalHold | :white_check_mark: | Object Lock |
| PutObjectLegalHold | :white_check_mark: | Object Lock |
| RestoreObject | :no_entry: | No Glacier-style storage |
| SelectObjectContent | :x: | May be planned |

## Multipart Upload Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| CreateMultipartUpload | :white_check_mark: | |
| UploadPart | :white_check_mark: | |
| CompleteMultipartUpload | :white_check_mark: | |
| AbortMultipartUpload | :white_check_mark: | |
| ListParts | :white_check_mark: | |
| ListMultipartUploads | :white_check_mark: | |
| UploadPartCopy | :white_check_mark: | Implemented in S3 service; covered by integration test (`tests/integration/src/multipart.rs`) |

## Pre-signed URLs

| Feature | Status | Notes |
|---------|--------|-------|
| Pre-signed GET | :white_check_mark: | |
| Pre-signed PUT | :white_check_mark: | |
| Pre-signed DELETE | :white_check_mark: | |
| Query string auth | :white_check_mark: | X-Amz-* query params |

## Authentication

| Method | Status | Notes |
|--------|--------|-------|
| AWS Signature V4 | :white_check_mark: | |
| AWS Signature V2 | :no_entry: | Deprecated, not planned |
| STS AssumeRole | :white_check_mark: | Via Admin API |
| IAM Roles | :x: | Planned |
| Anonymous access | :white_check_mark: | Via bucket policies |
| OIDC/SSO | :white_check_mark: | Azure AD, Google, custom |

## Server-Side Encryption

| Feature | Status | Notes |
|---------|--------|-------|
| SSE-S3 | :white_check_mark: | AES-256-GCM |
| SSE-C | :white_check_mark: | Customer-provided keys |
| SSE-KMS | :x: | Planned |

## Headers

### Request Headers

| Header | Status | Notes |
|--------|--------|-------|
| Authorization | :white_check_mark: | AWS4-HMAC-SHA256 |
| X-Amz-Date | :white_check_mark: | |
| X-Amz-Content-Sha256 | :white_check_mark: | |
| X-Amz-Security-Token | :white_check_mark: | STS temporary credentials |
| X-Amz-Copy-Source | :white_check_mark: | |
| X-Amz-Copy-Source-Range | :white_check_mark: | Supported for `UploadPartCopy` |
| X-Amz-Metadata-Directive | :white_check_mark: | `COPY` and `REPLACE` supported |
| X-Amz-Server-Side-Encryption | :white_check_mark: | |
| X-Amz-Server-Side-Encryption-Customer-Algorithm | :white_check_mark: | SSE-C |
| X-Amz-Server-Side-Encryption-Customer-Key | :white_check_mark: | SSE-C |
| X-Amz-Server-Side-Encryption-Customer-Key-MD5 | :white_check_mark: | SSE-C |
| Range | :white_check_mark: | |
| If-Match | :white_check_mark: | |
| If-None-Match | :white_check_mark: | |
| If-Modified-Since | :white_check_mark: | |
| If-Unmodified-Since | :white_check_mark: | |
| Content-Type | :white_check_mark: | |
| Content-Length | :white_check_mark: | |
| Content-MD5 | :white_check_mark: | |
| Content-Encoding | :white_check_mark: | |
| Content-Disposition | :white_check_mark: | |
| Cache-Control | :white_check_mark: | |
| Expires | :white_check_mark: | |
| X-Amz-Meta-* | :white_check_mark: | Custom metadata |
| X-Amz-Object-Lock-Mode | :white_check_mark: | |
| X-Amz-Object-Lock-Retain-Until-Date | :white_check_mark: | |
| X-Amz-Object-Lock-Legal-Hold | :white_check_mark: | |

### Response Headers

| Header | Status | Notes |
|--------|--------|-------|
| ETag | :white_check_mark: | |
| Content-Length | :white_check_mark: | |
| Content-Type | :white_check_mark: | |
| Last-Modified | :white_check_mark: | |
| Content-Range | :white_check_mark: | For Range requests |
| X-Amz-Request-Id | :white_check_mark: | |
| X-Amz-Id-2 | :white_check_mark: | |
| X-Amz-Version-Id | :white_check_mark: | |
| X-Amz-Delete-Marker | :white_check_mark: | |
| X-Amz-Meta-* | :white_check_mark: | |
| X-Amz-Server-Side-Encryption | :white_check_mark: | |

## Error Codes

| Error | Status | Notes |
|-------|--------|-------|
| AccessDenied | :white_check_mark: | |
| BucketAlreadyExists | :white_check_mark: | |
| BucketNotEmpty | :white_check_mark: | |
| EntityTooLarge | :white_check_mark: | |
| EntityTooSmall | :white_check_mark: | |
| InvalidAccessKeyId | :white_check_mark: | |
| InvalidArgument | :white_check_mark: | |
| InvalidBucketName | :white_check_mark: | |
| InvalidDigest | :white_check_mark: | |
| InvalidRange | :white_check_mark: | |
| InvalidRequest | :white_check_mark: | |
| InvalidSignature | :white_check_mark: | |
| KeyTooLong | :white_check_mark: | |
| MalformedXML | :white_check_mark: | |
| NoSuchBucket | :white_check_mark: | |
| NoSuchKey | :white_check_mark: | |
| NoSuchUpload | :white_check_mark: | |
| NoSuchVersion | :white_check_mark: | |
| NotImplemented | :white_check_mark: | |
| ObjectLocked | :white_check_mark: | |
| PreconditionFailed | :white_check_mark: | |
| SignatureDoesNotMatch | :white_check_mark: | |
| SlowDown | :white_check_mark: | Rate limiting |

## SDK and Tool Compatibility

Strix compatibility validation status for SDKs and tools:

### Official AWS SDKs

| SDK | Status | Notes |
|-----|--------|-------|
| AWS CLI | :white_check_mark: | Verified via integration and smoke flows |
| AWS SDK for Python (boto3) | :white_check_mark: | Verified via smoke test script |
| AWS SDK for JavaScript | :construction: | Expected to work; dedicated verification matrix in progress |
| AWS SDK for Go | :construction: | Expected to work; dedicated verification matrix in progress |
| AWS SDK for Java | :construction: | Expected to work; dedicated verification matrix in progress |
| AWS SDK for .NET | :construction: | Expected to work; dedicated verification matrix in progress |
| AWS SDK for Rust | :construction: | Expected to work; dedicated verification matrix in progress |

### Third-Party Tools

| Tool | Status | Notes |
|------|--------|-------|
| restic | :white_check_mark: | `init/backup/snapshots/restore/forget --prune` validated after ranged GET compatibility fix |
| rclone | :white_check_mark: | Smoke validated with `use_unsigned_payload = true` rclone profile |
| s3cmd | :white_check_mark: | Smoke validated (`mb/put/ls/get/del/rb`) with explicit endpoint/path-style configuration |
| s3fs-fuse | :construction: | Basic operations expected; not fully validated |
| Cyberduck | :construction: | Expected to work; dedicated verification matrix in progress |
| MinIO Client (mc) | :construction: | S3 operations expected; dedicated verification matrix in progress |
| goofys | :construction: | Basic operations expected; not fully validated |

## Path Style vs Virtual-Hosted Style

| Style | Status | Notes |
|-------|--------|-------|
| Path style | :white_check_mark: | `http://s3.example.com/bucket/key` |
| Virtual-hosted style | :construction: | Requires DNS/proxy config |

**Note:** Path style is the default and recommended for self-hosted deployments.

## Limitations

### Current Limitations

1. **Single node only** - Distributed mode not yet implemented
2. **No SSE-KMS** - AWS KMS integration planned

### By Design

1. **No ACLs** - Use IAM and bucket policies instead
2. **No Glacier** - No cold storage tier
3. **No S3 Signature V2** - Deprecated, security risk

## Feature Requests

Have a feature request? Open an issue on GitHub with the `feature-request` label.
