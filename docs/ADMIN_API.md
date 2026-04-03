# Strix Admin API

The Strix Admin API provides RESTful endpoints for server administration, user management, and access key operations.

## Base URL

```
http://localhost:9001/api/v1
```

The admin API runs on a separate port from the S3 API (default: 9001).

## Authentication

Most endpoints require JWT authentication. Obtain a token by calling the login endpoint.

### Login

```http
POST /api/v1/login
Content-Type: application/json

{
  "access_key_id": "your-access-key",
  "secret_access_key": "your-secret-key"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_in": 86400
}
```

Use the token in subsequent requests:
```http
Authorization: Bearer <token>
```

### Request IDs and Audit Logging

- Every authenticated admin API request generates an audit event.
- Responses include `X-Request-Id` for request correlation.
- Request ID selection order:
  1. `X-Request-Id` request header
  2. `X-Amz-Request-Id` request header
  3. Generated UUID
- Source IP selection order:
  1. `X-Forwarded-For` (first hop)
  2. `X-Real-IP`
  3. Direct peer socket (when available)

Audit entries persist `request_id`, `source_ip`, status code, and duration for incident tracing and compliance workflows.

### Authorization (RBAC)

- Authentication and authorization are separate:
  - JWT authentication verifies identity.
  - IAM policy authorization determines admin route access.
- Root credentials bypass IAM checks for bootstrap/recovery operations.
- Non-root users are denied admin API access by default unless IAM policies grant required actions.
- Current policy mapping is conservative:
  - bucket/object admin routes are resource-aware and map to specific S3 actions (for example `s3:CreateBucket`, `s3:DeleteBucket`, `s3:GetBucketVersioning`, `s3:PutBucketVersioning`, `s3:ListBucket`, `s3:DeleteObject`),
  - user/group/policy and other control-plane routes still require broad admin rights (`s3:*`).

### Rate Limiting

Login attempts are rate-limited to prevent brute-force attacks:
- **5 attempts** per minute per IP address
- **15 minute lockout** after exceeding the limit

## Public Endpoints

These endpoints do not require authentication:

### Health Check

```http
GET /api/v1/health
```

**Response:**
```json
{
  "status": "healthy",
  "storage": "ok",
  "database": "ok"
}
```

### Server Info

```http
GET /api/v1/info
```

**Response:**
```json
{
  "version": "0.1.0",
  "commit": "abc1234",
  "mode": "standalone",
  "uptime": 3600,
  "region": "us-east-1"
}
```

## User Management

### List Users

```http
GET /api/v1/users
Authorization: Bearer <token>
```

**Query Parameters:**
- `limit` (optional): Maximum number of users (default: 100, max: 1000)
- `offset` (optional): Number of users to skip

**Response:**
```json
{
  "users": [
    {
      "username": "alice",
      "arn": "arn:aws:iam::000000000000:user/alice",
      "created_at": "2025-01-15T10:30:00Z",
      "status": "Active",
      "policies": ["ReadOnly"]
    }
  ],
  "total": 1,
  "count": 1,
  "offset": 0,
  "has_more": false
}
```

### Create User

```http
POST /api/v1/users
Authorization: Bearer <token>
Content-Type: application/json

{
  "username": "alice"
}
```

**Response:**
```json
{
  "user": {
    "username": "alice",
    "arn": "arn:aws:iam::000000000000:user/alice",
    "created_at": "2025-01-15T10:30:00Z",
    "status": "Active",
    "policies": []
  },
  "access_key": null
}
```

### Get User

```http
GET /api/v1/users/{username}
Authorization: Bearer <token>
```

**Response:**
```json
{
  "username": "alice",
  "arn": "arn:aws:iam::000000000000:user/alice",
  "created_at": "2025-01-15T10:30:00Z",
  "status": "Active",
  "policies": ["ReadOnly"]
}
```

### Delete User

```http
DELETE /api/v1/users/{username}
Authorization: Bearer <token>
```

**Response:** `204 No Content`

## Access Key Management

### List Access Keys

```http
GET /api/v1/users/{username}/keys
Authorization: Bearer <token>
```

**Response:**
```json
{
  "access_keys": [
    {
      "access_key_id": "AKIAIOSFODNN7EXAMPLE",
      "username": "alice",
      "created_at": "2025-01-15T10:30:00Z",
      "status": "Active"
    }
  ]
}
```

### Create Access Key

```http
POST /api/v1/users/{username}/keys
Authorization: Bearer <token>
```

**Response:**
```json
{
  "access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "username": "alice",
  "created_at": "2025-01-15T10:30:00Z",
  "status": "Active"
}
```

**Note:** The `secret_access_key` is only returned once at creation time. Store it securely.

### Update Access Key Status

```http
PUT /api/v1/users/{username}/keys/{access_key_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "status": "Inactive"
}
```

**Response:** `204 No Content`

### Delete Access Key

```http
DELETE /api/v1/users/{username}/keys/{access_key_id}
Authorization: Bearer <token>
```

**Response:** `204 No Content`

## Group Management

### List Groups

```http
GET /api/v1/groups
Authorization: Bearer <token>
```

**Response:**
```json
{
  "groups": [
    {
      "name": "developers",
      "arn": "arn:aws:iam::000000000000:group/developers",
      "created_at": "2025-01-15T10:30:00Z",
      "members": ["alice", "bob"],
      "policies": ["ReadWrite"]
    }
  ]
}
```

### Create Group

```http
POST /api/v1/groups
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "developers"
}
```

### Add User to Group

```http
POST /api/v1/groups/{group_name}/members
Authorization: Bearer <token>
Content-Type: application/json

{
  "username": "alice"
}
```

### Remove User from Group

```http
DELETE /api/v1/groups/{group_name}/members/{username}
Authorization: Bearer <token>
```

## Policy Management

### List Policies

```http
GET /api/v1/policies
Authorization: Bearer <token>
```

**Response:**
```json
{
  "policies": [
    {
      "name": "ReadOnly",
      "version": "2012-10-17",
      "statements": [
        {
          "effect": "Allow",
          "action": ["s3:GetObject", "s3:ListBucket"],
          "resource": "*"
        }
      ],
      "description": "Read-only access to all buckets"
    }
  ]
}
```

### Create Policy

```http
POST /api/v1/policies
Authorization: Bearer <token>
Content-Type: application/json

{
  "policy": {
    "name": "MyPolicy",
    "version": "2012-10-17",
    "statements": [
      {
        "effect": "Allow",
        "action": ["s3:*"],
        "resource": ["arn:aws:s3:::my-bucket/*"]
      }
    ]
  },
  "description": "Full access to my-bucket"
}
```

### Attach Policy to User

```http
POST /api/v1/users/{username}/policies
Authorization: Bearer <token>
Content-Type: application/json

{
  "policy": {
    "name": "ReadOnly",
    "version": "2012-10-17",
    "statements": [
      {
        "effect": "Allow",
        "action": ["s3:GetObject"],
        "resource": "*"
      }
    ]
  }
}
```

### Detach Policy from User

```http
DELETE /api/v1/users/{username}/policies/{policy_name}
Authorization: Bearer <token>
```

## Bucket Management

### List Buckets

```http
GET /api/v1/buckets
Authorization: Bearer <token>
```

**Response:**
```json
{
  "buckets": [
    {
      "name": "my-bucket",
      "created_at": "2025-01-15T10:30:00Z"
    }
  ]
}
```

### Create Bucket

```http
POST /api/v1/buckets
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "my-bucket"
}
```

### Delete Bucket

```http
DELETE /api/v1/buckets/{bucket_name}
Authorization: Bearer <token>
```

### List Objects

```http
GET /api/v1/buckets/{bucket_name}/objects
Authorization: Bearer <token>
```

**Query Parameters:**
- `prefix` (optional): Filter by prefix
- `delimiter` (optional): Delimiter for hierarchy
- `max_keys` (optional): Maximum number of objects
- `continuation_token` (optional): Pagination token

### Get Bucket Policy

```http
GET /api/v1/buckets/{bucket_name}/policy
Authorization: Bearer <token>
```

### Set Bucket Policy

```http
PUT /api/v1/buckets/{bucket_name}/policy
Authorization: Bearer <token>
Content-Type: application/json

{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::my-bucket/*"
    }
  ]
}
```

## Pre-signed URLs

### Generate Pre-signed URL

```http
POST /api/v1/presign
Authorization: Bearer <token>
Content-Type: application/json

{
  "bucket": "my-bucket",
  "key": "my-object.txt",
  "method": "GET",
  "expires_in": 3600
}
```

**Response:**
```json
{
  "url": "http://localhost:9000/my-bucket/my-object.txt?X-Amz-Algorithm=...",
  "expires_in": 3600,
  "method": "GET"
}
```

**Supported Methods:**
- `GET`: Download object
- `PUT`: Upload object
- `DELETE`: Delete object

**Expiration:**
- Default: 3600 seconds (1 hour)
- Maximum: 604800 seconds (7 days)

## Storage Usage

### Get Usage Statistics

```http
GET /api/v1/usage
Authorization: Bearer <token>
```

**Response:**
```json
{
  "buckets": [
    {
      "name": "my-bucket",
      "created_at": "2025-01-15T10:30:00Z",
      "object_count": 1234,
      "total_size": 5678901234
    }
  ],
  "total_buckets": 1,
  "total_objects": 1234,
  "total_size": 5678901234
}
```

## Audit Logging

### Query Audit Log

```http
GET /api/v1/audit
Authorization: Bearer <token>
```

**Query Parameters:**
- `bucket` (optional): Filter by bucket name
- `operation` (optional): Filter by operation (e.g., "GetObject")
- `principal` (optional): Filter by user/access key
- `status` (optional): Filter by status ("success" or "error")
- `start_time` (optional): ISO 8601 start time
- `end_time` (optional): ISO 8601 end time
- `limit` (optional): Maximum entries (default: 100)
- `offset` (optional): Pagination offset

**Response:**
```json
{
  "entries": [
    {
      "id": "01HQXYZ...",
      "timestamp": "2025-01-15T10:30:00Z",
      "operation": "GetObject",
      "bucket": "my-bucket",
      "key": "my-object.txt",
      "principal": "alice",
      "source_ip": "192.168.1.100",
      "status_code": 200,
      "error_code": null,
      "duration_ms": 15,
      "bytes_sent": 1024,
      "request_id": "abc123"
    }
  ],
  "total": 100,
  "limit": 100,
  "offset": 0
}
```

## Event Notifications

### List Notification Rules

```http
GET /api/v1/buckets/{bucket_name}/notifications
Authorization: Bearer <token>
```

**Response:**
```json
{
  "rules": [
    {
      "id": "rule-1",
      "events": ["s3:ObjectCreated:*"],
      "prefix": "uploads/",
      "suffix": ".jpg",
      "destination_type": "webhook",
      "destination_url": "https://example.com/webhook"
    }
  ]
}
```

### Create Notification Rule

```http
POST /api/v1/buckets/{bucket_name}/notifications
Authorization: Bearer <token>
Content-Type: application/json

{
  "id": "my-rule",
  "events": ["s3:ObjectCreated:*", "s3:ObjectRemoved:*"],
  "prefix": "uploads/",
  "suffix": ".jpg",
  "destination_type": "webhook",
  "destination_url": "https://example.com/webhook"
}
```

## STS (Security Token Service)

### Assume Role / Get Temporary Credentials

Get temporary credentials for a user. These credentials can be used with the S3 API and will expire after the specified duration.

```http
POST /api/v1/sts/assume-role
Authorization: Bearer <token>
Content-Type: application/json

{
  "username": "alice",
  "session_name": "s3-backup-session",
  "duration_seconds": 3600
}
```

**Parameters:**
- `username` (required): The user to generate temporary credentials for
- `session_name` (optional): A name for the session (for audit tracking)
- `duration_seconds` (optional): Duration in seconds, 900-43200 (default: 3600)

**Response:**
```json
{
  "access_key_id": "ASIA1234567890ABCDEF",
  "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "session_token": "FwoGZXIvYXdzEB0aDN...",
  "expiration": "2026-04-02T13:00:00Z"
}
```

**Using Temporary Credentials:**

Temporary credentials use the `ASIA` prefix (vs `AKIA` for permanent keys). When making S3 API requests, include:
- The temporary access key ID in the signature
- The `X-Amz-Security-Token` header with the session token value

Example with AWS CLI:
```bash
export AWS_ACCESS_KEY_ID=ASIA1234567890ABCDEF
export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
export AWS_SESSION_TOKEN=FwoGZXIvYXdzEB0aDN...
aws s3 ls s3://my-bucket/ --endpoint-url http://localhost:9000
```

## Error Responses

All error responses follow this format:

```json
{
  "code": "USER_NOT_FOUND",
  "error": "User not found: alice",
  "message": "The specified user does not exist",
  "request_id": "abc123"
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| USER_NOT_FOUND | 404 | User does not exist |
| USER_ALREADY_EXISTS | 409 | User already exists |
| GROUP_NOT_FOUND | 404 | Group does not exist |
| GROUP_ALREADY_EXISTS | 409 | Group already exists |
| POLICY_NOT_FOUND | 404 | Policy does not exist |
| ACCESS_KEY_NOT_FOUND | 404 | Access key does not exist |
| BUCKET_NOT_FOUND | 404 | Bucket does not exist |
| BUCKET_ALREADY_EXISTS | 409 | Bucket already exists |
| INVALID_REQUEST | 400 | Invalid request parameters |
| UNAUTHORIZED | 401 | Authentication required |
| FORBIDDEN | 403 | Permission denied |
| RATE_LIMIT_EXCEEDED | 429 | Too many requests |
| INTERNAL_ERROR | 500 | Internal server error |
