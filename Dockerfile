# Strix - S3-compatible object storage server
#
# Multi-stage Dockerfile for building and running Strix
#
# Build: docker build -t strix .
# Run:   docker run -p 9000:9000 -p 9001:9001 -e STRIX_ROOT_USER=admin -e STRIX_ROOT_PASSWORD=password strix

# =============================================================================
# Stage 1: Build the GUI (WASM) - MUST BE FIRST
# =============================================================================
FROM rust:1.91.1-bookworm AS gui-builder

# Install Trunk for building Leptos WASM apps
RUN cargo install trunk
RUN rustup target add wasm32-unknown-unknown

WORKDIR /build

# Copy GUI source
COPY crates/strix-gui/ ./crates/strix-gui/
COPY crates/strix-core/ ./crates/strix-core/

# Build GUI
WORKDIR /build/crates/strix-gui
RUN trunk build --release

# Ensure static files are copied to dist (Trunk hook may not work in Docker)
RUN cp -r static/* dist/ 2>/dev/null || true

# =============================================================================
# Stage 2: Build the Rust binary (after GUI so it can embed fresh assets)
# =============================================================================
FROM rust:1.91.1-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY strix/ strix/
COPY crates/ crates/
COPY tests/ tests/

# Copy GUI dist from gui-builder so rust_embed can include it
COPY --from=gui-builder /build/crates/strix-gui/dist/ ./crates/strix-gui/dist/

# Build release binary (now with fresh GUI embedded)
RUN cargo build --release -p strix

# =============================================================================
# Stage 3: Runtime image
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -m strix

# Create data directory
RUN mkdir -p /var/lib/strix && chown strix:strix /var/lib/strix

# Copy binary from builder
COPY --from=builder /build/target/release/strix /usr/local/bin/strix

# Copy GUI assets from gui-builder
COPY --from=gui-builder /build/crates/strix-gui/dist/ /usr/local/share/strix/gui/

# Set environment variables
ENV STRIX_DATA_DIR=/var/lib/strix
ENV STRIX_ADDRESS=0.0.0.0:9000
ENV STRIX_CONSOLE_ADDRESS=0.0.0.0:9001
ENV STRIX_METRICS_ADDRESS=0.0.0.0:9090
ENV STRIX_LOG_LEVEL=info
ENV STRIX_GUI_PATH=/usr/local/share/strix/gui

# Expose ports
# 9000 - S3 API
# 9001 - Web Console & Admin API
# 9090 - Prometheus Metrics
EXPOSE 9000 9001 9090

# Switch to non-root user
USER strix

# Data volume
VOLUME ["/var/lib/strix"]

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9001/api/v1/health || exit 1

# Run the server
ENTRYPOINT ["/usr/local/bin/strix"]
