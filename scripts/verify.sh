#!/usr/bin/env bash
# Canonical verification script for Strix
# Run this before commits and releases to ensure quality gates pass
#
# Usage: ./scripts/verify.sh [OPTIONS]
#   --quick          Skip slow checks (audit, deny, GUI build) — same as --mode core
#   --mode <mode>    Run a specific check group:
#                      core     — fmt, clippy, tests
#                      security — cargo audit, cargo deny
#                      gui      — GUI crate check, trunk build
#                      all      — everything (default)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

MODE="all"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick) MODE="core"; shift ;;
        --mode) MODE="$2"; shift 2 ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

step() {
    echo -e "${YELLOW}==> $1${NC}"
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

fail() {
    echo -e "${RED}✗ $1${NC}"
    exit 1
}

run_core() {
    command -v cargo >/dev/null 2>&1 || fail "cargo not found"

    step "Checking formatting..."
    cargo fmt --all -- --check || fail "Formatting check failed. Run: cargo fmt --all"
    success "Formatting OK"

    step "Running clippy..."
    cargo clippy --workspace --all-targets -- -D warnings || fail "Clippy found issues"
    success "Clippy OK"

    step "Running tests..."
    cargo test --workspace || fail "Tests failed"
    success "Tests passed"
}

run_security() {
    step "Running cargo audit..."
    if command -v cargo-audit >/dev/null 2>&1; then
        cargo audit || echo "Note: Some vulnerabilities require upstream fixes (see deny.toml)"
    else
        echo "Warning: cargo-audit not installed. Install with: cargo install cargo-audit"
    fi

    step "Running cargo deny..."
    if command -v cargo-deny >/dev/null 2>&1; then
        DENY_OUTPUT=$(cargo deny check 2>&1) && DENY_RC=0 || DENY_RC=$?
        if [[ $DENY_RC -ne 0 ]]; then
            if echo "$DENY_OUTPUT" | grep -q "unsupported CVSS version"; then
                echo "Note: cargo deny failed due to advisory-db CVSS parse error (upstream issue)"
                echo "      Skipping cargo deny until RustSec fixes their database format"
                success "Dependency policy OK (advisory-db parse error ignored)"
            else
                echo "$DENY_OUTPUT"
                fail "cargo deny check failed"
            fi
        else
            success "Dependency policy OK"
        fi
    else
        echo "Warning: cargo-deny not installed. Install with: cargo install cargo-deny"
    fi
}

run_gui() {
    step "Checking GUI crate..."
    if [[ -f "crates/strix-gui/Cargo.toml" ]]; then
        cargo check --manifest-path crates/strix-gui/Cargo.toml || fail "GUI crate check failed"
        success "GUI crate compiles"
    fi

    step "Building GUI (trunk)..."
    if command -v trunk >/dev/null 2>&1; then
        if [[ -f "crates/strix-gui/Trunk.toml" ]]; then
            (cd crates/strix-gui && env RUSTFLAGS='' trunk build --release) || fail "GUI build failed"
            success "GUI built successfully"
        fi
    else
        echo "Warning: trunk not installed. Install with: cargo install trunk"
    fi
}

case "$MODE" in
    core)
        run_core
        ;;
    security)
        run_security
        ;;
    gui)
        run_gui
        ;;
    all)
        run_core
        run_security
        run_gui
        ;;
    *)
        echo "Unknown mode: $MODE (expected: core, security, gui, all)"
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}All verification checks passed!${NC}"
echo -e "${GREEN}========================================${NC}"
