# Changelog

## 0.37.0 — 2026-07-13 (local development)

### Added

- Fault-injection coverage for vec0 registry, trigger, row-membership, and table
  corruption, with automatic reconstruction of invalid indexes.
- Sync transport tests for concurrent writers, lock ownership, malformed and
  expired leases, interrupted temporary files, corrupt recovery generations,
  and private atomic replacement.
- Vector benchmark JSON v2 with configurable warmup, iterations, vector limit,
  p50/p95/p99 latency, throughput, backend equivalence, and exact-scale vec0
  comparison.
- Stable `memory-control-center` and `web-control-center` CLI/HTTP aliases while
  retaining all versioned commands and routes for compatibility.

### Changed

- The autonomous supervisor now runs at the conservative level, reports
  before/after quality and guardrails, and previews inferred feedback without
  writing synthetic feedback events; explicit `auto-feedback` remains the
  opt-in materialization path.
- Atomic sync writes now fsync the containing directory after rename, and a
  failed lock initialization removes the incomplete lock file.
- The built-in memory UI consumes stable control-center endpoints.

### Fixed

- Detect vec0 indexes whose row counts happen to match while their row ids do
  not, and repair missing/orphaned memberships on the next database open.
- Recover missing triggers, stale trigger versions, stale registry table names,
  and ordinary SQLite tables shadowing expected vec0 virtual tables.
- Prevent an old sync lock guard from deleting a replacement owner's lock.

## 0.36.0 — 2026-07-13

### Added

- Persistent dimension-specific sqlite-vec indexes for memory and RAG
  embeddings, including startup backfill, synchronized triggers, consistency
  status, explicit rebuild, recovery from stale registry data, and a native vs
  JSON benchmark.
- Versioned sync generations with parent links, per-target peer state, lease
  locks, stale-client protection, previous-generation backups, corruption
  diagnostics, and `sync recover`.
- A tag-driven release workflow that verifies the crate, builds Linux/macOS
  archives, runs installed-binary smoke tests, publishes SHA-256 manifests,
  creates the GitHub release, and publishes to crates.io through a protected
  repository secret.

### Changed

- Native vector search now prefilters endpoint/model metadata in vec0 and keeps
  a transparent JSON fallback for internal semantic memory and RAG flows.
- Sync push performs locked atomic writes with checksum/generation read-back;
  untracked or newer remotes require pull/merge or an explicit `--force`.
- Schema v19 adds vector-index registry and sync peer-generation state.

### Fixed

- Preserve embedding row ids during reindexing and use vec0-compatible
  delete/insert update triggers instead of unsupported replace semantics.
- Import memory exports in two passes so forward supersession references do not
  violate foreign keys or replace already inserted rows.
- Preserve and verify the last good remote generation before overwriting or
  recovering a damaged sync bundle.

## 0.35.0 — 2026-07-13

### Added

- Built-in age/scrypt encryption for sync export/push, passphrase files with
  Unix permission checks, encrypted imports, and encrypted rollback bundles.
- A statically bundled sqlite-vec feature with native SQL cosine search for
  memory and RAG embeddings plus a real `vec0` validation probe.
- Hardened systemd, Caddy, and nginx production templates with a TLS deployment
  and operations guide.

### Changed

- `remote-sync-v2 --apply` now writes and verifies an encrypted remote bundle;
  dry-run remains non-mutating and reports the exact guarded command sequence.
- Sync writes are atomic and permission-restricted, status works without a key
  as unverified metadata, and pull prefers encrypted bundles when both formats
  are present.
- Optimize SHA-256 in development and test profiles, reducing repeated binary
  install dry-runs in the extended compatibility matrix by almost six times.

### Fixed

- Place sync rollback files next to the active project database instead of the
  caller's unrelated working directory.
- Preserve an explicit JSON semantic-search fallback in sqlite-vec builds for
  equivalence testing and compatibility.

## 0.34.0 — 2026-07-13

### Added

- Local RAG ingestion, grounded answers, graph RAG, source diagnostics, and
  optional local generation.
- GitHub CI for formatting, all-target checks, Clippy, default tests, vector
  capability tests, local generation builds, and extended HTTP/autonomy tests.
- HTTP bearer tokens from permission-restricted files, same-origin protection,
  structured access logs, bounded workers, and graceful signal shutdown.

### Changed

- Split the CLI dispatcher, RAG, HTTP routing/security, and sync planning into
  focused modules.
- Run the two slow integration matrices in parallel CI jobs and reuse one HTTP
  server process across the extended HTTP compatibility test.
- Make `vec-validate` the canonical vector command; `vec-migrate` remains a
  hidden compatibility alias.
- Mark `remote-sync-v2` as an experimental, plan-only workflow that does not
  claim encryption or transfer execution.

### Fixed

- Restore 116 hermetic CLI integration tests with mock providers.
- Support nested transactions through SQLite savepoints.
- Avoid HTTP worker stack exhaustion on deep diagnostic routes.
- Remove environment-dependent autopilot alert behavior from CI.
