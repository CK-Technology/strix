# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability in Strix, please report it responsibly:

1. **Do not** open a public GitHub issue for security vulnerabilities
2. Email security concerns to the maintainers (see CONTRIBUTING.md for contact)
3. Include detailed steps to reproduce the issue
4. Allow reasonable time for a fix before public disclosure

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Security Practices

### Authentication
- AWS Signature v4 for S3 API requests
- JWT tokens for Admin API with configurable expiration
- Rate limiting on login endpoints (5 attempts/minute, 15-minute lockout)

### Credentials Storage
- Access key secrets encrypted at rest (AES-256-GCM)
- STS session tokens stored as SHA-256 hash only (never plaintext)
- Root credentials required via environment variables (not stored in DB)

### Authorization
- IAM policy evaluation for all S3 operations
- Bucket policies with principal/action/resource matching
- STS temporary credentials enforce `X-Amz-Security-Token` header validation

### Data Protection
- Server-side encryption (SSE-S3 with AES-256-GCM)
- Customer-provided keys (SSE-C) support
- Object Lock for WORM compliance

## Dependency Auditing

We use `cargo audit` to check for known vulnerabilities in dependencies.

### Running an Audit

```bash
# Install cargo-audit (requires 0.22.0+ for CVSS 4.0 support)
cargo install cargo-audit

# Run audit
cargo audit
```

### v0.1.0 Advisory Status

Audit performed: 2026-04-03
Tool version: cargo-audit 0.22.1
Result: **0 vulnerabilities**, 3 warnings

#### Accepted Warnings

The following advisory warnings are accepted for v0.1.0. They are transitive dependencies with low practical risk for this release:

| Advisory | Crate | Severity | Source | Disposition |
|----------|-------|----------|--------|-------------|
| RUSTSEC-2025-0119 | `number_prefix` | Warning (unmaintained) | `indicatif` -> `strix-cli` | Accept for v0.1.0 |
| RUSTSEC-2025-0134 | `rustls-pemfile` | Warning (unmaintained) | `aws-smithy-http-client` -> AWS SDK chain | Accept for v0.1.0 |
| RUSTSEC-2026-0002 | `lru` | Warning (unsound `IterMut`) | `aws-sdk-s3` -> CLI/tests | Accept with caution |

#### Rationale

- **number_prefix**: Only affects CLI progress bar formatting. No security impact.
- **rustls-pemfile**: Transitive via AWS SDK TLS stack. AWS SDK updates will resolve.
- **lru**: Unsoundness in `IterMut` iterator. We do not use this API path directly. AWS SDK updates will resolve.

#### Remediation Plan

These warnings will be addressed in v0.1.1 by:
1. Updating `indicatif` when a release with replaced `number_prefix` is available
2. Updating AWS SDK dependencies when new releases address the transitives

## Security Hardening Checklist

For production deployments:

- [ ] Run behind a reverse proxy with TLS termination
- [ ] Set strong root credentials via `STRIX_ROOT_USER` and `STRIX_ROOT_PASSWORD`
- [ ] Enable audit logging and forward to SIEM
- [ ] Restrict Admin API port (9001) to management network
- [ ] Configure appropriate bucket policies (deny by default)
- [ ] Enable Object Lock for compliance-critical buckets
- [ ] Regular `cargo audit` checks in CI pipeline
- [ ] Monitor for dependency updates addressing advisories

## Encryption Details

### At-Rest Encryption

| Component | Algorithm | Key Derivation |
|-----------|-----------|----------------|
| Access key secrets | AES-256-GCM | HKDF from root secret |
| Object data (SSE-S3) | AES-256-GCM | Per-object random key |
| Object data (SSE-C) | AES-256-GCM | Customer-provided key |

### In-Transit Encryption

Strix does not terminate TLS directly. Deploy behind a TLS-terminating reverse proxy (nginx, Caddy, etc.) for production use.

## Changelog

Security-related changes are documented in [CHANGELOG.md](CHANGELOG.md) under the "Security" section for each release.
