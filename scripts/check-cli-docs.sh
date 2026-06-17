#!/usr/bin/env bash
# CLI documentation drift detection
#
# Parses documented sx subcommands from docs/reference/cli.md and verifies
# each exists by running `sx <cmd> --help`. Catches renamed, removed, or
# broken commands before they reach users.
#
# Usage: ./scripts/check-cli-docs.sh
#
# Environment:
#   SX_BIN   Path to sx binary (default: auto-detect)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CLI_DOC="$PROJECT_ROOT/docs/reference/cli.md"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

# Find sx binary
SX="${SX_BIN:-}"
if [[ -z "$SX" ]]; then
    if command -v sx &>/dev/null; then
        SX="sx"
    elif [[ -x "$PROJECT_ROOT/target/release/sx" ]]; then
        SX="$PROJECT_ROOT/target/release/sx"
    elif [[ -x "$PROJECT_ROOT/target/debug/sx" ]]; then
        SX="$PROJECT_ROOT/target/debug/sx"
    else
        echo -e "${RED}Error: sx binary not found. Build with 'cargo build -p strix-cli' or set SX_BIN${NC}"
        exit 1
    fi
fi

if [[ ! -f "$CLI_DOC" ]]; then
    echo -e "${RED}Error: $CLI_DOC not found${NC}"
    exit 1
fi

# Extract sx subcommand patterns from code blocks in cli.md
# Matches lines like: sx alias set, sx ls, sx mb, sx user list
# Filters out arguments (anything starting with -, ., /, $, ", ', or containing =)
DOCUMENTED_CMDS=$(
    grep -oP '(?<=^sx )\S+(\s+\S+)?' "$CLI_DOC" |
    sed 's/ local.*//; s/ \S*[\/\.\$"'\''=\-].*//; s/ [A-Z].*//; s/ [a-z]*\.txt.*//; s/ [a-z]*\.json.*//; s/ [\{].*//; s/ *$//' |
    grep -v '^$' |
    sort -u
)

echo "Checking $(echo "$DOCUMENTED_CMDS" | wc -l) documented sx commands against $SX"
echo ""

FAILURES=0
PASSED=0

while IFS= read -r cmd; do
    # shellcheck disable=SC2086
    if $SX $cmd --help >/dev/null 2>&1; then
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}FAIL${NC}: sx $cmd"
        FAILURES=$((FAILURES + 1))
    fi
done <<< "$DOCUMENTED_CMDS"

echo ""
if [[ $FAILURES -gt 0 ]]; then
    echo -e "${RED}$FAILURES documented command(s) failed --help check${NC}"
    echo "Either fix the CLI or update docs/reference/cli.md"
    exit 1
fi

echo -e "${GREEN}All $PASSED documented commands verified${NC}"
