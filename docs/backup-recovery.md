# Backup & Disaster Recovery

This guide covers backup strategies and disaster recovery procedures for Strix.

## Data Layout

Understanding Strix's data layout is essential for backup planning:

```
{data-dir}/
├── meta/
│   ├── iam.db           # SQLite database (IAM users, keys, policies)
│   └── storage.db       # SQLite database (object metadata, buckets)
├── blobs/
│   └── {sharded}/       # Object data (256 shards)
│       └── {blob-id}
└── multipart/
    └── {upload-id}/     # In-progress multipart uploads
```

### Critical Files

| Component | Path | Description |
|-----------|------|-------------|
| IAM Database | `meta/iam.db` | Users, access keys, policies, groups |
| Storage Database | `meta/storage.db` | Bucket and object metadata |
| Object Data | `blobs/` | Actual object content |
| Multipart Uploads | `multipart/` | In-progress uploads (ephemeral) |

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
sqlite3 $DATA_DIR/meta/iam.db ".backup '$BACKUP_DIR/$DATE/iam.db'"
sqlite3 $DATA_DIR/meta/storage.db ".backup '$BACKUP_DIR/$DATE/storage.db'"

# Sync blob data (rsync for incremental)
rsync -av --delete $DATA_DIR/blobs/ $BACKUP_DIR/$DATE/blobs/

# Create manifest
cat > $BACKUP_DIR/$DATE/manifest.json << EOF
{
  "date": "$(date -Iseconds)",
  "version": "$(strix --version)",
  "iam_db_size": $(stat -c%s $BACKUP_DIR/$DATE/iam.db),
  "storage_db_size": $(stat -c%s $BACKUP_DIR/$DATE/storage.db)
}
EOF

echo "Backup completed: $BACKUP_DIR/$DATE"
```

### Incremental Backup

For large deployments, use incremental blob backups:

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
sqlite3 $DATA_DIR/meta/iam.db ".backup '$BACKUP_DIR/incremental/iam.db'"
sqlite3 $DATA_DIR/meta/storage.db ".backup '$BACKUP_DIR/incremental/storage.db'"

# Incremental blob backup
find $DATA_DIR/blobs -type f $SINCE -print0 | \
    tar -czvf $BACKUP_DIR/incremental/blobs-$(date +%Y%m%d%H%M%S).tar.gz --null -T -

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
sqlite3 /var/lib/strix/meta/iam.db "PRAGMA integrity_check"
sqlite3 /var/lib/strix/meta/storage.db "PRAGMA integrity_check"

# Start Strix
systemctl start strix
```

### Point-in-Time Recovery

For incremental backups:

```bash
# 1. Restore latest full backup
tar -xzf strix-backup-full.tar.gz -C /var/lib/strix

# 2. Apply incremental blob backups in order
for backup in /backup/strix/incremental/blobs-*.tar.gz; do
    tar -xzf $backup -C /var/lib/strix/blobs/
done

# 3. Restore latest database backups
cp /backup/strix/incremental/iam.db /var/lib/strix/meta/
cp /backup/strix/incremental/storage.db /var/lib/strix/meta/
```

## Disaster Recovery

### Prerequisites

- Backup storage in separate location/region
- Documented recovery procedures
- Regular backup verification

### Recovery Runbook

1. **Assess Damage**
   - Identify failed components
   - Check backup availability

2. **Provision Infrastructure**
   - New server with same OS
   - Required storage capacity
   - Network configuration

3. **Install Strix**
   ```bash
   # Install Strix binary
   curl -sSL https://get.strix.io | sh

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
   curl http://localhost:9000/health/ready

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
TEST_DATA_DIR=/tmp/strix-verify-$$

# Create test environment
mkdir -p $TEST_DATA_DIR
tar -xzf $BACKUP_PATH -C $TEST_DATA_DIR

# Start test instance
STRIX_DATA_DIR=$TEST_DATA_DIR \
STRIX_ADDRESS=127.0.0.1:$TEST_PORT \
STRIX_ROOT_USER=admin \
STRIX_ROOT_PASSWORD=test \
strix &

STRIX_PID=$!
sleep 5

# Verify
HEALTH=$(curl -s http://127.0.0.1:$TEST_PORT/health/ready)
BUCKET_COUNT=$(aws --endpoint-url http://127.0.0.1:$TEST_PORT s3 ls | wc -l)

# Cleanup
kill $STRIX_PID
rm -rf $TEST_DATA_DIR

# Report
echo "Health: $HEALTH"
echo "Buckets: $BUCKET_COUNT"
```

## Monitoring Backups

Set up alerts for:

- Backup job failures
- Backup size anomalies
- Time since last successful backup
- Backup storage capacity

```yaml
# Example Prometheus alerts
groups:
  - name: strix-backup
    rules:
      - alert: BackupFailed
        expr: strix_backup_last_success_timestamp < time() - 86400
        for: 1h
        labels:
          severity: critical
        annotations:
          summary: Strix backup is overdue

      - alert: BackupSizeAnomaly
        expr: |
          abs(strix_backup_size_bytes - strix_backup_size_bytes offset 1d)
          / strix_backup_size_bytes > 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: Strix backup size changed significantly
```

## Best Practices

1. **3-2-1 Rule**: 3 copies, 2 different media types, 1 offsite
2. **Encryption**: Encrypt backups at rest and in transit
3. **Retention**: Keep daily backups for 7 days, weekly for 4 weeks, monthly for 12 months
4. **Testing**: Test restore procedures quarterly
5. **Documentation**: Keep recovery procedures updated
6. **Automation**: Use cron/systemd timers for regular backups
