# Strix v0.1.0 Release Readiness (Updated 2026-04-03)

## Current Verdict

You are **ready to cut v0.1.0** after completing a short final release-execution checklist.

Core quality and security gates are green locally:
- `cargo fmt --all -- --check` -> pass
- `cargo clippy --workspace --all-targets -- -D warnings` -> pass
- `cargo test --workspace` -> pass
- `cargo build --release --workspace` -> pass
- `cargo audit` -> no vulnerabilities, 3 warnings

## What Is Confirmed Working

### Security fixes
- STS session token enforcement exists in S3 auth path (`crates/strix-s3/src/service.rs:140`).
- ASIA temporary credentials now resolve identity via temp-credential lookup (`crates/strix-s3/src/service.rs:104`).
- IAM temp credential storage is hash-only for session token (`crates/strix-iam/src/store.rs:272`).

### Test coverage
- STS integration tests are present and compiled:
  - `test_sts_assume_role`
  - `test_sts_temp_credentials_require_session_token`
  - `test_sts_temp_credentials_with_valid_session_token`
  - `test_sts_temp_credentials_wrong_session_token`
  (`tests/integration/src/iam.rs:836`)

## Security Audit Results (Local)

`cargo audit` reports **0 vulnerabilities** and **3 warnings**:

1. `RUSTSEC-2025-0119` (`number_prefix` unmaintained) via `indicatif` -> `strix-cli`
2. `RUSTSEC-2025-0134` (`rustls-pemfile` unmaintained) via AWS SDK TLS chain (`strix-cli`, integration tests)
3. `RUSTSEC-2026-0002` (`lru` unsound `IterMut`) via `aws-sdk-s3` (`strix-cli`, integration tests)

Risk note:
- These are warning-level advisories from transitive dependencies in CLI/integration dependency trees.
- No blocking vulnerability advisories were reported for this release check.

## Full List: What Is Left Before v0.1.0

### Must do before tagging

- [x] **Record advisory triage decision** for 3 RustSec warnings - Documented in SECURITY.md
- [x] **Confirm CHANGELOG accuracy** against actual shipped behavior - Updated CHANGELOG.md with security section
- [ ] **Create release tag**: `v0.1.0`

### Strongly recommended before public announcement

- [ ] Add a brief note in release notes about known advisory warnings and planned follow-up
- [ ] Run one final manual STS smoke test against a running server (assume-role, then S3 call with/without token)

### Release execution

- [ ] Push `v0.1.0` tag
- [ ] Publish release notes
- [ ] Attach/build artifacts if you are distributing binaries/images

## Suggested Advisory Triage (Default)

- `RUSTSEC-2025-0119` (`number_prefix`): **Accept for v0.1.0**, track replacement with newer `indicatif` chain.
- `RUSTSEC-2025-0134` (`rustls-pemfile`): **Accept for v0.1.0**, track AWS SDK dependency updates.
- `RUSTSEC-2026-0002` (`lru`): **Accept with caution for v0.1.0**, prioritize SDK update in v0.1.1 cycle.

## Go/No-Go

**Go** once the 3 "Must do" items above are checked.
