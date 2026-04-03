# Contributing to Strix

Thank you for your interest in contributing to Strix! This document provides guidelines and information about contributing to the project.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for everyone.

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)
- Trunk (for GUI development): `cargo install trunk`
- wasm32 target: `rustup target add wasm32-unknown-unknown`

### Development Setup

```bash
# Clone the repository
git clone https://github.com/ghostkellz/strix.git
cd strix

# Build all crates
cargo build

# Run tests
cargo test --workspace

# Run the server in development mode
STRIX_ROOT_USER=admin STRIX_ROOT_PASSWORD=admin123 \
  cargo run -p strix -- --data-dir ./dev-data --log-level debug
```

### Building the GUI

```bash
cd crates/strix-gui
trunk serve  # Development server with hot reload
trunk build --release  # Production build
```

## Development Guidelines

### Code Style

- Follow the official [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/)
- Run `cargo fmt --all` before committing
- Ensure `cargo clippy --workspace -- -D warnings` passes
- No `unsafe` code unless absolutely necessary and well-documented

### Commit Messages

Use clear, descriptive commit messages:

```
feat: add bucket versioning support
fix: correct signature validation for chunked uploads
docs: update API reference for multipart operations
refactor: simplify IAM policy evaluation logic
test: add integration tests for CORS configuration
```

### Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Ensure all tests pass: `cargo test --workspace`
5. Run formatting: `cargo fmt --all`
6. Run lints: `cargo clippy --workspace -- -D warnings`
7. Push your branch and create a pull request

### Testing

- Write unit tests for new functionality
- Ensure integration tests pass
- Test with AWS CLI and common S3 clients when adding S3 operations

```bash
# Run unit tests
cargo test --workspace

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

## Project Structure

```
strix/
├── strix/                 # Main binary
├── crates/
│   ├── strix-core/        # Shared types, traits, errors
│   ├── strix-s3/          # S3 API implementation
│   ├── strix-storage/     # Storage backend
│   ├── strix-iam/         # IAM (users, policies, keys)
│   ├── strix-crypto/      # Cryptographic operations
│   ├── strix-admin/       # Admin REST API
│   ├── strix-gui/         # Leptos web console
│   └── strix-cli/         # CLI tool (sx)
├── tests/
│   └── integration/       # Integration tests
└── docs/                  # Documentation
```

## Areas for Contribution

### Good First Issues

Look for issues labeled `good-first-issue` for beginner-friendly tasks.

### Feature Development

- Object locking (WORM compliance)
- Lifecycle rules
- Event notifications (webhooks, SQS, etc.)
- LDAP/OIDC authentication
- Distributed mode with erasure coding

### Documentation

- API documentation improvements
- Tutorial and how-to guides
- Architecture documentation

### Testing

- Additional unit test coverage
- S3 compatibility tests
- Performance benchmarks

## Releasing

Releases are managed through GitHub Actions. To create a release:

1. Update version in `Cargo.toml` files
2. Update `CHANGELOG.md`
3. Create and push a tag: `git tag v0.1.0 && git push origin v0.1.0`
4. GitHub Actions will build and publish the release

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions or ideas

Thank you for contributing to Strix!
