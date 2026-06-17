#!/usr/bin/env bash
# Backup and restore rehearsal script
#
# Starts Strix, seeds data, performs a hot backup, destroys originals,
# restores from backup, and verifies all data survived the round-trip.
#
# Requires: sqlite3, strix binary
#
# Usage:
#   ./scripts/backup-restore-test.sh /path/to/strix-binary

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

BINARY="${1:?Usage: $0 <strix-binary-path>}"
if [[ ! -x "$BINARY" ]]; then
    echo -e "${RED}Error: $BINARY is not executable${NC}"
    exit 1
fi

# Check for sqlite3
if ! command -v sqlite3 >/dev/null 2>&1; then
    echo -e "${RED}Error: sqlite3 is required but not found${NC}"
    exit 1
fi

S3_PORT=18000
ADMIN_PORT=18001
METRICS_PORT=18090
ROOT_USER="backupadmin"
ROOT_PASS="backuppass12345678"
DATA_DIR=$(mktemp -d)
BACKUP_DIR=$(mktemp -d)
PID=""
TOKEN=""

TEST_BUCKET="backup-test-bucket"
TEST_USER="backup-testuser"
TEST_OBJECTS=("doc/readme.txt" "images/photo.png" "data/report.csv")
TEST_CONTENT_PREFIX="backup-test-content"

pass() { echo -e "${GREEN}OK${NC}"; }
fail() { echo -e "${RED}FAILED${NC}: $1"; exit 1; }
warn() { echo -e "${YELLOW}WARN${NC}: $1"; }

stop_server() {
    if [[ -n "$PID" ]]; then
        kill "$PID" 2>/dev/null || true
        wait "$PID" 2>/dev/null || true
        PID=""
    fi
}

cleanup() {
    stop_server
    rm -rf "$DATA_DIR" "$BACKUP_DIR"
}
trap cleanup EXIT

start_server() {
    local data_dir="$1"
    "$BINARY" \
        --address "127.0.0.1:$S3_PORT" \
        --console-address "127.0.0.1:$ADMIN_PORT" \
        --metrics-address "127.0.0.1:$METRICS_PORT" \
        --data-dir "$data_dir" \
        --root-user "$ROOT_USER" \
        --root-password "$ROOT_PASS" \
        --log-level warn &
    PID=$!

    # Wait for readiness
    for _ in $(seq 1 30); do
        if curl -sf "http://127.0.0.1:$ADMIN_PORT/health/ready" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    fail "Server did not become ready within 30s"
}

login() {
    local resp
    resp=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/login" \
        -H "Content-Type: application/json" \
        -d "{\"accessKey\":\"$ROOT_USER\",\"secretKey\":\"$ROOT_PASS\"}" 2>/dev/null || echo "")
    if command -v jq >/dev/null 2>&1; then
        TOKEN=$(echo "$resp" | jq -r '.token // empty')
    else
        TOKEN=$(echo "$resp" | grep -oP '"token"\s*:\s*"[^"]+"' | head -1 | sed 's/"token"\s*:\s*"//' | sed 's/"$//')
    fi
    if [[ -z "$TOKEN" ]]; then
        fail "Login failed"
    fi
}

presign_url() {
    local bucket="$1" key="$2" method="$3"
    local resp
    resp=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/presign" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"bucket\":\"$bucket\",\"key\":\"$key\",\"method\":\"$method\",\"expires_in\":300}" \
        2>/dev/null || echo "")
    if command -v jq >/dev/null 2>&1; then
        echo "$resp" | jq -r '.url // empty'
    else
        echo "$resp" | grep -oP '"url"\s*:\s*"[^"]+"' | sed 's/"url"\s*:\s*"//' | sed 's/"$//' | head -1
    fi
}

echo "========================================"
echo "Strix Backup/Restore Rehearsal"
echo "========================================"
echo "Binary:  $BINARY"
echo "Data:    $DATA_DIR"
echo "Backup:  $BACKUP_DIR"
echo "Ports:   S3=$S3_PORT Admin=$ADMIN_PORT"
echo ""

# === Phase 1: Start server and seed data ===
echo "--- Phase 1: Seed data ---"

echo -n "1. Start server... "
start_server "$DATA_DIR"
pass

echo -n "2. Login... "
login
pass

echo -n "3. Create bucket... "
BUCKET_RESP=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/buckets" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$TEST_BUCKET\"}" 2>/dev/null || echo "")
if ! echo "$BUCKET_RESP" | grep -q "$TEST_BUCKET"; then
    fail "Create bucket failed"
fi
pass

echo -n "4. Upload 3 test objects... "
for i in "${!TEST_OBJECTS[@]}"; do
    key="${TEST_OBJECTS[$i]}"
    content="${TEST_CONTENT_PREFIX}-${i}-$(date +%s%N)"
    # Store content for later verification
    echo "$content" > "$BACKUP_DIR/expected-$i.txt"

    put_url=$(presign_url "$TEST_BUCKET" "$key" "PUT")
    if [[ -z "$put_url" ]]; then
        fail "Presign PUT failed for $key"
    fi
    status=$(curl -sf -o /dev/null -w "%{http_code}" -X PUT "$put_url" \
        -H "Content-Type: text/plain" \
        --data-raw "$content" 2>/dev/null || echo "000")
    if [[ "$status" != "200" ]]; then
        fail "PUT $key returned HTTP $status"
    fi
done
pass

echo -n "5. Create IAM user... "
USER_RESP=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/users" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$TEST_USER\"}" 2>/dev/null || echo "")
if ! echo "$USER_RESP" | grep -q "$TEST_USER"; then
    fail "Create user failed"
fi
pass

# === Phase 2: Hot backup ===
echo ""
echo "--- Phase 2: Hot backup ---"

echo -n "6. Stop server... "
stop_server
pass

echo -n "7. Backup databases (sqlite3 .backup)... "
mkdir -p "$BACKUP_DIR/meta"
sqlite3 "$DATA_DIR/meta/strix.db" ".backup '$BACKUP_DIR/meta/strix.db'"
sqlite3 "$DATA_DIR/meta/iam.db" ".backup '$BACKUP_DIR/meta/iam.db'"
pass

echo -n "8. Backup encryption key + objects... "
cp "$DATA_DIR/meta/encryption.key" "$BACKUP_DIR/meta/encryption.key"
cp -a "$DATA_DIR/objects" "$BACKUP_DIR/objects"
pass

echo -n "9. Verify backup integrity... "
STRIX_CHECK=$(sqlite3 "$BACKUP_DIR/meta/strix.db" "PRAGMA integrity_check;" 2>&1)
IAM_CHECK=$(sqlite3 "$BACKUP_DIR/meta/iam.db" "PRAGMA integrity_check;" 2>&1)
if [[ "$STRIX_CHECK" != "ok" ]]; then
    fail "strix.db integrity check failed: $STRIX_CHECK"
fi
if [[ "$IAM_CHECK" != "ok" ]]; then
    fail "iam.db integrity check failed: $IAM_CHECK"
fi
pass

# === Phase 3: Destroy and restore ===
echo ""
echo "--- Phase 3: Destroy and restore ---"

echo -n "10. Destroy original data... "
rm -rf "$DATA_DIR"
if [[ -d "$DATA_DIR" ]]; then
    fail "Data directory still exists after rm"
fi
pass

echo -n "11. Restore from backup... "
mkdir -p "$DATA_DIR/meta" "$DATA_DIR/tmp" "$DATA_DIR/multipart"
cp "$BACKUP_DIR/meta/strix.db" "$DATA_DIR/meta/strix.db"
cp "$BACKUP_DIR/meta/iam.db" "$DATA_DIR/meta/iam.db"
cp "$BACKUP_DIR/meta/encryption.key" "$DATA_DIR/meta/encryption.key"
cp -a "$BACKUP_DIR/objects" "$DATA_DIR/objects"
pass

# === Phase 4: Verify restored data ===
echo ""
echo "--- Phase 4: Verify restored data ---"

echo -n "12. Start server on restored data... "
start_server "$DATA_DIR"
pass

echo -n "13. Login to restored server... "
login
pass

echo -n "14. Verify bucket exists... "
BUCKETS_RESP=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/api/v1/buckets" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "")
if ! echo "$BUCKETS_RESP" | grep -q "$TEST_BUCKET"; then
    fail "Bucket $TEST_BUCKET not found after restore"
fi
pass

echo -n "15. Verify all 3 objects (content round-trip)... "
for i in "${!TEST_OBJECTS[@]}"; do
    key="${TEST_OBJECTS[$i]}"
    expected=$(cat "$BACKUP_DIR/expected-$i.txt")

    get_url=$(presign_url "$TEST_BUCKET" "$key" "GET")
    if [[ -z "$get_url" ]]; then
        fail "Presign GET failed for $key"
    fi
    got=$(curl -sf "$get_url" 2>/dev/null || echo "")
    if [[ "$got" != "$expected" ]]; then
        fail "Content mismatch for $key"
    fi
done
pass

echo -n "16. Verify IAM user exists... "
USERS_RESP=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/api/v1/users" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "")
if ! echo "$USERS_RESP" | grep -q "$TEST_USER"; then
    fail "User $TEST_USER not found after restore"
fi
pass

echo ""
echo -e "${GREEN}========================================"
echo "Backup/restore rehearsal passed! (16/16)"
echo -e "========================================${NC}"
