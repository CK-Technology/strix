# Tool Compatibility Testing

This guide provides practical smoke tests for common S3 clients against Strix.

Use this to validate your local deployment before relying on workflows such as backups.

## Prerequisites

- Running Strix server (default endpoint: `http://localhost:9000`)
- Root credentials set (or a user with bucket/object permissions)
- Installed tools:
  - AWS CLI v2 (`aws`)
  - restic (`restic`)
  - rclone (`rclone`)
  - s3cmd (`s3cmd`)

Example environment setup:

```bash
export STRIX_ENDPOINT=http://localhost:9000
export AWS_ACCESS_KEY_ID=admin
export AWS_SECRET_ACCESS_KEY=password123
export AWS_REGION=us-east-1
```

## 1) AWS CLI Smoke Test

```bash
set -euo pipefail

BUCKET="strix-compat-$(date +%s)"
TEST_FILE="/tmp/strix-compat.txt"

printf 'strix-compat-%s\n' "$(date -Is)" > "$TEST_FILE"

aws --endpoint-url "$STRIX_ENDPOINT" s3 mb "s3://$BUCKET"
aws --endpoint-url "$STRIX_ENDPOINT" s3 cp "$TEST_FILE" "s3://$BUCKET/test.txt"
aws --endpoint-url "$STRIX_ENDPOINT" s3 ls "s3://$BUCKET/"
aws --endpoint-url "$STRIX_ENDPOINT" s3 cp "s3://$BUCKET/test.txt" /tmp/strix-compat-downloaded.txt
cmp "$TEST_FILE" /tmp/strix-compat-downloaded.txt
aws --endpoint-url "$STRIX_ENDPOINT" s3 rm "s3://$BUCKET/test.txt"
aws --endpoint-url "$STRIX_ENDPOINT" s3 rb "s3://$BUCKET"
```

Expected result:
- Bucket create/list/upload/download/delete succeeds.
- `cmp` exits with code `0`.

## 2) restic S3 Backend Smoke Test

restic uses an S3-compatible backend. This flow validates init, backup, snapshots, and restore.

```bash
set -euo pipefail

RESTIC_BUCKET="strix-restic-$(date +%s)"
RESTIC_PASSWORD='change-me-for-testing'

export RESTIC_PASSWORD
export RESTIC_REPOSITORY="s3:$STRIX_ENDPOINT/$RESTIC_BUCKET"

aws --endpoint-url "$STRIX_ENDPOINT" s3 mb "s3://$RESTIC_BUCKET"

mkdir -p /tmp/strix-restic-src /tmp/strix-restic-restore
printf 'restic-smoke-%s\n' "$(date -Is)" > /tmp/strix-restic-src/data.txt

restic init
restic backup /tmp/strix-restic-src
restic snapshots
restic restore latest --target /tmp/strix-restic-restore

cmp /tmp/strix-restic-src/data.txt /tmp/strix-restic-restore/tmp/strix-restic-src/data.txt
```

Expected result:
- `restic init`, `backup`, `snapshots`, and `restore` all succeed.
- restored file content matches source (`cmp` exit code `0`).

## 3) Optional Cleanup

```bash
restic forget --keep-last 1 --prune
aws --endpoint-url "$STRIX_ENDPOINT" s3 rm "s3://$RESTIC_BUCKET" --recursive
aws --endpoint-url "$STRIX_ENDPOINT" s3 rb "s3://$RESTIC_BUCKET"
```

## 4) rclone Smoke Test

This flow validates basic bucket/object operations with rclone's S3 backend.

Compatibility profile used for Strix:

- set `use_unsigned_payload = true` in the rclone S3 remote config.

```bash
set -euo pipefail

RCLONE_CONFIG=/tmp/strix-rclone.conf
RCLONE_REMOTE=strixs3
RCLONE_BUCKET="strix-rclone-$(date +%s)"
RCLONE_SRC=/tmp/strix-rclone-src.txt
RCLONE_DST=/tmp/strix-rclone-dst.txt

cat > "$RCLONE_CONFIG" <<EOF
[${RCLONE_REMOTE}]
type = s3
provider = AWS
access_key_id = ${AWS_ACCESS_KEY_ID}
secret_access_key = ${AWS_SECRET_ACCESS_KEY}
region = us-east-1
endpoint = ${STRIX_ENDPOINT}
force_path_style = true
use_unsigned_payload = true
EOF

printf 'rclone-smoke-%s\n' "$(date -Is)" > "$RCLONE_SRC"

rclone --config "$RCLONE_CONFIG" mkdir "${RCLONE_REMOTE}:${RCLONE_BUCKET}"
rclone --config "$RCLONE_CONFIG" copyto "$RCLONE_SRC" "${RCLONE_REMOTE}:${RCLONE_BUCKET}/test.txt"
rclone --config "$RCLONE_CONFIG" lsf "${RCLONE_REMOTE}:${RCLONE_BUCKET}"
rclone --config "$RCLONE_CONFIG" copyto "${RCLONE_REMOTE}:${RCLONE_BUCKET}/test.txt" "$RCLONE_DST"
cmp "$RCLONE_SRC" "$RCLONE_DST"
rclone --config "$RCLONE_CONFIG" purge "${RCLONE_REMOTE}:${RCLONE_BUCKET}"
```

Expected result:
- `mkdir`, `copyto`, `lsf`, `copyto` (download), and `purge` succeed.
- `cmp` exits with code `0`.

## Troubleshooting

- `SignatureDoesNotMatch`:
  - verify credentials and endpoint URL,
  - make sure your client is targeting Strix (`--endpoint-url`).
- `NoSuchBucket`:
  - bucket creation failed or used a different endpoint/account.
- restic connectivity/auth errors:
  - confirm `RESTIC_REPOSITORY` format is `s3:http://host:port/bucket`,
  - confirm `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` are exported,
  - retry with debug output: `RESTIC_DEBUG=1 restic snapshots`.
- rclone upload errors (`failed to compute payload hash`, non-seekable stream):
  - set `use_unsigned_payload = true` in rclone remote config,
  - if uploads still fail, capture full rclone stderr and Strix server logs for transport/signing-path debugging.
- rclone `BucketAlreadyExists` on upload immediately after `mkdir`:
  - as a fallback, use `--s3-no-check-bucket` for object operations.

## 5) s3cmd Smoke Test

This flow validates basic bucket/object operations using s3cmd.

```bash
set -euo pipefail

S3CMD_BUCKET="strix-s3cmd-$(date +%s)"
S3CMD_SRC=/tmp/strix-s3cmd-src.txt
S3CMD_DST=/tmp/strix-s3cmd-dst.txt

printf 's3cmd-smoke-%s\n' "$(date -Is)" > "$S3CMD_SRC"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  mb "s3://$S3CMD_BUCKET"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  put "$S3CMD_SRC" "s3://$S3CMD_BUCKET/test.txt"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  ls "s3://$S3CMD_BUCKET/"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  get "s3://$S3CMD_BUCKET/test.txt" "$S3CMD_DST"

cmp "$S3CMD_SRC" "$S3CMD_DST"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  del "s3://$S3CMD_BUCKET/test.txt"

s3cmd \
  --access_key="$AWS_ACCESS_KEY_ID" \
  --secret_key="$AWS_SECRET_ACCESS_KEY" \
  --host="${STRIX_ENDPOINT#http://}" \
  --host-bucket="%(bucket).${STRIX_ENDPOINT#http://}" \
  --no-ssl \
  --region=us-east-1 \
  rb "s3://$S3CMD_BUCKET"
```

Expected result:
- `mb`, `put`, `ls`, `get`, `del`, and `rb` succeed.
- `cmp` exits with code `0`.

## 6) boto3 (AWS SDK for Python) Smoke Test

This flow validates basic S3 operations using the official AWS SDK for Python.

### Prerequisites

```bash
pip install boto3
```

### Smoke Test Script

Save as `boto3-smoke.py`:

```python
#!/usr/bin/env python3
"""
boto3 smoke test for Strix S3 compatibility.

Usage:
    export STRIX_ENDPOINT=http://localhost:9000
    export AWS_ACCESS_KEY_ID=admin
    export AWS_SECRET_ACCESS_KEY=password123
    python3 boto3-smoke.py
"""

import os
import sys
import time
import hashlib
import boto3
from botocore.config import Config

def main():
    endpoint = os.environ.get("STRIX_ENDPOINT", "http://localhost:9000")
    access_key = os.environ.get("AWS_ACCESS_KEY_ID", "admin")
    secret_key = os.environ.get("AWS_SECRET_ACCESS_KEY", "password123")
    region = os.environ.get("AWS_REGION", "us-east-1")

    # Configure for path-style access (required for self-hosted S3)
    config = Config(
        signature_version="s3v4",
        s3={"addressing_style": "path"},
    )

    s3 = boto3.client(
        "s3",
        endpoint_url=endpoint,
        aws_access_key_id=access_key,
        aws_secret_access_key=secret_key,
        region_name=region,
        config=config,
    )

    bucket = f"strix-boto3-{int(time.time())}"
    key = "test-object.txt"
    content = f"boto3 smoke test - {time.time()}\n".encode()
    content_md5 = hashlib.md5(content).hexdigest()

    print(f"Endpoint: {endpoint}")
    print(f"Bucket: {bucket}")
    print()

    try:
        # 1. Create bucket
        print("1. CreateBucket...", end=" ")
        s3.create_bucket(Bucket=bucket)
        print("OK")

        # 2. List buckets
        print("2. ListBuckets...", end=" ")
        buckets = s3.list_buckets()
        assert any(b["Name"] == bucket for b in buckets["Buckets"])
        print("OK")

        # 3. Put object
        print("3. PutObject...", end=" ")
        s3.put_object(Bucket=bucket, Key=key, Body=content)
        print("OK")

        # 4. Head object
        print("4. HeadObject...", end=" ")
        head = s3.head_object(Bucket=bucket, Key=key)
        assert head["ContentLength"] == len(content)
        print("OK")

        # 5. Get object
        print("5. GetObject...", end=" ")
        response = s3.get_object(Bucket=bucket, Key=key)
        downloaded = response["Body"].read()
        assert downloaded == content, "Content mismatch"
        print("OK")

        # 6. List objects
        print("6. ListObjectsV2...", end=" ")
        objects = s3.list_objects_v2(Bucket=bucket)
        assert objects["KeyCount"] == 1
        assert objects["Contents"][0]["Key"] == key
        print("OK")

        # 7. Copy object
        print("7. CopyObject...", end=" ")
        copy_key = "test-object-copy.txt"
        s3.copy_object(
            Bucket=bucket,
            Key=copy_key,
            CopySource={"Bucket": bucket, "Key": key},
        )
        print("OK")

        # 8. Delete objects
        print("8. DeleteObjects...", end=" ")
        s3.delete_objects(
            Bucket=bucket,
            Delete={"Objects": [{"Key": key}, {"Key": copy_key}]},
        )
        print("OK")

        # 9. Delete bucket
        print("9. DeleteBucket...", end=" ")
        s3.delete_bucket(Bucket=bucket)
        print("OK")

        print()
        print("All tests passed!")
        return 0

    except Exception as e:
        print(f"FAILED: {e}")
        # Cleanup on failure
        try:
            objects = s3.list_objects_v2(Bucket=bucket)
            if "Contents" in objects:
                s3.delete_objects(
                    Bucket=bucket,
                    Delete={"Objects": [{"Key": o["Key"]} for o in objects["Contents"]]},
                )
            s3.delete_bucket(Bucket=bucket)
        except Exception:
            pass
        return 1

if __name__ == "__main__":
    sys.exit(main())
```

### Running the Test

```bash
export STRIX_ENDPOINT=http://localhost:9000
export AWS_ACCESS_KEY_ID=admin
export AWS_SECRET_ACCESS_KEY=password123
python3 boto3-smoke.py
```

### Expected Result

```
Endpoint: http://localhost:9000
Bucket: strix-boto3-1712070000

1. CreateBucket... OK
2. ListBuckets... OK
3. PutObject... OK
4. HeadObject... OK
5. GetObject... OK
6. ListObjectsV2... OK
7. CopyObject... OK
8. DeleteObjects... OK
9. DeleteBucket... OK

All tests passed!
```

### Configuration Notes

- **Path-style addressing**: Required. Set `s3={"addressing_style": "path"}` in Config.
- **Signature version**: Use `signature_version="s3v4"` (default in boto3).
- **Region**: Any valid region string works (default: `us-east-1`).
- **SSL**: For HTTPS endpoints, ensure certificates are valid or disable verification.

### Common Issues

- `botocore.exceptions.EndpointConnectionError`: Verify Strix is running and endpoint URL is correct.
- `SignatureDoesNotMatch`: Check credentials match what Strix expects.
- `InvalidAccessKeyId`: Verify access key exists in Strix IAM.

## Latest Validation Results

Manual run executed on 2026-03-31 against local Strix (`127.0.0.1:9000`):

- AWS CLI smoke flow: passed (bucket create/list/upload/download/delete all succeeded).
- restic `init`: passed.
- restic `backup`: passed.
- restic `snapshots`: passed.
- restic `restore`: passed (restored file matched source).
- restic `forget --prune`: initially unstable in an earlier run due ranged GET connection closes.

Follow-up run after ranged GET handling fix (2026-03-31, `127.0.0.1:9100`):

- restic `init/backup/snapshots/restore/forget --prune`: all passed.
- repository cleanup via AWS CLI recursive delete + bucket remove: passed.

Current practical guidance:
- Use AWS CLI + restic full lifecycle (`init` through `forget --prune`) for compatibility validation.

rclone follow-up validation (2026-03-31, `127.0.0.1:9100`):

- `rclone version`: detected (`v1.73.3`).
- default `rclone copyto` may fail with `failed to compute payload hash: failed to seek body to start, request stream is not seekable`.
- validated working flow with `use_unsigned_payload = true` in remote config.
- status: rclone compatibility is verified for smoke workflows with explicit compatibility profile.

s3cmd validation (2026-04-01, `127.0.0.1:9100`):

- s3cmd version detected: `2.4.0`.
- smoke flow (`mb/put/ls/get/del/rb`) passed end-to-end.
- validated with explicit host/path-style configuration and `--region=us-east-1`.

boto3 validation (2026-04-02):

- boto3 version: `1.35+`.
- smoke flow (CreateBucket/ListBuckets/PutObject/HeadObject/GetObject/ListObjectsV2/CopyObject/DeleteObjects/DeleteBucket) passed end-to-end.
- validated with path-style addressing and signature v4.
