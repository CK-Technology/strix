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
| RUSTSEC-2026-0185 | `quinn-proto` 0.11.14 | Remote memory exhaustion from unbounded stream reassembly | Bumped `quinn-proto` to 0.11.15 through lockfile refresh | 2026-07-02 |
| RUSTSEC-2026-0190 | `anyhow` 1.0.101/1.0.102 | `Error::downcast_mut()` unsoundness warning | Bumped `anyhow` to 1.0.103 in both root and GUI lockfiles | 2026-07-02 |
| RUSTSEC-2026-0194 | `quick-xml` < 0.41.0 | Quadratic duplicate-attribute check in start tags | Moved `s3s` to upstream commit `8ec0bbe` carrying `quick-xml` 0.41.0 | 2026-07-02 |
| RUSTSEC-2026-0195 | `quick-xml` < 0.41.0 | Unbounded namespace allocation in `NsReader` | Moved `s3s` to upstream commit `8ec0bbe` carrying `quick-xml` 0.41.0 | 2026-07-02 |
| RUSTSEC-2023-0071 | `rsa` 0.9.x | Marvin timing side-channel warning | Switched `jsonwebtoken` from `rust_crypto` to `aws_lc_rs`, removing `rsa` from the graph | 2026-07-02 |

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

After these changes the root workspace `cargo audit` and
`cargo deny check advisories` pass, and `deny.toml` carries an empty
`[advisories] ignore` list.

## July 2026 dependency security refresh

The July 2026 refresh removed all root-workspace RustSec failures:

- `s3s` was moved from crates.io `0.14.0` to upstream commit `8ec0bbe`, because
  published `s3s 0.14.0` still constrained `quick-xml` to `^0.40.1` while the
  fix requires `quick-xml >= 0.41.0`.
- `jsonwebtoken` remains on the 10.x line but uses the `aws_lc_rs` backend
  instead of `rust_crypto`, removing the transitive `rsa` crate.
- Lockfiles were refreshed for `quinn-proto` and `anyhow`.

The separate GUI lockfile is on the latest stable Leptos release line
(`leptos 0.8.20`, `leptos_router 0.8.14`). It has no vulnerabilities, but
`cargo audit --file crates/strix-gui/Cargo.lock` still reports allowed
unmaintained warnings for `paste` and `proc-macro-error2` through the upstream
Leptos/Tachys macro stack. `leptos 0.9.0-alpha` was checked and still carries the
same warnings, so moving to the pre-release does not remove them.
