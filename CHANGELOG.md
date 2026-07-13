# Changelog

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
