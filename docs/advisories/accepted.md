# Accepted Advisories

Security advisories that are knowingly accepted because they cannot currently be
removed (for example, a vulnerable crate pulled only transitively with no
upstream fix available). Each entry here must have a matching ID in the
`[advisories] ignore` list in `deny.toml` so the CI gate and this document stay
in sync.

**There are currently no accepted root-workspace advisories.** `cargo audit`
and `cargo deny check advisories` pass for the root workspace with an empty
ignore list. Advisories that were previously accepted have been resolved; see
[resolved.md](resolved.md).

The GUI has a separate lockfile at `crates/strix-gui/Cargo.lock`. It currently
has no RustSec vulnerabilities, but it does report allowed unmaintained warnings
for `paste` and `proc-macro-error2` through the stable Leptos/Tachys macro stack.
Those warnings are not ignored in `deny.toml`; they should be removed by an
upstream Leptos/Tachys release or by replacing the GUI framework/tooling chain.

## Process for accepting a new advisory

1. Confirm the advisory cannot be cleared by `cargo update` or a dependency
   feature change.
2. Add the `RUSTSEC-XXXX-XXXX` ID to `[advisories] ignore` in `deny.toml`.
3. Add a row to the table below with the rationale and a review date.

| Advisory | Crate | Severity | Source chain | Rationale | Review date |
|----------|-------|----------|--------------|-----------|-------------|
| _(none)_ | | | | | |
