#!/usr/bin/env bash
#
# Strix Operator Smoke Test
#
# Validates the complete single-node operator workflow:
# - Server connectivity
# - User/key/policy management via sx CLI
# - Bucket operations
# - Object put/get
#
# Prerequisites:
# - Strix server running on localhost:9000 (S3) and localhost:9001 (admin)
# - sx CLI built and in PATH (or ../target/release/sx)
# - STRIX_ROOT_USER and STRIX_ROOT_PASSWORD set
#
# Usage:
#   ./scripts/smoke-test.sh
#
# Environment:
#   STRIX_ENDPOINT     S3 endpoint (default: http://localhost:9000)
#   STRIX_ADMIN        Admin endpoint (default: http://localhost:9001)
#   STRIX_ROOT_USER    Root username (required)
#   STRIX_ROOT_PASSWORD Root password (required)
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Config
ENDPOINT="${STRIX_ENDPOINT:-http://localhost:9000}"
ADMIN="${STRIX_ADMIN:-http://localhost:9001}"
ROOT_USER="${STRIX_ROOT_USER:-}"
ROOT_PASSWORD="${STRIX_ROOT_PASSWORD:-}"

# Find sx binary
SX="${SX_BIN:-}"
if [[ -z "$SX" ]]; then
    if command -v sx &>/dev/null; then
        SX="sx"
    elif [[ -x "./target/release/sx" ]]; then
        SX="./target/release/sx"
    elif [[ -x "./target/debug/sx" ]]; then
        SX="./target/debug/sx"
    else
        echo -e "${RED}Error: sx binary not found. Build with 'cargo build --release' or set SX_BIN${NC}"
        exit 1
    fi
fi

# Validate required env
if [[ -z "$ROOT_USER" || -z "$ROOT_PASSWORD" ]]; then
    echo -e "${RED}Error: STRIX_ROOT_USER and STRIX_ROOT_PASSWORD must be set${NC}"
    exit 1
fi

# Test state
TEST_ALIAS="smoke-test-$$"
TEST_USER="smokeuser$$"
TEST_BUCKET="smoke-bucket-$$"
TEST_FILE="/tmp/strix-smoke-$$.txt"
CLEANUP_NEEDED=false

cleanup() {
    if [[ "$CLEANUP_NEEDED" == "true" ]]; then
        echo -e "\n${YELLOW}Cleaning up...${NC}"
        # Remove test file
        rm -f "$TEST_FILE" "/tmp/strix-smoke-downloaded-$$.txt"
        # Try to clean up bucket and user (ignore errors)
        $SX rm "$TEST_ALIAS/$TEST_BUCKET/test.txt" 2>/dev/null || true
        $SX rb "$TEST_ALIAS/$TEST_BUCKET" 2>/dev/null || true
        $SX user remove "$TEST_ALIAS" "$TEST_USER" 2>/dev/null || true
        $SX alias remove "$TEST_ALIAS" 2>/dev/null || true
    fi
}
trap cleanup EXIT

pass() {
    echo -e "${GREEN}OK${NC}"
}

fail() {
    echo -e "${RED}FAILED${NC}"
    echo -e "${RED}Error: $1${NC}"
    exit 1
}

echo "========================================"
echo "Strix Operator Smoke Test"
echo "========================================"
echo "Endpoint: $ENDPOINT"
echo "Admin:    $ADMIN"
echo "Root:     $ROOT_USER"
echo "sx:       $SX"
echo ""

# 0. CLI doc drift check (doesn't need a running server)
if [[ "${SKIP_DOCS:-false}" != "true" ]]; then
    echo -n "0. CLI doc drift check... "
    if "$SCRIPT_DIR/check-cli-docs.sh" >/dev/null 2>&1; then
        pass
    else
        fail "Documented CLI commands out of sync (run scripts/check-cli-docs.sh for details)"
    fi
fi

# 1. Health check
echo -n "1. Health check... "
if curl -sf "$ADMIN/health/ready" >/dev/null 2>&1; then
    pass
else
    fail "Server not responding at $ADMIN/health/ready"
fi

# 2. Set alias
echo -n "2. Setting alias... "
CLEANUP_NEEDED=true
if $SX alias set "$TEST_ALIAS" "$ENDPOINT" "$ROOT_USER" "$ROOT_PASSWORD" >/dev/null 2>&1; then
    pass
else
    fail "Failed to set alias"
fi

# 3. List buckets (empty is OK)
echo -n "3. List buckets... "
if $SX ls "$TEST_ALIAS" >/dev/null 2>&1; then
    pass
else
    fail "Failed to list buckets"
fi

# 4. Create bucket
echo -n "4. Create bucket... "
if $SX mb "$TEST_ALIAS/$TEST_BUCKET" >/dev/null 2>&1; then
    pass
else
    fail "Failed to create bucket"
fi

# 5. Create test file
echo -n "5. Create test file... "
echo "Strix smoke test $(date -Iseconds)" > "$TEST_FILE"
pass

# 6. Upload object
echo -n "6. Upload object... "
if $SX cp "$TEST_FILE" "$TEST_ALIAS/$TEST_BUCKET/test.txt" >/dev/null 2>&1; then
    pass
else
    fail "Failed to upload object"
fi

# 7. List objects
echo -n "7. List objects... "
if $SX ls "$TEST_ALIAS/$TEST_BUCKET" 2>/dev/null | grep -q "test.txt"; then
    pass
else
    fail "Object not found in listing"
fi

# 8. Download object
echo -n "8. Download object... "
if $SX cp "$TEST_ALIAS/$TEST_BUCKET/test.txt" "/tmp/strix-smoke-downloaded-$$.txt" >/dev/null 2>&1; then
    pass
else
    fail "Failed to download object"
fi

# 9. Verify content
echo -n "9. Verify content... "
if cmp -s "$TEST_FILE" "/tmp/strix-smoke-downloaded-$$.txt"; then
    pass
else
    fail "Downloaded content does not match"
fi

# 10. Create user
echo -n "10. Create user... "
if $SX user add "$TEST_ALIAS" "$TEST_USER" >/dev/null 2>&1; then
    pass
else
    fail "Failed to create user"
fi

# 11. List users
echo -n "11. List users... "
if $SX user list "$TEST_ALIAS" 2>/dev/null | grep -q "$TEST_USER"; then
    pass
else
    fail "Created user not found in listing"
fi

# 12. Delete user
echo -n "12. Delete user... "
if $SX user remove "$TEST_ALIAS" "$TEST_USER" >/dev/null 2>&1; then
    pass
else
    fail "Failed to delete user"
fi

# 13. Delete object
echo -n "13. Delete object... "
if $SX rm "$TEST_ALIAS/$TEST_BUCKET/test.txt" >/dev/null 2>&1; then
    pass
else
    fail "Failed to delete object"
fi

# 14. Delete bucket
echo -n "14. Delete bucket... "
if $SX rb "$TEST_ALIAS/$TEST_BUCKET" >/dev/null 2>&1; then
    pass
else
    fail "Failed to delete bucket"
fi

# 15. Remove alias
echo -n "15. Remove alias... "
if $SX alias remove "$TEST_ALIAS" >/dev/null 2>&1; then
    pass
else
    fail "Failed to remove alias"
fi

CLEANUP_NEEDED=false

echo ""
echo -e "${GREEN}========================================"
echo "All smoke tests passed!"
echo -e "========================================${NC}"
