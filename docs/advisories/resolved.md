# Resolved Advisories

Security advisories that previously appeared in `cargo audit` / `cargo deny`
output and have since been cleared by dependency updates. None of these affected
the core `strix` server binary; all were confined to the `sx` CLI and the
integration test harness, which depend on the AWS SDK.

| Advisory | Crate | Issue | Resolved by | Date |
|----------|-------|-------|-------------|------|
| RUSTSEC-2025-0119 | `number_prefix` | Crate unmaintained | Bumped `indicatif` 0.17 → 0.18 (drops `number_prefix`, adds `unit-prefix`) | 2026-06-16 |
| RUSTSEC-2026-0002 | `lru` 0.12.x | `IterMut` violates Stacked Borrows | AWS SDK update pulls `lru` 0.16.x | 2026-06-16 |
| RUSTSEC-2026-0098 | `rustls-webpki` 0.101.7 | Name constraints accepted for URI names | Switched AWS SDK TLS to `default-https-client` (rustls 0.23 / webpki 0.103) | 2026-06-16 |
| RUSTSEC-2026-0099 | `rustls-webpki` 0.101.7 | Name constraints accepted for wildcard certs | Switched AWS SDK TLS to `default-https-client` (rustls 0.23 / webpki 0.103) | 2026-06-16 |
| RUSTSEC-2026-0104 | `rustls-webpki` 0.101.7 | Reachable panic in CRL parsing | Switched AWS SDK TLS to `default-https-client` (rustls 0.23 / webpki 0.103) | 2026-06-16 |

## How the `rustls-webpki` advisories were cleared

The AWS SDK crates (`aws-config`, `aws-sdk-s3`) default to a legacy TLS stack
(`rustls` feature → `aws-smithy-http-client` legacy path → `rustls 0.21` →
`rustls-webpki 0.101.7`). Selecting the `default-https-client` feature instead
routes through `hyper-rustls 0.27` on `rustls 0.23`, which uses the patched
`rustls-webpki 0.103.x`:

```toml
aws-config = { version = "1", default-features = false, features = ["rt-tokio", "default-https-client", "behavior-version-latest"] }
aws-sdk-s3 = { version = "1", default-features = false, features = ["rt-tokio", "default-https-client", "behavior-version-latest"] }
```

After these changes `cargo audit` reports zero vulnerabilities and zero
warnings, and `deny.toml` carries an empty `[advisories] ignore` list.
