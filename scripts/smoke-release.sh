#!/usr/bin/env bash
# Release artifact smoke test
#
# Boots a Strix binary or container, waits for readiness, then validates:
# - Admin login returns a JWT
# - Console serves real GUI assets (not placeholder)
# - S3 readiness endpoint responds
# - Console API returns server info and user list
# - SPA routing serves index.html for app routes
# - S3 object round-trip (create bucket, PUT object, GET object, verify content)
# - IAM user creation
# - Metrics endpoint (if reachable)
#
# Usage:
#   ./scripts/smoke-release.sh /path/to/strix-binary
#   ./scripts/smoke-release.sh --container <image>

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

CONTAINER_MODE=false
BINARY=""
IMAGE=""

if [[ "${1:-}" == "--container" ]]; then
    CONTAINER_MODE=true
    IMAGE="${2:?Usage: $0 --container <image>}"
elif [[ -n "${1:-}" ]]; then
    BINARY="$1"
    if [[ ! -x "$BINARY" ]]; then
        echo -e "${RED}Error: $BINARY is not executable${NC}"
        exit 1
    fi
else
    echo "Usage: $0 <binary-path> | --container <image>"
    exit 1
fi

S3_PORT=19000
ADMIN_PORT=19001
METRICS_PORT=19090
ROOT_USER="smokeadmin"
ROOT_PASS="smokepass12345678"
DATA_DIR=$(mktemp -d)
PID=""
CONTAINER_NAME="strix-smoke-$$"
TEST_BUCKET="smoke-test-bucket"
TEST_KEY="smoke-test-object.txt"
TEST_CONTENT="strix-smoke-test-$(date +%s)"
TEST_USER="smoke-testuser"

pass() { echo -e "${GREEN}OK${NC}"; }
fail() { echo -e "${RED}FAILED${NC}: $1"; exit 1; }

cleanup() {
    if [[ -n "$PID" ]]; then
        kill "$PID" 2>/dev/null || true
        wait "$PID" 2>/dev/null || true
    fi
    if [[ "$CONTAINER_MODE" == "true" ]]; then
        docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
    fi
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

echo "========================================"
echo "Strix Release Smoke Test"
echo "========================================"

# Boot the server
if [[ "$CONTAINER_MODE" == "true" ]]; then
    echo "Image: $IMAGE"
    echo "Mode:  container"
    docker run -d --name "$CONTAINER_NAME" \
        -p "$S3_PORT:9000" -p "$ADMIN_PORT:9001" -p "$METRICS_PORT:9090" \
        -e "STRIX_ROOT_USER=$ROOT_USER" \
        -e "STRIX_ROOT_PASSWORD=$ROOT_PASS" \
        "$IMAGE" >/dev/null
else
    echo "Binary: $BINARY"
    echo "Mode:   direct"
    "$BINARY" \
        --address "127.0.0.1:$S3_PORT" \
        --console-address "127.0.0.1:$ADMIN_PORT" \
        --metrics-address "127.0.0.1:$METRICS_PORT" \
        --data-dir "$DATA_DIR" \
        --root-user "$ROOT_USER" \
        --root-password "$ROOT_PASS" &
    PID=$!
fi

echo "Ports:  S3=$S3_PORT Admin=$ADMIN_PORT Metrics=$METRICS_PORT"
echo ""

# Wait for readiness (up to 30s)
echo -n "1. Waiting for readiness... "
READY=false
for _ in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:$ADMIN_PORT/health/ready" >/dev/null 2>&1; then
        READY=true
        break
    fi
    sleep 1
done
if [[ "$READY" != "true" ]]; then
    fail "Server did not become ready within 30s"
fi
pass

# Test admin login
echo -n "2. Admin login... "
LOGIN_RESP=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/login" \
    -H "Content-Type: application/json" \
    -d "{\"accessKey\":\"$ROOT_USER\",\"secretKey\":\"$ROOT_PASS\"}" 2>/dev/null || echo "")
if [[ -z "$LOGIN_RESP" ]]; then
    fail "Login request failed (no response)"
fi
# Check for token in response (handles both jq-available and jq-missing cases)
if command -v jq >/dev/null 2>&1; then
    TOKEN=$(echo "$LOGIN_RESP" | jq -r '.token // empty')
else
    TOKEN=$(echo "$LOGIN_RESP" | grep -oP '"token"\s*:\s*"[^"]+"' | head -1)
fi
if [[ -z "$TOKEN" ]]; then
    fail "Login did not return a token"
fi
pass

# Test console serves real GUI
echo -n "3. Console GUI assets... "
CONSOLE_HTML=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/" 2>/dev/null || echo "")
if [[ -z "$CONSOLE_HTML" ]]; then
    fail "Console returned empty response"
fi
if echo "$CONSOLE_HTML" | grep -qi "not built yet"; then
    fail "Console serving placeholder HTML — GUI assets not embedded"
fi
if ! echo "$CONSOLE_HTML" | grep -qi "wasm\|\.js"; then
    fail "Console HTML does not reference WASM or JS modules"
fi
pass

# Test S3 readiness
echo -n "4. S3 readiness... "
if ! curl -sf "http://127.0.0.1:$S3_PORT/health/ready" >/dev/null 2>&1; then
    fail "S3 readiness endpoint not responding"
fi
pass

# Test console API: server info
echo -n "5. Console API /info... "
INFO_RESP=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/api/v1/info" 2>/dev/null || echo "")
if [[ -z "$INFO_RESP" ]]; then
    fail "Info endpoint returned empty response"
fi
if command -v jq >/dev/null 2>&1; then
    INFO_VERSION=$(echo "$INFO_RESP" | jq -r '.version // empty')
else
    INFO_VERSION=$(echo "$INFO_RESP" | grep -oP '"version"\s*:\s*"[^"]+"' | head -1)
fi
if [[ -z "$INFO_VERSION" ]]; then
    fail "Info response missing version field"
fi
pass

# Test console API: list users (authenticated)
echo -n "6. Console API /users... "
USERS_RESP=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/api/v1/users" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "")
if [[ -z "$USERS_RESP" ]]; then
    fail "Users endpoint returned empty response"
fi
if ! echo "$USERS_RESP" | grep -q "items"; then
    fail "Users response missing items field"
fi
pass

# Test SPA routing: app route returns index.html, not 404
echo -n "7. SPA routing (/buckets)... "
SPA_HTML=$(curl -sf "http://127.0.0.1:$ADMIN_PORT/buckets" 2>/dev/null || echo "")
if [[ -z "$SPA_HTML" ]]; then
    fail "SPA route returned empty response"
fi
if echo "$SPA_HTML" | grep -qi "not built yet"; then
    fail "SPA route returning placeholder instead of index.html"
fi
pass

# Test: create bucket via admin API
echo -n "8. Create test bucket... "
BUCKET_RESP=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/buckets" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$TEST_BUCKET\"}" 2>/dev/null || echo "")
if [[ -z "$BUCKET_RESP" ]]; then
    fail "Create bucket returned empty response"
fi
if ! echo "$BUCKET_RESP" | grep -q "$TEST_BUCKET"; then
    fail "Create bucket response does not contain bucket name"
fi
pass

# Test: PUT object via presigned URL
echo -n "9. PUT object (presigned)... "
PUT_PRESIGN=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/presign" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"bucket\":\"$TEST_BUCKET\",\"key\":\"$TEST_KEY\",\"method\":\"PUT\",\"expires_in\":300}" \
    2>/dev/null || echo "")
if [[ -z "$PUT_PRESIGN" ]]; then
    fail "Presign PUT returned empty response"
fi
if command -v jq >/dev/null 2>&1; then
    PUT_URL=$(echo "$PUT_PRESIGN" | jq -r '.url // empty')
else
    PUT_URL=$(echo "$PUT_PRESIGN" | grep -oP '"url"\s*:\s*"[^"]+"' | sed 's/"url"\s*:\s*"//' | sed 's/"$//' | head -1)
fi
if [[ -z "$PUT_URL" ]]; then
    fail "Presign PUT response missing url"
fi
PUT_STATUS=$(curl -sf -o /dev/null -w "%{http_code}" -X PUT "$PUT_URL" \
    -H "Content-Type: text/plain" \
    --data-raw "$TEST_CONTENT" 2>/dev/null || echo "000")
if [[ "$PUT_STATUS" != "200" ]]; then
    fail "PUT object returned HTTP $PUT_STATUS (expected 200)"
fi
pass

# Test: GET object via presigned URL, verify content
echo -n "10. GET object (presigned)... "
GET_PRESIGN=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/presign" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"bucket\":\"$TEST_BUCKET\",\"key\":\"$TEST_KEY\",\"method\":\"GET\",\"expires_in\":300}" \
    2>/dev/null || echo "")
if command -v jq >/dev/null 2>&1; then
    GET_URL=$(echo "$GET_PRESIGN" | jq -r '.url // empty')
else
    GET_URL=$(echo "$GET_PRESIGN" | grep -oP '"url"\s*:\s*"[^"]+"' | sed 's/"url"\s*:\s*"//' | sed 's/"$//' | head -1)
fi
if [[ -z "$GET_URL" ]]; then
    fail "Presign GET response missing url"
fi
GOT_CONTENT=$(curl -sf "$GET_URL" 2>/dev/null || echo "")
if [[ "$GOT_CONTENT" != "$TEST_CONTENT" ]]; then
    fail "Object content mismatch (expected '$TEST_CONTENT', got '$GOT_CONTENT')"
fi
pass

# Test: create IAM user
echo -n "11. Create IAM user... "
USER_RESP=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/users" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$TEST_USER\"}" 2>/dev/null || echo "")
if [[ -z "$USER_RESP" ]]; then
    fail "Create user returned empty response"
fi
if ! echo "$USER_RESP" | grep -q "$TEST_USER"; then
    fail "Create user response does not contain username"
fi
pass

# Test: metrics endpoint
echo -n "12. Metrics endpoint... "
METRICS_RESP=$(curl -sf "http://127.0.0.1:$METRICS_PORT/metrics" 2>/dev/null || echo "")
if [[ -z "$METRICS_RESP" ]]; then
    fail "Metrics endpoint returned empty response"
fi
if ! echo "$METRICS_RESP" | grep -q "strix_"; then
    fail "Metrics response missing strix_ prefix metrics"
fi
pass

# Cleanup: delete test resources
echo -n "13. Cleanup test resources... "
# Delete the test object
DEL_PRESIGN=$(curl -sf -X POST "http://127.0.0.1:$ADMIN_PORT/api/v1/presign" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"bucket\":\"$TEST_BUCKET\",\"key\":\"$TEST_KEY\",\"method\":\"DELETE\",\"expires_in\":300}" \
    2>/dev/null || echo "")
if [[ -n "$DEL_PRESIGN" ]]; then
    if command -v jq >/dev/null 2>&1; then
        DEL_URL=$(echo "$DEL_PRESIGN" | jq -r '.url // empty')
    else
        DEL_URL=$(echo "$DEL_PRESIGN" | grep -oP '"url"\s*:\s*"[^"]+"' | sed 's/"url"\s*:\s*"//' | sed 's/"$//' | head -1)
    fi
    [[ -n "$DEL_URL" ]] && curl -sf -X DELETE "$DEL_URL" >/dev/null 2>&1 || true
fi
# Delete the test bucket
curl -sf -X DELETE "http://127.0.0.1:$ADMIN_PORT/api/v1/buckets/$TEST_BUCKET" \
    -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 || true
# Delete the test user
curl -sf -X DELETE "http://127.0.0.1:$ADMIN_PORT/api/v1/users/$TEST_USER" \
    -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 || true
pass

echo ""
echo -e "${GREEN}========================================"
echo "All release smoke tests passed! (13/13)"
echo -e "========================================${NC}"
