# Changelog

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
