# CLI Reference (sx)

The Strix CLI (`sx`) is a command-line tool for interacting with Strix servers. It provides S3 operations and administrative commands.

## Installation

```bash
# From source
cargo install --path crates/strix-cli

# Or build with the workspace
cargo build --release
./target/release/sx --help
```

## Configuration

### Setting Up Aliases

Before using `sx`, configure an alias for your Strix server:

```bash
# Add an alias
sx alias set local http://localhost:9000 admin password123

# List aliases
sx alias list

# Remove an alias
sx alias remove local
```

Aliases are stored in `~/.config/sx/config.json`.

### Alias Format

```json
{
  "aliases": {
    "local": {
      "endpoint": "http://localhost:9000",
      "access_key": "admin",
      "secret_key": "password123"
    },
    "prod": {
      "endpoint": "https://s3.example.com",
      "access_key": "AKIAIOSFODNN7EXAMPLE",
      "secret_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    }
  }
}
```

## Command Reference

### Bucket Operations

#### List Buckets

```bash
# List all buckets
sx ls local

# Output:
# 2024-01-01 00:00:00  my-bucket
# 2024-01-02 00:00:00  another-bucket
```

#### Create Bucket

```bash
# Create a bucket
sx mb local/my-bucket

# With region
sx mb local/my-bucket --region us-west-2
```

#### Remove Bucket

```bash
# Remove empty bucket
sx rb local/my-bucket

# Force remove (delete all objects first)
sx rb local/my-bucket --force
```

### Object Operations

#### List Objects

```bash
# List objects in bucket
sx ls local/my-bucket

# List with prefix
sx ls local/my-bucket/folder/

# Recursive listing
sx ls local/my-bucket -r

# Output:
# 2024-01-01 00:00:00    1.2KiB file.txt
# 2024-01-01 00:00:00    5.0MiB image.png
# 2024-01-01 00:00:00      DIR folder/
```

#### Copy/Upload Files

```bash
# Upload a file
sx cp file.txt local/my-bucket/

# Upload to specific key
sx cp file.txt local/my-bucket/path/to/file.txt

# Upload directory recursively
sx cp -r ./folder/ local/my-bucket/backup/

# Download a file
sx cp local/my-bucket/file.txt ./downloaded.txt

# Download directory
sx cp -r local/my-bucket/backup/ ./restored/

# Copy between buckets
sx cp local/bucket1/file.txt local/bucket2/file.txt
```

#### Remove Objects

```bash
# Remove single object
sx rm local/my-bucket/file.txt

# Remove multiple objects
sx rm local/my-bucket/file1.txt local/my-bucket/file2.txt

# Remove with prefix (recursive)
sx rm -r local/my-bucket/folder/

# Force remove (no confirmation)
sx rm -r -f local/my-bucket/folder/
```

#### Object Information

```bash
# Get object metadata
sx stat local/my-bucket/file.txt

# Output:
# Name:         file.txt
# Size:         1234 bytes
# ETag:         "abc123def456"
# Content-Type: text/plain
# Last Modified: 2024-01-01T00:00:00Z
```

#### Cat (Print Object Content)

```bash
# Print object content to stdout
sx cat local/my-bucket/config.json

# Pipe to another command
sx cat local/my-bucket/data.csv | head -10
```

#### Head (Print First Lines)

```bash
# Print first 10 lines
sx head local/my-bucket/log.txt

# Print first N lines
sx head -n 50 local/my-bucket/log.txt
```

### Admin Commands

#### Server Information

```bash
# Get server info
sx admin info local

# Output:
# Version:  0.1.0
# Commit:   abc123
# Mode:     standalone
# Uptime:   3600s
# Region:   us-east-1
```

#### Storage Usage

```bash
# Get storage usage
sx admin usage local

# Output:
# Bucket          Objects    Size
# my-bucket       100        1.2 GiB
# another-bucket  50         500 MiB
# ─────────────────────────────────
# Total           150        1.7 GiB
```

#### User Management

```bash
# List users
sx admin user list local

# Create user
sx admin user add local alice

# Output:
# User created: alice
# Access Key: AKIAIOSFODNN7EXAMPLE
# Secret Key: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
#
# Save these credentials - the secret key cannot be retrieved later!

# Get user info
sx admin user info local alice

# Delete user
sx admin user remove local alice

# Disable user
sx admin user disable local alice

# Enable user
sx admin user enable local alice
```

#### Access Key Management

```bash
# List access keys for user
sx admin key list local alice

# Create new access key
sx admin key add local alice

# Delete access key
sx admin key remove local AKIAIOSFODNN7EXAMPLE

# Disable access key
sx admin key disable local AKIAIOSFODNN7EXAMPLE

# Enable access key
sx admin key enable local AKIAIOSFODNN7EXAMPLE
```

#### Policy Management

```bash
# List user policies
sx admin policy list local alice

# Attach policy from file
sx admin policy attach local alice --file policy.json

# Attach inline policy
sx admin policy attach local alice --policy '{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": ["s3:GetObject"],
    "Resource": ["arn:aws:s3:::my-bucket/*"]
  }]
}'

# Detach policy
sx admin policy detach local alice MyPolicy
```

### Utility Commands

#### Generate Pre-signed URL

```bash
# Generate download URL (default 1 hour)
sx presign local/my-bucket/file.txt

# Custom expiration (in seconds)
sx presign local/my-bucket/file.txt --expires 3600

# Generate upload URL
sx presign local/my-bucket/file.txt --method PUT

# Output:
# http://localhost:9000/my-bucket/file.txt?X-Amz-Algorithm=...
```

#### Mirror (Sync)

```bash
# Mirror local directory to bucket
sx mirror ./local-folder/ local/my-bucket/

# Mirror bucket to local directory
sx mirror local/my-bucket/ ./local-folder/

# Mirror between buckets
sx mirror local/bucket1/ local/bucket2/

# Options
sx mirror --overwrite ./local/ local/my-bucket/  # Overwrite existing
sx mirror --remove ./local/ local/my-bucket/      # Remove extra files
sx mirror --dry-run ./local/ local/my-bucket/     # Preview changes
```

#### Find

```bash
# Find objects by pattern
sx find local/my-bucket --name "*.txt"

# Find by size
sx find local/my-bucket --larger 1M
sx find local/my-bucket --smaller 100K

# Find by age
sx find local/my-bucket --older 7d
sx find local/my-bucket --newer 1h
```

#### Diff

```bash
# Compare local directory with bucket
sx diff ./local-folder/ local/my-bucket/

# Output:
# + file1.txt      (only in local)
# - file2.txt      (only in bucket)
# ~ file3.txt      (different)
```

### Global Options

```bash
# Enable debug output
sx --debug ls local

# Use specific config file
sx --config /path/to/config.json ls local

# JSON output
sx --json ls local/my-bucket

# Quiet mode (errors only)
sx --quiet cp file.txt local/my-bucket/
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SX_CONFIG` | Path to config file |
| `SX_DEBUG` | Enable debug output |
| `SX_ACCESS_KEY` | Override access key |
| `SX_SECRET_KEY` | Override secret key |
| `SX_ENDPOINT` | Override endpoint URL |

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | Connection error |
| 4 | Authentication error |
| 5 | Permission denied |
| 6 | Not found |

## Examples

### Backup Script

```bash
#!/bin/bash
# Daily backup script

DATE=$(date +%Y-%m-%d)
BACKUP_DIR="/var/backups"

# Create dated backup
sx cp -r "$BACKUP_DIR/" local/backups/$DATE/

# Keep only last 7 days
for old in $(sx ls local/backups/ | awk '{print $3}' | sort | head -n -7); do
  sx rm -r -f "local/backups/$old"
done
```

### Migration Script

```bash
#!/bin/bash
# Migrate from MinIO to Strix

# Set up aliases
sx alias set minio http://minio.old:9000 minioadmin minioadmin
sx alias set strix http://strix.new:9000 admin password123

# Mirror all buckets
for bucket in $(sx ls minio | awk '{print $3}'); do
  echo "Migrating $bucket..."
  sx mb "strix/$bucket" 2>/dev/null || true
  sx mirror "minio/$bucket/" "strix/$bucket/"
done

echo "Migration complete!"
```

### Watch for Changes

```bash
#!/bin/bash
# Watch directory and upload changes

while inotifywait -r -e modify,create,delete ./watch-dir/; do
  sx mirror ./watch-dir/ local/synced-bucket/
done
```
