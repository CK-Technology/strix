# Strix - S3-compatible object storage server
#
# Multi-stage build with GUI compilation for a fully functional image.
#
# Build: docker build -f deploy/Dockerfile -t strix .
# Run:   docker run -p 9000:9000 -p 9001:9001 -e STRIX_ROOT_USER=admin -e STRIX_ROOT_PASSWORD=password123 strix

# =============================================================================
# Stage 1: Build the GUI (WASM)
# =============================================================================
FROM rust:1.96-bookworm AS gui-builder

RUN cargo install trunk
RUN rustup target add wasm32-unknown-unknown

WORKDIR /build

COPY crates/strix-gui/ ./crates/strix-gui/
COPY crates/strix-core/ ./crates/strix-core/

WORKDIR /build/crates/strix-gui
RUN env RUSTFLAGS='' trunk build --release

# Ensure static files are copied to dist (Trunk hook may not run in Docker)
RUN cp -r static/* dist/ 2>/dev/null || true

# Verify GUI build produced real assets
RUN test -f dist/index.html && ls dist/*.wasm >/dev/null 2>&1 \
    || (echo "ERROR: GUI build did not produce expected assets" && exit 1)

# =============================================================================
# Stage 2: Build the Rust binary (embeds GUI assets via rust-embed)
# =============================================================================
FROM rust:1.96-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY strix/ ./strix/
COPY crates/ ./crates/
COPY tests/ ./tests/

# Copy GUI dist from gui-builder so rust-embed includes real assets
COPY --from=gui-builder /build/crates/strix-gui/dist/ ./crates/strix-gui/dist/

RUN cargo build --release -p strix

# =============================================================================
# Stage 3: Runtime image
# =============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false strix

# Copy binary from builder
COPY --from=builder /build/target/release/strix /usr/local/bin/strix

# Create data directory
RUN mkdir -p /var/lib/strix && chown strix:strix /var/lib/strix

# Switch to non-root user
USER strix

# Expose ports
# 9000 - S3 API
# 9001 - Admin console + Web GUI
EXPOSE 9000 9001

# Data volume
VOLUME /var/lib/strix

# Environment defaults
ENV STRIX_DATA_DIR=/var/lib/strix
ENV STRIX_ADDRESS=0.0.0.0:9000
ENV STRIX_CONSOLE_ADDRESS=0.0.0.0:9001

# Health check — readiness probe verifies storage + DB connectivity
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9000/health/ready || exit 1

ENTRYPOINT ["strix"]
