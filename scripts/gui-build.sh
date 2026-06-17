#!/usr/bin/env bash
#
# Build the Strix GUI (Leptos/WASM) for embedding in the main binary.
#
# This script ensures reproducible builds by:
# - Clearing RUSTFLAGS that might interfere with WASM compilation
# - Using the workspace's pinned toolchain
# - Building in release mode for optimized WASM output
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
GUI_DIR="$PROJECT_ROOT/crates/strix-gui"

echo "Building Strix GUI..."
echo "  Project root: $PROJECT_ROOT"
echo "  GUI directory: $GUI_DIR"

# Check prerequisites
if ! command -v trunk &> /dev/null; then
    echo "Error: trunk is not installed."
    echo "Install it with: cargo install trunk"
    exit 1
fi

if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
    echo "Error: wasm32-unknown-unknown target not installed."
    echo "Install it with: rustup target add wasm32-unknown-unknown"
    exit 1
fi

# Clear RUSTFLAGS to avoid host-specific flags affecting WASM compilation
# This is critical for reproducible builds
unset RUSTFLAGS

cd "$GUI_DIR"

# Clean previous build artifacts
rm -rf dist/

# Build with Trunk
echo "Running: trunk build --release"
trunk build --release

# Verify build succeeded
if [ ! -d "dist" ] || [ -z "$(ls -A dist 2>/dev/null)" ]; then
    echo "Error: GUI build failed - dist directory is empty or missing"
    exit 1
fi

# Count files
FILE_COUNT=$(find dist -type f | wc -l)
echo ""
echo "GUI build complete!"
echo "  Output: $GUI_DIR/dist/"
echo "  Files: $FILE_COUNT"
echo ""
echo "To embed in the main binary, rebuild strix:"
echo "  cargo build --release -p strix"
