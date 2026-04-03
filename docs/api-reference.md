# API Reference

Strix exposes two APIs:
1. **S3 API** (port 9000) - Amazon S3-compatible API for object storage operations
2. **Admin API** (port 9001) - REST API for server administration

## S3 API

The S3 API is compatible with AWS SDKs and S3 clients. All requests must be signed using AWS Signature Version 4.

### Authentication

All S3 API requests must include AWS Signature V4 authentication headers:

```
Authorization: AWS4-HMAC-SHA256 Credential=.../s3/aws4_request, SignedHeaders=..., Signature=...
X-Amz-Date: 20240101T000000Z
X-Amz-Content-Sha256: <payload-hash>
```

### Bucket Operations

#### Create Bucket
```http
PUT /{bucket} HTTP/1.1
Host: localhost:9000
```

**Response:** `200 OK`

#### Delete Bucket
```http
DELETE /{bucket} HTTP/1.1
Host: localhost:9000
```

**Response:** `204 No Content`

**Error:** `409 Conflict` if bucket is not empty

#### List Buckets
```http
GET / HTTP/1.1
Host: localhost:9000
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult>
  <Owner>
    <ID>owner-id</ID>
    <DisplayName>owner</DisplayName>
  </Owner>
  <Buckets>
    <Bucket>
      <Name>my-bucket</Name>
      <CreationDate>2024-01-01T00:00:00.000Z</CreationDate>
    </Bucket>
  </Buckets>
</ListAllMyBucketsResult>
```

#### Head Bucket
```http
HEAD /{bucket} HTTP/1.1
Host: localhost:9000
```

**Response:** `200 OK` if bucket exists, `404 Not Found` otherwise

### Object Operations

#### Put Object
```http
PUT /{bucket}/{key} HTTP/1.1
Host: localhost:9000
Content-Length: <size>
Content-Type: <mime-type>

<binary-data>
```

**Response Headers:**
- `ETag: "<md5-hash>"`

#### Get Object
```http
GET /{bucket}/{key} HTTP/1.1
Host: localhost:9000
```

**Optional Headers:**
- `Range: bytes=0-1023` - Request partial content

**Response Headers:**
- `Content-Length: <size>`
- `Content-Type: <mime-type>`
- `ETag: "<md5-hash>"`
- `Last-Modified: <date>`

#### Delete Object
```http
DELETE /{bucket}/{key} HTTP/1.1
Host: localhost:9000
```

**Response:** `204 No Content`

#### Head Object
```http
HEAD /{bucket}/{key} HTTP/1.1
Host: localhost:9000
```

**Response Headers:**
- `Content-Length: <size>`
- `Content-Type: <mime-type>`
- `ETag: "<md5-hash>"`
- `Last-Modified: <date>`

#### List Objects (V2)
```http
GET /{bucket}?list-type=2 HTTP/1.1
Host: localhost:9000
```

**Query Parameters:**
- `prefix` - Filter by prefix
- `delimiter` - Group by delimiter (typically `/`)
- `max-keys` - Maximum number of keys (default: 1000)
- `continuation-token` - Pagination token

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
  <Name>my-bucket</Name>
  <Prefix></Prefix>
  <MaxKeys>1000</MaxKeys>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>file.txt</Key>
    <LastModified>2024-01-01T00:00:00.000Z</LastModified>
    <ETag>"abc123"</ETag>
    <Size>1024</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>
  <CommonPrefixes>
    <Prefix>folder/</Prefix>
  </CommonPrefixes>
</ListBucketResult>
```

#### Copy Object
```http
PUT /{bucket}/{key} HTTP/1.1
Host: localhost:9000
X-Amz-Copy-Source: /source-bucket/source-key
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CopyObjectResult>
  <LastModified>2024-01-01T00:00:00.000Z</LastModified>
  <ETag>"abc123"</ETag>
</CopyObjectResult>
```

#### Delete Objects (Batch)
```http
POST /{bucket}?delete HTTP/1.1
Host: localhost:9000
Content-Type: application/xml

<?xml version="1.0" encoding="UTF-8"?>
<Delete>
  <Object>
    <Key>file1.txt</Key>
  </Object>
  <Object>
    <Key>file2.txt</Key>
  </Object>
</Delete>
```

### Multipart Upload Operations

#### Initiate Multipart Upload
```http
POST /{bucket}/{key}?uploads HTTP/1.1
Host: localhost:9000
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult>
  <Bucket>my-bucket</Bucket>
  <Key>large-file.zip</Key>
  <UploadId>abc123</UploadId>
</InitiateMultipartUploadResult>
```

#### Upload Part
```http
PUT /{bucket}/{key}?partNumber=1&uploadId=abc123 HTTP/1.1
Host: localhost:9000
Content-Length: <part-size>

<binary-data>
```

**Response Headers:**
- `ETag: "<part-etag>"`

#### Complete Multipart Upload
```http
POST /{bucket}/{key}?uploadId=abc123 HTTP/1.1
Host: localhost:9000

<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUpload>
  <Part>
    <PartNumber>1</PartNumber>
    <ETag>"part1-etag"</ETag>
  </Part>
  <Part>
    <PartNumber>2</PartNumber>
    <ETag>"part2-etag"</ETag>
  </Part>
</CompleteMultipartUpload>
```

#### Abort Multipart Upload
```http
DELETE /{bucket}/{key}?uploadId=abc123 HTTP/1.1
Host: localhost:9000
```

---

## Admin API

The Admin API is a REST API for managing the Strix server. All endpoints are under `/api/v1/`.

### Server Information

#### Get Server Info
```http
GET /api/v1/info HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "version": "0.1.0",
  "commit": "abc123",
  "mode": "standalone",
  "uptime": 3600,
  "region": "us-east-1"
}
```

#### Health Check
```http
GET /api/v1/health HTTP/1.1
Host: localhost:9001
```

**Response:** `200 OK`

### User Management

#### List Users
```http
GET /api/v1/users HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "users": [
    {
      "username": "alice",
      "arn": "arn:aws:iam:::user/alice",
      "created_at": "2024-01-01T00:00:00Z",
      "status": "active",
      "policies": ["ReadWriteAccess"]
    }
  ]
}
```

#### Create User
```http
POST /api/v1/users HTTP/1.1
Host: localhost:9001
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
    "arn": "arn:aws:iam:::user/alice",
    "created_at": "2024-01-01T00:00:00Z",
    "status": "active"
  },
  "access_key": {
    "access_key_id": "AKIAIOSFODNN7EXAMPLE",
    "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    "username": "alice",
    "created_at": "2024-01-01T00:00:00Z",
    "status": "active"
  }
}
```

#### Get User
```http
GET /api/v1/users/{username} HTTP/1.1
Host: localhost:9001
```

#### Delete User
```http
DELETE /api/v1/users/{username} HTTP/1.1
Host: localhost:9001
```

**Response:** `204 No Content`

### Access Key Management

#### List Access Keys
```http
GET /api/v1/users/{username}/access-keys HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "access_keys": [
    {
      "access_key_id": "AKIAIOSFODNN7EXAMPLE",
      "username": "alice",
      "created_at": "2024-01-01T00:00:00Z",
      "status": "active"
    }
  ]
}
```

#### Create Access Key
```http
POST /api/v1/users/{username}/access-keys HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "username": "alice",
  "created_at": "2024-01-01T00:00:00Z",
  "status": "active"
}
```

#### Delete Access Key
```http
DELETE /api/v1/access-keys/{access_key_id} HTTP/1.1
Host: localhost:9001
```

### Policy Management

#### List User Policies
```http
GET /api/v1/users/{username}/policies HTTP/1.1
Host: localhost:9001
```

#### Attach User Policy
```http
POST /api/v1/users/{username}/policies HTTP/1.1
Host: localhost:9001
Content-Type: application/json

{
  "policy": {
    "name": "MyPolicy",
    "Statement": [
      {
        "Effect": "Allow",
        "Action": ["s3:GetObject", "s3:PutObject"],
        "Resource": ["arn:aws:s3:::my-bucket/*"]
      }
    ]
  }
}
```

#### Detach User Policy
```http
DELETE /api/v1/users/{username}/policies/{policy_name} HTTP/1.1
Host: localhost:9001
```

### Bucket Management

#### List Buckets
```http
GET /api/v1/buckets HTTP/1.1
Host: localhost:9001
```

#### Create Bucket
```http
POST /api/v1/buckets HTTP/1.1
Host: localhost:9001
Content-Type: application/json

{
  "name": "my-bucket"
}
```

#### Delete Bucket
```http
DELETE /api/v1/buckets/{name} HTTP/1.1
Host: localhost:9001
```

### Bucket Policies

#### Get Bucket Policy
```http
GET /api/v1/buckets/{bucket}/policy HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "policy": {
    "Version": "2012-10-17",
    "Statement": [
      {
        "Sid": "PublicRead",
        "Effect": "Allow",
        "Principal": "*",
        "Action": ["s3:GetObject"],
        "Resource": ["arn:aws:s3:::my-bucket/*"]
      }
    ]
  }
}
```

#### Set Bucket Policy
```http
PUT /api/v1/buckets/{bucket}/policy HTTP/1.1
Host: localhost:9001
Content-Type: application/json

{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicRead",
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject"],
      "Resource": ["arn:aws:s3:::my-bucket/*"]
    }
  ]
}
```

#### Delete Bucket Policy
```http
DELETE /api/v1/buckets/{bucket}/policy HTTP/1.1
Host: localhost:9001
```

### Object Management

#### List Objects
```http
GET /api/v1/buckets/{bucket}/objects?prefix=folder/&delimiter=/ HTTP/1.1
Host: localhost:9001
```

#### Delete Object
```http
DELETE /api/v1/buckets/{bucket}/objects/{key} HTTP/1.1
Host: localhost:9001
```

#### Delete Multiple Objects
```http
DELETE /api/v1/buckets/{bucket}/objects HTTP/1.1
Host: localhost:9001
Content-Type: application/json

{
  "keys": ["file1.txt", "file2.txt"]
}
```

### Pre-signed URLs

#### Generate Pre-signed URL
```http
POST /api/v1/presign HTTP/1.1
Host: localhost:9001
Content-Type: application/json

{
  "bucket": "my-bucket",
  "key": "file.txt",
  "method": "GET",
  "expires_in": 3600
}
```

**Response:**
```json
{
  "url": "http://localhost:9000/my-bucket/file.txt?X-Amz-Algorithm=...",
  "expires_in": 3600,
  "method": "GET"
}
```

### Storage Usage

#### Get Storage Usage
```http
GET /api/v1/usage HTTP/1.1
Host: localhost:9001
```

**Response:**
```json
{
  "buckets": [
    {
      "name": "my-bucket",
      "created_at": "2024-01-01T00:00:00Z",
      "object_count": 100,
      "total_size": 1073741824
    }
  ],
  "total_buckets": 1,
  "total_objects": 100,
  "total_size": 1073741824
}
```

---

## Error Responses

### S3 API Errors

S3 API errors are returned as XML:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Error>
  <Code>NoSuchBucket</Code>
  <Message>The specified bucket does not exist</Message>
  <Resource>/my-bucket</Resource>
  <RequestId>abc123</RequestId>
</Error>
```

Common error codes:
- `NoSuchBucket` - Bucket does not exist
- `NoSuchKey` - Object does not exist
- `BucketAlreadyExists` - Bucket name is taken
- `BucketNotEmpty` - Cannot delete non-empty bucket
- `AccessDenied` - Permission denied
- `InvalidArgument` - Invalid request parameter
- `SignatureDoesNotMatch` - Invalid signature

### Admin API Errors

Admin API errors are returned as JSON:

```json
{
  "error": "User not found",
  "message": "The user 'alice' does not exist"
}
```

HTTP status codes:
- `400 Bad Request` - Invalid request
- `401 Unauthorized` - Authentication required
- `403 Forbidden` - Permission denied
- `404 Not Found` - Resource not found
- `409 Conflict` - Resource conflict (e.g., bucket not empty)
- `500 Internal Server Error` - Server error
