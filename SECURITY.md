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

### Current Audit Status

`cargo audit` and `cargo deny check advisories` both pass with no vulnerabilities
and no accepted (ignored) advisories.

Known transitive advisories, both accepted and resolved, are tracked under
[`docs/advisories/`](docs/advisories/):

- [`docs/advisories/accepted.md`](docs/advisories/accepted.md) — advisories that
  are knowingly accepted and the matching `deny.toml` ignore entries (currently
  none).
- [`docs/advisories/resolved.md`](docs/advisories/resolved.md) — advisories that
  were cleared by dependency updates, with the fix that resolved each one.

`deny.toml` is the authoritative ignore list and is kept in sync with
`docs/advisories/accepted.md`.

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
