#!/usr/bin/env bash
# Validate advisory tracking consistency
#
# Ensures deny.toml (source of truth), SECURITY.md, and any workflow
# references stay in sync. Warns on expired revisit dates.
#
# Usage: ./scripts/check-advisories.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

FAILURES=0

fail() {
    echo -e "${RED}FAIL${NC}: $1"
    FAILURES=$((FAILURES + 1))
}

warn() {
    echo -e "${YELLOW}WARN${NC}: $1"
}

# Extract advisory IDs from deny.toml (source of truth)
DENY_IDS=$(grep -oP 'RUSTSEC-\d+-\d+' "$PROJECT_ROOT/deny.toml" | sort -u)

if [[ -z "$DENY_IDS" ]]; then
    echo -e "${GREEN}No advisories in deny.toml — nothing to check${NC}"
    exit 0
fi

echo "Advisories in deny.toml:"
echo "$DENY_IDS" | sed 's/^/  /'
echo ""

# Check each advisory appears in SECURITY.md
echo "Checking SECURITY.md..."
while IFS= read -r id; do
    if ! grep -q "$id" "$PROJECT_ROOT/SECURITY.md"; then
        fail "$id missing from SECURITY.md"
    fi
done <<< "$DENY_IDS"

# Check revisit dates haven't expired
echo "Checking revisit dates..."
TODAY=$(date +%Y-%m-%d)
while IFS= read -r line; do
    REVISIT=$(echo "$line" | grep -oP '\d{4}-\d{2}-\d{2}')
    ADVISORY=$(grep -B5 "$line" "$PROJECT_ROOT/deny.toml" | grep -oP 'RUSTSEC-\d+-\d+' | tail -1)
    if [[ -n "$REVISIT" && "$REVISIT" < "$TODAY" ]]; then
        warn "$ADVISORY revisit date $REVISIT has passed — review needed"
    fi
done < <(grep '# Revisit:' "$PROJECT_ROOT/deny.toml")

if [[ $FAILURES -gt 0 ]]; then
    echo ""
    echo -e "${RED}$FAILURES check(s) failed${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}All advisory checks passed${NC}"
