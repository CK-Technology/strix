# IAM & Policies

Strix implements AWS-compatible Identity and Access Management (IAM) for controlling access to resources.

## Overview

Strix supports two types of access control:

1. **IAM Policies** - Attached to users, define what actions they can perform
2. **Bucket Policies** - Attached to buckets, define who can access the bucket

## IAM Users

### Creating Users

Users can be created via the Admin API or CLI:

```bash
# Using sx CLI
sx admin user add local alice

# Using Admin API
curl -X POST http://localhost:9001/api/v1/users \
  -H "Content-Type: application/json" \
  -d '{"username": "alice"}'
```

When a user is created, an access key is automatically generated.

### User Properties

- **username** - Unique identifier (1-64 characters, alphanumeric, underscore, hyphen)
- **arn** - Amazon Resource Name: `arn:aws:iam:::user/{username}`
- **status** - `active` or `inactive`
- **created_at** - Creation timestamp

## Access Keys

Each user can have multiple access keys for authentication:

```bash
# Create additional access key
sx admin key add local alice

# List access keys
sx admin key list local alice

# Disable access key
sx admin key disable local AKIAEXAMPLE

# Delete access key
sx admin key remove local AKIAEXAMPLE
```

### Access Key Properties

- **access_key_id** - 20-character identifier (AKIA...)
- **secret_access_key** - 40-character secret (only shown once at creation)
- **status** - `active` or `inactive`
- **created_at** - Creation timestamp

## IAM Policies

IAM policies define permissions using JSON documents following AWS IAM policy syntax.

### Policy Structure

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "OptionalStatementId",
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject"],
      "Resource": ["arn:aws:s3:::my-bucket/*"]
    }
  ]
}
```

### Policy Elements

#### Version

Always use `"2012-10-17"` for full feature support.

#### Statement

Array of permission statements. Each statement contains:

- **Sid** (optional) - Statement identifier for reference
- **Effect** - `Allow` or `Deny`
- **Action** - List of actions (supports wildcards)
- **Resource** - List of resource ARNs (supports wildcards)
- **Condition** (optional) - Conditions for when the statement applies

#### Actions

| Action | Description |
|--------|-------------|
| `s3:*` | All S3 actions |
| `s3:GetObject` | Download objects |
| `s3:PutObject` | Upload objects |
| `s3:DeleteObject` | Delete objects |
| `s3:ListBucket` | List objects in bucket |
| `s3:GetBucketLocation` | Get bucket region |
| `s3:CreateBucket` | Create buckets |
| `s3:DeleteBucket` | Delete buckets |
| `s3:ListAllMyBuckets` | List all buckets |
| `s3:GetBucketPolicy` | Read bucket policy |
| `s3:PutBucketPolicy` | Set bucket policy |
| `s3:DeleteBucketPolicy` | Delete bucket policy |
| `s3:GetBucketVersioning` | Get versioning config |
| `s3:PutBucketVersioning` | Set versioning config |

#### Resources

Resources use ARN format:

```
arn:aws:s3:::bucket-name           # Bucket itself
arn:aws:s3:::bucket-name/*         # All objects in bucket
arn:aws:s3:::bucket-name/prefix/*  # Objects with prefix
arn:aws:s3:::*                     # All buckets
```

### Attaching Policies

```bash
# Attach policy from file
sx admin policy attach local alice --file policy.json

# Attach inline policy
sx admin policy attach local alice --policy '{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": ["s3:*"],
    "Resource": ["arn:aws:s3:::alice-bucket", "arn:aws:s3:::alice-bucket/*"]
  }]
}'

# List attached policies
sx admin policy list local alice

# Detach policy
sx admin policy detach local alice MyPolicyName
```

### Common Policy Examples

#### Read-Only Access to Bucket

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "ReadOnlyAccess",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::my-bucket",
        "arn:aws:s3:::my-bucket/*"
      ]
    }
  ]
}
```

#### Read-Write Access to Bucket

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "ReadWriteAccess",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::my-bucket",
        "arn:aws:s3:::my-bucket/*"
      ]
    }
  ]
}
```

#### Full Access to Own Bucket

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "FullBucketAccess",
      "Effect": "Allow",
      "Action": ["s3:*"],
      "Resource": [
        "arn:aws:s3:::${aws:username}-*",
        "arn:aws:s3:::${aws:username}-*/*"
      ]
    }
  ]
}
```

#### Deny Delete Operations

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "AllowReadWrite",
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject", "s3:ListBucket"],
      "Resource": ["arn:aws:s3:::my-bucket", "arn:aws:s3:::my-bucket/*"]
    },
    {
      "Sid": "DenyDelete",
      "Effect": "Deny",
      "Action": ["s3:DeleteObject", "s3:DeleteBucket"],
      "Resource": ["arn:aws:s3:::my-bucket", "arn:aws:s3:::my-bucket/*"]
    }
  ]
}
```

## Bucket Policies

Bucket policies are attached to buckets and control access at the bucket level.

### Setting Bucket Policy

```bash
# Using Admin API
curl -X PUT http://localhost:9001/api/v1/buckets/my-bucket/policy \
  -H "Content-Type: application/json" \
  -d '{
    "Version": "2012-10-17",
    "Statement": [{
      "Sid": "PublicRead",
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject"],
      "Resource": ["arn:aws:s3:::my-bucket/*"]
    }]
  }'

# Get bucket policy
curl http://localhost:9001/api/v1/buckets/my-bucket/policy

# Delete bucket policy
curl -X DELETE http://localhost:9001/api/v1/buckets/my-bucket/policy
```

### Bucket Policy Structure

Bucket policies are similar to IAM policies but include a **Principal** element:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "StatementId",
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject"],
      "Resource": ["arn:aws:s3:::my-bucket/*"]
    }
  ]
}
```

### Principal Types

| Principal | Description |
|-----------|-------------|
| `"*"` | Anyone (anonymous access) |
| `{"AWS": "*"}` | Any authenticated AWS/Strix user |
| `{"AWS": "arn:aws:iam:::user/alice"}` | Specific user |
| `{"AWS": ["arn:aws:iam:::user/alice", "arn:aws:iam:::user/bob"]}` | Multiple users |

### Common Bucket Policy Examples

#### Public Read Access

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicRead",
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject"],
      "Resource": ["arn:aws:s3:::public-bucket/*"]
    }
  ]
}
```

#### Cross-User Access

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "AllowBobRead",
      "Effect": "Allow",
      "Principal": {"AWS": "arn:aws:iam:::user/bob"},
      "Action": ["s3:GetObject", "s3:ListBucket"],
      "Resource": [
        "arn:aws:s3:::alice-bucket",
        "arn:aws:s3:::alice-bucket/*"
      ]
    }
  ]
}
```

#### Upload-Only (Write-Only) Access

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "WriteOnly",
      "Effect": "Allow",
      "Principal": {"AWS": "arn:aws:iam:::user/uploader"},
      "Action": ["s3:PutObject"],
      "Resource": ["arn:aws:s3:::upload-bucket/*"]
    }
  ]
}
```

#### Block Public Access

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "DenyPublicAccess",
      "Effect": "Deny",
      "Principal": "*",
      "Action": ["s3:*"],
      "Resource": [
        "arn:aws:s3:::private-bucket",
        "arn:aws:s3:::private-bucket/*"
      ],
      "Condition": {
        "Bool": {
          "aws:SecureTransport": "false"
        }
      }
    }
  ]
}
```

## Policy Evaluation

When a request is made, Strix evaluates policies in this order:

1. **Explicit Deny** - If any policy explicitly denies the action, access is denied
2. **Explicit Allow** - If a policy explicitly allows the action, check continues
3. **Default Deny** - If no policy allows the action, access is denied

### Evaluation Flow

```
Request → Check IAM Policies → Check Bucket Policy → Allow/Deny
           ↓                    ↓
        Deny wins           Deny wins
```

### Root User

The root user (configured via `STRIX_ROOT_USER`) bypasses all policy checks and has full access to all resources.

## Conditions (Planned)

Conditions allow fine-grained control based on request context:

```json
{
  "Condition": {
    "IpAddress": {
      "aws:SourceIp": "192.168.1.0/24"
    },
    "DateLessThan": {
      "aws:CurrentTime": "2024-12-31T23:59:59Z"
    }
  }
}
```

Supported condition operators (planned):

| Operator | Description |
|----------|-------------|
| `StringEquals` | Exact string match |
| `StringNotEquals` | String does not match |
| `StringLike` | Wildcard string match |
| `IpAddress` | IP address/CIDR match |
| `NotIpAddress` | IP address does not match |
| `DateLessThan` | Before date |
| `DateGreaterThan` | After date |
| `Bool` | Boolean value |

## Best Practices

1. **Least Privilege** - Grant only the permissions needed
2. **Use Groups** (when available) - Manage permissions via groups, not individual users
3. **Prefer IAM Policies** - Use bucket policies only for cross-account or anonymous access
4. **Avoid Wildcards** - Be specific with actions and resources when possible
5. **Regular Audits** - Review policies periodically
6. **Document Policies** - Use meaningful Sid values

## Troubleshooting

### Access Denied Errors

1. Check if the user exists and is active
2. Verify the access key is active
3. Check IAM policies attached to the user
4. Check bucket policy if accessing a specific bucket
5. Verify the resource ARN matches the policy

### Debugging

Enable debug logging to see policy evaluation:

```bash
STRIX_LOG_LEVEL=debug ./strix
```

Look for log entries like:
```
DEBUG strix_iam: Evaluating policy for user=alice action=s3:GetObject resource=arn:aws:s3:::my-bucket/file.txt
DEBUG strix_iam: IAM policy evaluation result: Allow
DEBUG strix_iam: Bucket policy evaluation result: Allow
DEBUG strix_iam: Final decision: Allow
```
