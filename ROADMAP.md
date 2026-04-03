# Strix Console & Backend Roadmap

Principle: minimize scope creep; ship in thin vertical slices. Backend streaming/metrics can run in parallel with console work.

## Phase 1 – State & Data Foundations
- Replace ad-hoc reloads with `create_resource`/`create_action` and signals for buckets, objects, users.
- Centralize API base URL, auth token attachment, and 401/429 handling (redirect to login + toasts).
- Add mutation invalidation helpers; drop browser alerts/confirms in favor of modal/toast patterns.
- Outcome: stable reactive data flow; no full-page reloads.

## Phase 2 – High-Value Pages (Buckets, Metrics, Users)
- Buckets: search/filter, prefix chips, create modal with versioning/object-lock toggles; improved upload UI (progress per file, retry, drag-drop help overlay).
- Metrics: wire to real Prometheus endpoint; auto-refresh (10–30s); cards + circular gauges for ops/sec, errors, latency, capacity.
- Users: searchable table, create user wizard with access key reveal, status badges.
- Outcome: primary admin journeys align with MinIO expectations.

## Phase 3 – Components As Needed
- Build only what’s required for current pages: FeatureToggle, SearchableTable (filter/sort/empty state), CircularGauge, ContextPanel (inline help), PolicyEditor (JSON + validation).
- Enhance Toast queue, Modal accessibility, responsive Sidebar.
- Outcome: reusable primitives without overbuilding.

## Phase 4 – Backend Parity Enablers (in parallel)
- Streaming GET/PUT and range support (avoid full buffering); enforce memory/backpressure limits.
- Real metrics export (Prometheus) for S3/Admin ops, latency, errors; health/readiness endpoints.
- Security hardening: hash secrets, policy conditions/principals, bucket policy path, cleanup task dedupe, CSRF origin config.
- Outcome: backend supports console features and MinIO-like operability.

## Phase 5 – Polish, A11y, Delivery
- Theming: dark navy + Strix blue tokens; consistent spacing, focus rings; Strix logo assets.
- Accessibility: focus order, aria roles, keyboard nav, contrast; responsive tables → stacked cards on mobile.
- Docs: align README/SPEC vs actual features; note config for rate limiting, CSRF, cleanup.
- Tooling: Trunk release profile with hashed assets; CI for fmt/clippy/tests + selected integration GUI checks.
- Outcome: production-ready console with aligned docs and release pipeline.

## Guardrails
- Timebox feature slices; ship incrementally (Phase 1 foundations before UI polish).
- Keep backend/frontend tracks synced on metrics schema and auth behaviors.
