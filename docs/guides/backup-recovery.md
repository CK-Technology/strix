# Backup and Recovery

Backup strategies and disaster recovery procedures for Strix.

## Data Layout

Understanding Strix's data layout is essential for backup planning:

```
{data-dir}/
├── meta/
│   ├── strix.db          # SQLite database (object metadata, buckets)
│   ├── iam.db            # SQLite database (IAM users, keys, policies)
│   └── encryption.key    # Master encryption key for SSE-S3
├── objects/              # Object blobs with sharded paths
│   └── ab/cd/{object_id}.blob
├── multipart/            # In-progress multipart uploads
│   └── {upload-id}/
│       └── {part_number}.part
└── tmp/                  # Temporary files (ephemeral)
```

### Critical Files

| Component | Path | Description |
|-----------|------|-------------|
| Storage Database | `meta/strix.db` | Bucket and object metadata |
| IAM Database | `meta/iam.db` | Users, access keys, policies, groups |
| Encryption Key | `meta/encryption.key` | SSE-S3 master key (critical for encrypted objects) |
| Object Data | `objects/` | Actual object content (sharded by ID prefix) |
| Multipart Uploads | `multipart/` | In-progress uploads (ephemeral) |
| Temp Files | `tmp/` | Temporary processing files (ephemeral) |

**Warning**: If you lose `meta/encryption.key`, any objects encrypted with SSE-S3 become unrecoverable. Always back up this file securely.

## Backup Strategies

### Full Backup

Stop the server and copy the entire data directory:

```bash
# Stop Strix
systemctl stop strix

# Create backup
tar -czf strix-backup-$(date +%Y%m%d).tar.gz /var/lib/strix

# Restart Strix
systemctl start strix
```

### Online Backup

For minimal downtime, use SQLite's backup API and filesystem snapshots:

```bash
#!/bin/bash
# online-backup.sh

DATA_DIR=/var/lib/strix
BACKUP_DIR=/backup/strix
DATE=$(date +%Y%m%d-%H%M%S)

mkdir -p $BACKUP_DIR/$DATE

# Backup SQLite databases (online, consistent)
sqlite3 $DATA_DIR/meta/strix.db ".backup '$BACKUP_DIR/$DATE/strix.db'"
sqlite3 $DATA_DIR/meta/iam.db ".backup '$BACKUP_DIR/$DATE/iam.db'"

# Backup encryption key (critical!)
cp $DATA_DIR/meta/encryption.key $BACKUP_DIR/$DATE/encryption.key

# Sync object data (rsync for incremental)
rsync -av --delete $DATA_DIR/objects/ $BACKUP_DIR/$DATE/objects/

# Create manifest
cat > $BACKUP_DIR/$DATE/manifest.json << EOF
{
  "date": "$(date -Iseconds)",
  "version": "$(strix --version 2>/dev/null || echo 'unknown')",
  "strix_db_size": $(stat -c%s $BACKUP_DIR/$DATE/strix.db),
  "iam_db_size": $(stat -c%s $BACKUP_DIR/$DATE/iam.db)
}
EOF

echo "Backup completed: $BACKUP_DIR/$DATE"
```

### Incremental Backup

For large deployments, use incremental object backups:

```bash
#!/bin/bash
# incremental-backup.sh

DATA_DIR=/var/lib/strix
BACKUP_DIR=/backup/strix
LAST_BACKUP_FILE=$BACKUP_DIR/.last_backup_time

# Get last backup time
SINCE=""
if [ -f $LAST_BACKUP_FILE ]; then
    SINCE="--newer-mtime=$(cat $LAST_BACKUP_FILE)"
fi

# Backup databases (always full)
sqlite3 $DATA_DIR/meta/strix.db ".backup '$BACKUP_DIR/incremental/strix.db'"
sqlite3 $DATA_DIR/meta/iam.db ".backup '$BACKUP_DIR/incremental/iam.db'"

# Always backup encryption key
cp $DATA_DIR/meta/encryption.key $BACKUP_DIR/incremental/encryption.key

# Incremental object backup
find $DATA_DIR/objects -type f $SINCE -print0 | \
    tar -czvf $BACKUP_DIR/incremental/objects-$(date +%Y%m%d%H%M%S).tar.gz --null -T -

# Update timestamp
date -Iseconds > $LAST_BACKUP_FILE
```

### Filesystem Snapshots

If using ZFS, Btrfs, or LVM:

```bash
# ZFS
zfs snapshot tank/strix@backup-$(date +%Y%m%d)

# Btrfs
btrfs subvolume snapshot /var/lib/strix /var/lib/strix-snapshots/$(date +%Y%m%d)

# LVM
lvcreate -L 10G -s -n strix-backup /dev/vg0/strix
```

## Restore Procedures

### Full Restore

```bash
# Stop Strix
systemctl stop strix

# Remove existing data (careful!)
rm -rf /var/lib/strix/*

# Extract backup
tar -xzf strix-backup-20240115.tar.gz -C /

# Verify SQLite integrity
sqlite3 /var/lib/strix/meta/strix.db "PRAGMA integrity_check"
sqlite3 /var/lib/strix/meta/iam.db "PRAGMA integrity_check"

# Start Strix
systemctl start strix
```

### Point-in-Time Recovery

For incremental backups:

```bash
# 1. Restore latest full backup
tar -xzf strix-backup-full.tar.gz -C /var/lib/strix

# 2. Apply incremental object backups in order
for backup in /backup/strix/incremental/objects-*.tar.gz; do
    tar -xzf $backup -C /var/lib/strix/objects/
done

# 3. Restore latest database backups
cp /backup/strix/incremental/strix.db /var/lib/strix/meta/
cp /backup/strix/incremental/iam.db /var/lib/strix/meta/
cp /backup/strix/incremental/encryption.key /var/lib/strix/meta/
```

## Disaster Recovery

### Prerequisites

- Backup storage in separate location/region
- Documented recovery procedures
- Regular backup verification
- Secure storage for encryption key backup

### Recovery Runbook

1. **Assess Damage**
   - Identify failed components
   - Check backup availability
   - Verify encryption key backup exists

2. **Provision Infrastructure**
   - New server with same OS
   - Required storage capacity
   - Network configuration

3. **Install Strix**
   ```bash
   # Download Strix binary
   curl -LO https://github.com/CK-Technology/strix/releases/latest/download/strix-linux-x86_64
   chmod +x strix-linux-x86_64
   mv strix-linux-x86_64 /usr/local/bin/strix

   # Create data directory
   mkdir -p /var/lib/strix
   ```

4. **Restore Data**
   ```bash
   # Restore from backup
   ./restore-backup.sh /backup/strix/latest
   ```

5. **Update Configuration**
   ```bash
   # Set credentials
   export STRIX_ROOT_USER=admin
   export STRIX_ROOT_PASSWORD=<from-secure-storage>

   # Update addresses if needed
   export STRIX_ADDRESS=0.0.0.0:9000
   ```

6. **Verify Restoration**
   ```bash
   # Check health
   curl http://localhost:9001/health/ready

   # List buckets
   aws --endpoint-url http://localhost:9000 s3 ls
   ```

7. **Update DNS/Load Balancer**
   - Point to new server
   - Verify connectivity

## Backup Verification

Regularly verify backups by restoring to a test environment:

```bash
#!/bin/bash
# verify-backup.sh

BACKUP_PATH=$1
TEST_PORT=19000
TEST_CONSOLE_PORT=19001
TEST_DATA_DIR=/tmp/strix-verify-$$

# Create test environment
mkdir -p $TEST_DATA_DIR
tar -xzf $BACKUP_PATH -C $TEST_DATA_DIR

# Start test instance
STRIX_DATA_DIR=$TEST_DATA_DIR \
STRIX_ADDRESS=127.0.0.1:$TEST_PORT \
STRIX_CONSOLE_ADDRESS=127.0.0.1:$TEST_CONSOLE_PORT \
STRIX_ROOT_USER=admin \
STRIX_ROOT_PASSWORD=testpassword123 \
strix &

STRIX_PID=$!
sleep 5

# Verify
HEALTH=$(curl -s http://127.0.0.1:$TEST_CONSOLE_PORT/health/ready)
BUCKET_COUNT=$(aws --endpoint-url http://127.0.0.1:$TEST_PORT s3 ls 2>/dev/null | wc -l)

# Cleanup
kill $STRIX_PID
rm -rf $TEST_DATA_DIR

# Report
echo "Health: $HEALTH"
echo "Buckets: $BUCKET_COUNT"
```

## Best Practices

1. **3-2-1 Rule**: 3 copies, 2 different media types, 1 offsite
2. **Encryption**: Encrypt backups at rest and in transit
3. **Retention**: Keep daily backups for 7 days, weekly for 4 weeks, monthly for 12 months
4. **Testing**: Test restore procedures quarterly
5. **Documentation**: Keep recovery procedures updated
6. **Automation**: Use cron/systemd timers for regular backups
7. **Encryption Key**: Store `encryption.key` backup separately and securely
