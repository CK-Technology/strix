# CLI Reference (sx)

The Strix CLI (`sx`) is a command-line tool for interacting with Strix servers. It provides S3 operations and server administration commands.

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

# With admin API URL (for user/policy management)
sx alias set local http://localhost:9000 admin password123 --admin-url http://localhost:9001

# List aliases
sx alias list

# Remove an alias
sx alias remove local
```

Aliases are stored in `~/.config/sx/config.json`.

## Command Reference

### Bucket Operations

#### List Buckets

```bash
# List all buckets
sx ls local

# List with object versions
sx ls local --versions
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

# Remove with prefix (recursive)
sx rm -r local/my-bucket/folder/

# Force remove (no confirmation)
sx rm -r -f local/my-bucket/folder/

# Remove all versions
sx rm --versions local/my-bucket/file.txt
```

#### Object Information

```bash
# Get object metadata
sx stat local/my-bucket/file.txt
```

### User Management

```bash
# List users
sx user list local

# Create user (returns access key and secret key)
sx user add local alice

# Get user info
sx user info local alice

# Delete user
sx user remove local alice
```

### Access Key Management

```bash
# List access keys for user
sx key list local alice

# Create new access key
sx key create local alice

# Delete access key
sx key remove local AKIAIOSFODNN7EXAMPLE
```

### Group Management

```bash
# List groups
sx group list local

# Create group
sx group add local developers

# Get group info
sx group info local developers

# Delete group
sx group remove local developers

# Add user to group
sx group add-member local developers alice

# Remove user from group
sx group remove-member local developers alice

# Attach policy to group
sx group attach-policy local developers ReadOnlyAccess

# Detach policy from group
sx group detach-policy local developers ReadOnlyAccess
```

### Policy Management

```bash
# List all managed policies
sx policy list local

# Create a managed policy from a JSON document
sx policy add local MyPolicy '{"Version":"2012-10-17","Statement":[...]}'

# Create with description
sx policy add local MyPolicy '{"Version":"2012-10-17","Statement":[...]}' -d "Read-only access"

# Get policy details
sx policy info local MyPolicy

# Delete a managed policy
sx policy remove local MyPolicy

# Attach policy to user
sx policy attach local MyPolicy alice

# Detach policy from user
sx policy detach local MyPolicy alice
```

### Event Notifications

```bash
# List notification rules for a bucket
sx event list local my-bucket

# Add a notification rule
sx event add local my-bucket \
  -e s3:ObjectCreated:* \
  -a arn:strix:webhook:::https://example.com/hook

# Add with filters
sx event add local my-bucket \
  -e s3:ObjectCreated:Put -e s3:ObjectRemoved:Delete \
  -a arn:strix:webhook:::https://example.com/hook \
  --prefix logs/ --suffix .json

# Remove a notification rule
sx event remove local my-bucket <rule-id>
```

### Server Settings

```bash
# Get all settings
sx settings get local

# Get a specific setting
sx settings get local region

# Set a setting
sx settings set local region us-west-2
```

### Server Information

```bash
# Show server info (version, mode, uptime, region)
sx info local

# Show storage usage (buckets, objects, sizes)
sx usage local
```

## Examples

### Backup Script

```bash
#!/bin/bash
DATE=$(date +%Y-%m-%d)
BACKUP_DIR="/var/backups"

# Upload backup to dated prefix
sx cp -r "$BACKUP_DIR/" local/backups/$DATE/

# Clean up old backups
for old in $(sx ls local/backups/ | awk '{print $3}' | sort | head -n -7); do
  sx rm -r -f "local/backups/$old"
done
```
