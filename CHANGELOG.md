# Changelog

## 0.42.0 — 2026-07-14 (local development)

### Added

- A stable `ControlSnapshot` schema shared by CLI, MCP `memory_status`, HTTP,
  and the initial web UI, with one normalized health, quality, recall,
  autonomy, session, and optional-runner summary.
- Revision-aware in-process snapshot caching with bounded TTL/entry count,
  cache hit/age/compute telemetry, concurrent-request coverage, and automatic
  invalidation after relevant SQLite or control-file changes.
- Filtered, bounded evidence-session pages by status and outcome, explicit
  derived attempt states, and per-status retention policy defaults in
  `.agent/config.toml`.
- Dedicated control-plane integration coverage outside the historical
  monolithic CLI compatibility test file.
- RAG source packing promotes strong chunks from new files over weaker memory
  cards when the selected pack is memory-heavy and already at its limit.
- Legacy autonomous status snapshots with older embedded quality-report fields
  are normalized on read instead of blocking status, ops, or control surfaces.

### Changed

- `web-control-center` is now the canonical stable surface; V3 through V12
  commands are hidden compatibility aliases, while legacy V12 detail remains
  opt-in through `--details` or `?view=details`.
- The web UI loads detailed diagnostics with one stable request instead of a
  large parallel fan-out across every historical endpoint.
- Agent-session cleanup can safely select completed, failed, partial, and
  abandoned states while remaining dry-run-first and transactionally deleting
  child lifecycle events only after explicit `--apply`.
- Control snapshot and session operations live in focused modules rather than
  adding more routing and lifecycle logic to existing monoliths.

### Fixed

- Prevent repeated control requests from recomputing identical expensive
  diagnostics while still invalidating immediately after durable changes.
- Prevent optional runner or remote-sync readiness from blocking local memory
  readiness in the stable control result.
- Prevent unbounded session history reads and one-size-fits-all cleanup windows
  for unsuccessful or abandoned work.
- Prevent stale autonomous status JSON from failing after quality-report schema
  additions such as `age_days`, `classification`, or `actionable_count`.

## 0.41.0 — 2026-07-14 (local development)

### Added

- Recall benchmark v2 resolves historical read ids through explicit
  supersession chains, probes the exact active successor, records stable probe
  identities in baselines, and reports changed probe sets as stale instead of
  false regressions.
- Quality Score v2 classifies cards as healthy, fresh, dormant, stale,
  obsolete, noisy, oversized, or needing evidence, with separate evidence and
  recommended-action fields plus aggregate actionable counts.
- Evidence-session observability for lease contention, orphaned attempts,
  recovery latency, and stale heartbeats, plus dry-run-first completed-session
  retention over CLI, MCP, HTTP, and the local web UI.
- Explicit required local-autonomy checks and optional sync checks, with
  independent readiness, issues, and recommendations.

### Changed

- The stable web control snapshot now returns the small health, quality,
  recall, local-autonomy, runner, and session summary needed by the initial UI;
  the full diagnostic surface remains lazy and opt-in.
- Ordinary unused durable cards are treated as dormant history rather than
  automatic quality debt; actionable scoring is reserved for evidence-backed
  stale, obsolete, noisy, oversized, or unlinked conditions.
- The local autonomy result is no longer blocked by an unconfigured remote
  target; encrypted remote/VDS sync remains an optional readiness dimension.

### Fixed

- Prevent historical superseded cards in read telemetry from lowering recall
  benchmarks when the active successor is retrievable.
- Prevent changed benchmark probe sets from being compared as if they were the
  same baseline population.
- Prevent missing file links retained only by superseded/rejected history from
  polluting active drift and autonomy readiness; explicit per-card link
  inspection still preserves the historical evidence.
- Prevent completed evidence sessions from accumulating without a bounded,
  reviewable, reversible-by-backup retention workflow.

## 0.40.0 — 2026-07-14 (local development)

### Added

- Schema v21 leased agent sessions with atomic `claim`, `renew`, `release`,
  and stale-session recovery, including per-attempt owner fencing, opaque lease
  tokens, expiry timestamps, heartbeat state, and attempt counters.
- Retry-safe lifecycle events with caller-provided `event_id`, monotonic
  per-session sequences, attempt attribution, exact-retry acceptance, and
  conflicting-payload rejection.
- Agent-session trace v2 metrics for duration, attempts, heartbeats, failures,
  recoveries, runner/model attribution, lease state, evidence count, and
  evidence-backed effectiveness classification.
- CLI, MCP, HTTP, web-control, migration, contention, idempotency, and recovery
  coverage for the leased orchestration protocol.

### Changed

- A session remains compatible with unleased 0.39 clients until it is claimed;
  after claim, context, event, and finish mutations require the current owner
  and lease token and fail closed after expiry or takeover.
- External orchestrators can claim every new or resumed session, renew the
  lease before heartbeat events, attach stable attempt-scoped event ids, and
  pass lease credentials through runner completion and evidence-backed finish.
- The built-in memory UI reports active leases, recoverable workers, attempts,
  event sequence, and the last heartbeat for recent agent sessions.

### Fixed

- Prevent two workers from concurrently mutating or finishing the same durable
  agent session while still allowing a new attempt after release or expiry.
- Prevent retried runner events from duplicating causal history or silently
  changing a previously accepted event payload.
- Exclude sessions with a live lease from stale recovery and atomically fence a
  recovered attempt before it can load context or emit events.

## 0.39.0 — 2026-07-14 (local development)

### Added

- Bounded agent-session lifecycle events for runner selection, start,
  completion, failure, validation, recovery, and heartbeat updates.
- Recoverable-session queries across CLI, MCP, and HTTP so an orchestrator can
  find active work whose heartbeat stopped and resume the same durable session.
- End-to-end external-runner integration coverage using a real temporary
  project, runner profile discovery, memory context, evidence capture, finish
  feedback, causal trace, interruption, and recovery.

### Changed

- Agent-session event writes update the heartbeat and append the event in one
  SQLite transaction, failing closed if the session has already finished.
- External orchestrators can treat DukeMemory session context as the primary
  context layer, route CLI execution through named profiles, and record exact
  changed-file, validation-command, and commit evidence at finish.
- Antigravity review routing uses `Gemini 3.1 Pro (High)` while Gemini Flash
  research routing remains `gemini-3.5-flash`.

### Fixed

- Interrupted runner tasks retain their DukeMemory session id and become
  recoverable instead of silently losing causal context.
- External CLI runners have a bounded timeout with graceful termination and a
  forced-kill fallback.

## 0.38.0 — 2026-07-14 (local development)

### Added

- Schema v20 agent sessions with durable start/context/finish/status/trace
  lifecycle, explicit outcomes, validation evidence, runner attribution, and
  process-crash recovery through SQLite state.
- Evidence-backed feedback that writes a useful signal only after an explicit
  successful finish with recalled memory and recorded files, validation
  commands, or a commit; repeated finishes are idempotent and conflicting
  finishes fail closed.
- Named Codex, Gemini Flash High, Antigravity Pro High, and Ollama runner
  profiles with local TOML overrides, previewable initialization, PATH-based
  doctor checks, and CLI/MCP/HTTP visibility.
- Vector benchmark baselines with p95/QPS regression comparison, configurable
  thresholds, JSON evidence, and a failing local gate.
- MCP and HTTP agent-session control surfaces plus causal traces from recalled
  memory through actions and validation to the final outcome.

### Changed

- The built-in memory UI now loads one stable control snapshot initially;
  versioned diagnostic detail is fetched only on demand.
- Stable `web-control-center` responses include recent agent sessions, runner
  readiness, and an explicit one-request initial-load budget while the full V12
  response remains available at `web-control-center-v12`.
- Existing schema 19 databases add the read-event session link before its index
  is created, keeping upgrades safe and compatible.

### Fixed

- Prevent automatic positive memory feedback for successful-looking work that
  has no explicit validation evidence.
- Reject a second finish that attempts to rewrite a session's outcome or
  evidence while allowing exact retries after interrupted clients.
- Install binary upgrades through same-directory atomic rename so running MCP
  processes keep their old executable mapping while new processes start the
  replacement safely.

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
