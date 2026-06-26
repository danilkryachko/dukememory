# dukememory

Production v0.14.13 local memory for agent-driven projects.

- CodeGraph remembers code structure: files, symbols, calls, dependencies.
- dukememory remembers product intent: goals, decisions, preferences, commands,
  design notes, known issues, and current task state.

The implementation is Rust + SQLite + FTS5 + Rhai, with optional local
embeddings. It stays useful without a model server, and adds semantic recall
when Ollama or an OpenAI-compatible endpoint is available.

## Build

```bash
cargo build --release
```

The release binary is:

```bash
target/release/dukememory
```

During development, run:

```bash
cargo run -- <command>
```

Install the current binary:

```bash
cargo run -- install --to ~/.local/bin --force
```

Update an installed binary from a freshly built release binary:

```bash
cargo build --release
dukememory update-install \
  --from target/release/dukememory \
  --to ~/.local/bin/dukememory
dukememory build-info
```

`update-install` writes a backup of the previous installed binary under
`.agent/install-backups`, reports source/target versions and SHA-256 hashes, and
supports `--dry-run` and `--json`.

## Storage

Default database:

```text
.agent/memory.db
```

Override it with `--db` or `DUKEMEMORY_DB`.

Initialize local config and the database:

```bash
cargo run -- init
```

Runtime config is loaded from `--config`, `DUKEMEMORY_CONFIG`, or
`.agent/config.toml`, with environment overrides for embedding provider,
endpoint, and model.

## Quickstart

Use the smallest useful surfaces first:

```bash
cargo run -- remember "User wants memory to stay local, fast, and token-light."
cargo run -- brief "continue auth rate limit work" --budget-profile tiny
cargo run -- impact src/auth.rs --budget-profile tiny
cargo run -- drift --changed-only
cargo run -- evidence <memory-id>
```

`brief` gives the agent a tiny task pack, `impact` narrows memory to one
file/symbol/topic, and `drift` catches cheap local inconsistencies before larger
edits. None of these commands require embeddings; when embeddings are configured,
hybrid retrieval uses them only when the matching local index is already ready.

## Card Model

Types:

- `product_goal`
- `user_preference`
- `decision`
- `design_note`
- `known_issue`
- `command`
- `task_state`
- `domain_fact`
- `constraint`
- `note`

Statuses:

- `active`
- `superseded`
- `rejected`
- `uncertain`

Formal scopes:

- `global`
- `user`
- `project`
- `repo`
- `thread`
- `task`

Cards also support:

- `scope`
- `source`
- `confidence`
- `supersedes` / `superseded_by`
- links such as `file:src/main.rs` or `symbol:exportPdf`

## Common Commands

Add a decision:

```bash
cargo run -- add decision \
  "MVP auth without SSO" \
  "Use email/password for the first version. SSO is deferred because launch speed matters more."
```

Add a preference:

```bash
cargo run -- add user_preference \
  "No marketing landing pages" \
  "The user prefers opening directly into the working product interface."
```

Attach links to code or CodeGraph symbols:

```bash
cargo run -- add known_issue \
  "PDF export requires Chrome" \
  "The print pipeline is stable only in Chrome." \
  --link file:src/export.ts \
  --link symbol:exportPdf
```

Search memory:

```bash
cargo run -- search "authorization login"
```

Build a small context pack for an agent task:

```bash
cargo run -- context-pack "make login easier"
```

Read one card:

```bash
cargo run -- get <memory-id>
```

Update a card:

```bash
cargo run -- update <memory-id> \
  --title "Updated title" \
  --body "Updated body" \
  --confidence 0.9 \
  --replace-links \
  --link file:src/main.rs
```

Supersede an old decision while adding the replacement:

```bash
cargo run -- add decision \
  "MVP auth with magic links" \
  "Use passwordless magic links instead of email/password." \
  --supersedes <old-memory-id>
```

Change status directly:

```bash
cargo run -- status <memory-id> superseded
```

Delete:

```bash
cargo run -- delete <memory-id>
```

Show stats:

```bash
cargo run -- stats
```

## v5 Agent-Native Workflow

High-level commands for a non-programmer/user-driven project:

```bash
cargo run -- remember "User wants everything local, fast, and easy."
cargo run -- what-do-we-know "local memory"
cargo run -- what-next
cargo run -- forget "old firebase decision" --dry-run
```

Agent context planner:

```bash
cargo run -- context "continue memory implementation" --mode agent
cargo run -- context "deep code task" --mode deep
```

Project snapshot and maintenance:

```bash
cargo run -- snapshot
cargo run -- snapshot --with-codegraph
cargo run -- doctor
cargo run -- doctor --fix-redact
```

Incremental embeddings:

```bash
cargo run -- embed-status
cargo run -- embed-watch --once
```

Packaging helpers:

```bash
cargo run -- completions bash
cargo run -- completions zsh
cargo run -- completions fish
cargo run -- man
```

## v6 Policy And MCP

Content-Length MCP transport:

```bash
target/release/dukememory serve-mcp --content-length
```

Extended Rhai policy hooks:

```rhai
fn score_memory(type, status, scope, title, body, task, confidence) {
  if type == "decision" { 5.0 } else { 0.0 }
}

fn should_include(type, status, scope, title, body, task, confidence) {
  status != "rejected"
}

fn should_redact(type, status, scope, title, body, confidence) {
  body.contains("api_key") || body.contains("token =")
}
```

Check/apply:

```bash
cargo run -- policy-check .agent/rules.rhai
cargo run -- policy-apply .agent/rules.rhai --dry-run
cargo run -- policy-apply .agent/rules.rhai
```

## v7 Operations

Audit log:

```bash
cargo run -- audit
cargo run -- audit --json
```

Workspace bootstrap:

```bash
cargo run -- workspace-init
cargo run -- workspace-init --force
```

Support bundle:

```bash
cargo run -- bundle support-bundle.json --redact
```

## v8 Local Service And Maintenance

Daemon loop:

```bash
cargo run -- daemon --provider mock --endpoint local --model mock-small --once
cargo run -- daemon
cargo run -- daemon --auto-ingest --session-dir .agent/sessions
```

Local HTTP API:

```bash
cargo run -- serve-http --host 127.0.0.1 --port 8765
open http://127.0.0.1:8765/
curl http://127.0.0.1:8765/health
curl http://127.0.0.1:8765/snapshot
```

Supported endpoints:

- `GET /`
- `GET /ui`
- `GET /health`
- `GET /projects`
- `GET /autopilot/ui`
- `POST /autopilot/run-once`
- `POST /autopilot/repair`
- `POST /autopilot/export-status`
- `GET /metrics`
- `GET /audit`
- `GET /snapshot`
- `GET /doctrine`
- `GET /memory`
- `GET /inbox`
- `POST /remember`
- `POST /memory/status`
- `POST /memory/delete`
- `POST /memory/update`
- `POST /memory/bulk`
- `POST /context`
- `POST /brief`
- `POST /impact`
- `POST /drift`
- `POST /search`
- `POST /inbox/approve`
- `POST /inbox/reject`
- `POST /evidence`
- `POST /auto-ingest`
- `POST /doctor`
- `POST /sync/export`
- `POST /merge/candidates`
- `POST /merge/apply`

Vector backend switch:

```bash
cargo run -- vec-migrate --backend json
cargo run -- embed-search "memory" --backend json
```

`sqlite-vec` is guarded by the `vec` feature:

```bash
cargo build --features vec
cargo run -- vec-migrate --backend sqlite-vec
```

Merge and contradiction maintenance:

```bash
cargo run -- merge-candidates
cargo run -- merge-apply <primary-id> <duplicate-id> --dry-run
cargo run -- resolve-contradictions --dry-run
```

Profiles:

```bash
cargo run -- profile use dukegraph
cargo run -- profile list
```

Review and LLM-assisted maintenance:

```bash
cargo run -- review-ui
cargo run -- maintain
cargo run -- maintain --llm --endpoint http://192.168.0.13:11434 --model qwen3:14b
```

Budgeted context:

```bash
cargo run -- context "task" --budget 12000
```

Local sync:

```bash
cargo run -- sync export memory-sync.json --redact
cargo run -- sync import memory-sync.json
```

## v9 Hardening

Schema migrations and verification:

```bash
cargo run -- schema status
cargo run -- schema verify
cargo run -- schema upgrade
```

Lock diagnostics for daemon/HTTP/MCP safety:

```bash
cargo run -- lock status
cargo run -- lock clear
```

Hybrid retrieval and context contracts:

```bash
cargo run -- retrieve "task" --strategy hybrid --format agent
cargo run -- retrieve "task" --strategy fts --format markdown
cargo run -- context "task" --format agent --budget 12000
```

Evaluation harness:

```bash
cargo run -- eval add-case "auth decision" "auth login" "magic links"
cargo run -- eval run
```

Compaction v2:

```bash
cargo run -- compact-v2 --scope project --dry-run
cargo run -- compact-v2 --scope project
```

Self diagnostics and build info:

```bash
cargo run -- doctor --self-check
cargo run -- build-info
```

## v11 Session Ingest And Doctrine

Auto-ingest agent session files into pending inbox suggestions:

```bash
cargo run -- auto-ingest --input .agent/sessions
cargo run -- auto-ingest --input .agent/sessions --dry-run
```

Supported session file extensions are `.md`, `.txt`, `.log`, and `.jsonl`.
Files are tracked by path and content hash, so repeated runs do not create
duplicate inbox suggestions.

Print the active decision doctrine:

```bash
cargo run -- doctrine
cargo run -- doctrine --json
```

Doctrine shows active decisions, superseded decision chains, and likely active
decision conflicts. It is also available through `GET /doctrine` and the MCP
tool `memory_doctrine`.

Agent session ingest is also available through `POST /auto-ingest` and the MCP
tool `memory_auto_ingest`.

Production release operations:

```bash
cargo run -- self-host --force
cargo run -- bench --json
cargo run -- release-bundle dist/dukememory
cargo run -- update-install --from target/release/dukememory --to ~/.local/bin/dukememory --dry-run
```

`release-bundle` copies the current binary, writes `manifest.json` with schema,
version, stats, and SHA-256 checksum, and emits a production config template.
`update-install` safely replaces an installed binary after making a backup.
`bench` measures local SQLite/FTS/export paths without requiring Ollama.
`self-host` stores durable memory cards about the memory system itself.

The v11 module split adds explicit storage and service layers:

- `src/main.rs` - thin binary entrypoint.
- `src/app.rs` - command dispatch and shared application glue.
- `src/app/cli.rs` - clap command model.
- `src/app/db.rs` - SQLite schema, migrations, and schema verification.
- `src/app/http_server.rs` - local HTTP API runtime.
- `src/app/mcp_server.rs` - MCP JSON-RPC stdio/runtime surface.
- `src/app/embeddings.rs` - embedding providers, indexing, semantic search,
  status, watch, and vector benchmarks.
- `src/app/release_ops.rs` - support bundles, release bundles, benchmark, and
  self-host memory seeding.
- `src/app/ops.rs` - v12 health, backup rotation, cleanup, and launchd install.
- `src/storage.rs`
- `src/services.rs`

## v12 Always-On Operations

Production v12 is the first version intended for permanent local use.
Production v13 strengthens that base with SQLite tuning, integrity checks,
explicit optimization commands, hardened daemon lock release, and robust HTTP
request reading for larger local payloads.
Production v13.1 keeps the same schema and hardens daily use further with
atomic verified backups, SHA-256 sidecar files, deeper SQLite diagnostics, and
more defensive daemon lock error handling.
Production v13.2 keeps the same schema and switches backups to SQLite
`VACUUM INTO`, so WAL-mode databases are backed up as consistent standalone DB
files. Restore verifies a `.sha256` sidecar when one exists.
Production v13.3 keeps the same schema and adds explicit backup verification:
integrity check, optional sidecar checksum validation, schema version, and
critical table counts are reported before restore.
Production v13.4 keeps the same schema and adds a `.manifest.json` next to
policy backups. Restore now copies into a temporary DB, verifies SQLite
integrity on that temp file, then atomically renames it into place.
Production v13.5 keeps the same schema and adds strict backup verification plus
restore dry-run preflight, so restores can be validated without writing the
target database.
Production v13.6 keeps the same schema and makes restore reversible by saving a
rollback backup of the target DB before replacing it.
Production v13.7 keeps the same schema and turns restore rollback files into
strictly verifiable backup artifacts with `.sha256` and `.manifest.json`
sidecars.
Production v13.8 keeps the same schema and hardens backup metadata publication:
checksum and manifest sidecars are written through temporary files, atomically
renamed into place, and immediately strict-verified after publication.
Production v13.9 keeps the same schema and prunes orphan backup temp files from
interrupted backup runs before creating the next verified backup.
Production v13.10 keeps the same schema and prunes orphan backup sidecars whose
base backup DB has already been removed by older retention runs or manual
cleanup.
Production v13.11 keeps the same schema and tightens manifest verification by
requiring recorded source table counts to match the backup table counts.
Production v13.12 keeps the same schema and improves recovery diagnostics:
`backup-verify` reports machine-readable failure reasons, and real restores
write JSON journals.
Production v14.0 upgrades the agent-facing memory layer:
retrieve v2 returns scored hits with reasons, context-pack v2 groups memories by
role, hybrid retrieval reports semantic fallback state, and Rhai rules can tune
retrieve/context ranking.
Production v14.9 adds schema 14 read-audit telemetry: brief, impact, retrieve,
and evidence reads emit lightweight `Memory:` receipts and can be inspected with
`dukememory usage-report`.
Production v14.1 turns the daemon into an autopilot: each tick can ingest
session logs, refresh embeddings, rotate verified backups, run cleanup, and
write a daemon status JSON file without manual commands.
Production v14.2 adds an autopilot control plane: status, doctor, run-once, and
install commands for unattended operation.
Production v14.3 adds autopilot self-healing: repair creates safe missing
directories, clears only expired daemon locks, runs a maintenance tick when
status/backups/embeddings need it, and reports before/after doctor state.
Production v14.4 adds autopilot observability: structured daemon events,
history, report, and export-status for external monitoring.
Production v14.5 adds autopilot alerts, threshold checks, monitor-friendly
exit codes, optional JSON alert export, tiny task briefs, targeted impact
reports, cheap drift checks, and evidence reports for individual memory cards.
Production v14.6 adds the local Memory UI: `serve-http` now serves a browser
workbench for search, review, inbox approval, evidence, and status changes.
Production v14.7 adds project selection to the Memory UI, discovering sibling
projects with `.agent/memory.db` and showing the selected project's memory.
Production v14.8 turns the browser UI into a Memory Workbench with tabs,
detail view, editing, bulk actions, inbox review, project stats, autopilot
status, and persisted UI state.
Production v14.9 polishes the web workbench with a sticky master-detail layout,
inline editing, audit timelines, highlighted search, quick filters, richer
project stats, browser autopilot actions, activity history, and responsive UI
QA.

One-time setup:

```bash
cargo run -- init
cargo run -- self-host --force
cargo run -- health --endpoint mock --json
```

Daily/weekly operations:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10
cargo run -- cleanup --audit-keep 5000 --dry-run
cargo run -- cleanup --audit-keep 5000
```

macOS launchd daemon plist:

```bash
cargo run -- daemon-install \
  --output ~/Library/LaunchAgents/com.dukememory.daemon.plist \
  --session-dir .agent/sessions \
  --interval-secs 60 \
  --force
```

Then load it with:

```bash
launchctl load ~/Library/LaunchAgents/com.dukememory.daemon.plist
```

Health checks are intentionally non-destructive. `health --json` reports schema,
database, SQLite integrity, backup directory, session directory, and model
endpoint readiness. Use `--endpoint mock` when you want to validate local memory
without probing Ollama.

v13 stabilization commands:

```bash
cargo run -- integrity --json
cargo run -- optimize --json
cargo run -- optimize --vacuum --json
cargo run -- health --endpoint mock --json
```

`integrity` runs SQLite `PRAGMA integrity_check` and foreign-key checks.
`optimize` runs `ANALYZE`, SQLite `PRAGMA optimize`, FTS optimize, and WAL
checkpointing; `--vacuum` additionally compacts the database file.

v13.1 backup hardening:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
```

Backups are written through a temporary file, atomically renamed into place, and
verified with SHA-256. A `.sha256` sidecar is written next to each retained DB
backup and pruned together with old backups.

v13.2 backup consistency:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
cargo run -- backup-verify .agent/backups/<backup>.db --json
cargo run -- backup .agent/manual-backup.db
cargo run -- --db .agent/restored.db restore .agent/backups/<backup>.db --force
```

Both `backup-policy` and `backup` create backups through SQLite `VACUUM INTO`
instead of copying the main DB file directly. This matters in WAL mode because
recent committed changes may live in the WAL file.

v13.3 verification:

```bash
cargo run -- backup-verify .agent/backups/<backup>.db --json
```

`backup-verify` checks SQLite integrity, validates the `.sha256` sidecar when it
exists, reports the backup schema version, and lists table counts. `restore`
runs the same verification before replacing the target DB.

v13.4 manifest and restore safety:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
cargo run -- backup-verify .agent/backups/<backup>.db --json
cargo run -- --db .agent/restored.db restore .agent/backups/<backup>.db --force
```

Policy backups write `.db`, `.db.sha256`, and `.db.manifest.json`. The manifest
records tool version, schema, backup file name, file size, SHA-256, integrity
status, and table counts. `backup-verify` validates the manifest when present,
and `restore` refuses backups whose manifest or checksum no longer matches.

v13.5 strict verification and restore preflight:

```bash
cargo run -- backup-verify .agent/backups/<backup>.db --strict --json
cargo run -- --db .agent/restored.db restore .agent/backups/<backup>.db --strict --dry-run --force
cargo run -- --db .agent/restored.db restore .agent/backups/<backup>.db --strict --force
```

Strict mode requires both `.db.sha256` and `.db.manifest.json` to exist and
match. `restore --dry-run` performs the same verification path but does not
write the target DB.

v13.6 restore rollback safety:

```bash
cargo run -- --db .agent/memory.db restore .agent/backups/<backup>.db \
  --strict --dry-run --force --rollback-dir .agent/restore-rollbacks
cargo run -- --db .agent/memory.db restore .agent/backups/<backup>.db \
  --strict --force --rollback-dir .agent/restore-rollbacks
```

When the target DB already exists, `restore` writes a rollback DB into
`.agent/restore-rollbacks` before replacing the target. Use `--no-rollback`
only when you intentionally do not want that safety copy.

v13.7 rollback verification:

```bash
cargo run -- backup-verify .agent/restore-rollbacks/<rollback>.db --strict --json
```

Rollback DBs now get the same checksum and manifest sidecars as policy backups.
`restore` verifies the rollback in strict mode before replacing the target DB,
so a failed rollback write stops the restore instead of leaving an unchecked
safety copy.

v13.8 atomic backup metadata:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
cargo run -- backup-verify .agent/backups/<backup>.db --strict --json
```

Backup sidecars are no longer published by direct writes. The checksum and
manifest are written to temporary files, atomically renamed into their final
paths, then verified with the same strict path used by restore.

v13.9 backup temp cleanup:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
```

`backup-policy` now removes stale temp files it owns before creating a new
backup: `dukememory-*.db.tmp`, `*.db.sha256.tmp-*`, and
`*.db.manifest.tmp-*`. The JSON report includes `temp_pruned` so cleanup is
auditable.

v13.10 orphan sidecar cleanup:

```bash
cargo run -- backup-policy --output-dir .agent/backups --keep 10 --json
```

After retention pruning, `backup-policy` removes orphan
`dukememory-*.db.sha256` and `dukememory-*.db.manifest.json` files when the
matching `.db` file is gone. The JSON report includes `sidecar_pruned`.

v13.11 manifest source-count verification:

```bash
cargo run -- backup-verify .agent/backups/<backup>.db --strict --json
```

Manifest verification now checks both `source_table_counts` and
`backup_table_counts` against the actual backup DB. A manifest with tampered or
stale source counts fails strict verification.

v13.12 verification diagnostics and restore journal:

```bash
cargo run -- backup-verify .agent/backups/<backup>.db --strict --json
cargo run -- --db .agent/memory.db restore .agent/backups/<backup>.db \
  --strict --force \
  --rollback-dir .agent/restore-rollbacks \
  --journal-dir .agent/restore-journal
```

`backup-verify --json` includes `reasons`, such as checksum, manifest, or strict
sidecar failures. Real `restore` runs write a JSON journal with source, target,
rollback path, rollback verification status, final status, and error text when
the restore fails. `restore --dry-run` remains read-only and does not write a
journal.

v14.0 retrieve and context v2:

```bash
cargo run -- retrieve "task" --strategy hybrid --format json
cargo run -- retrieve "task" --strategy hybrid --format agent --rules .agent/rules.rhai
cargo run -- retrieve "task" --strategy hybrid --budget-profile tiny
cargo run -- brief "task" --budget-profile tiny
cargo run -- impact src/app.rs --budget-profile tiny
cargo run -- drift --changed-only
cargo run -- evidence <memory-id>
cargo run -- context-pack "task" --semantic --max-chars 4000
cargo run -- context-pack "task" --budget-profile normal
cargo run -- context "task" --mode agent --rules .agent/rules.rhai
cargo run -- context "task" --mode agent --budget-profile deep
```

`retrieve --format json` now returns a v14 report with scored hits,
`utility_score`, semantic score when available, and reasons such as
`text_match`, `link_match`, `semantic`, `fresh`, `superseded_by`, and
`rhai_score`. `context-pack` groups output into Decisions, Constraints, Current
Facts, Risks, Recent Work, and Other so the agent can consume only the relevant
fragments.

Hybrid retrieval is FTS-first and non-blocking: it uses embeddings only when the
selected provider/endpoint/model already has a ready local index. If the index is
missing or stale, `retrieve` reports `semantic_error` and falls back to local
FTS/ranking without waiting for model calls. `--budget-profile tiny|normal|deep`
maps to compact output budgets of 1200, 3000, and 8000 characters.

`brief TASK` is the smallest agent-facing surface. It returns must-follow
decisions/constraints, relevant memory, risks, linked files/symbols, and check
commands within the selected budget. `impact TARGET` is even more targeted for a
touched file, symbol, or topic: it combines explicit memory links with local FTS.
`drift --changed-only` checks changed-file memory links and metadata without
model calls. `evidence ID` shows source, links, supersession chain, and audit
events for one card.

v14.1 daemon autopilot:

```bash
cargo run -- daemon \
  --session-dir .agent/sessions \
  --backup-dir .agent/backups \
  --status-file .agent/daemon-status.json
```

Autopilot is enabled by default. A daemon tick refreshes embeddings, auto-ingests
session logs, creates verified backups on schedule, runs operational cleanup,
and writes JSON status. Use `--no-autopilot` when you only want the older
embed/ingest daemon behavior.

v14.2 autopilot control plane:

```bash
cargo run -- autopilot run-once --provider mock --endpoint local --model mock-small
cargo run -- autopilot status --json
cargo run -- autopilot doctor --provider mock --endpoint local --json
cargo run -- autopilot install --dry-run
```

`autopilot run-once` performs one full tick and prints the resulting status.
`autopilot doctor` verifies status freshness, session directory, latest strict
backup, daemon lock state, and endpoint readiness, then prints concrete
recommendations when unattended mode is not ready.

v14.3 autopilot self-healing:

```bash
cargo run -- autopilot repair --provider mock --endpoint local --model mock-small --json
cargo run -- autopilot doctor --repair --provider mock --endpoint local --json
```

Repair only applies safe fixes: it creates missing `.agent/sessions` and
`.agent/backups`, clears expired daemon locks, runs one autopilot tick when
status, backup, or embeddings need refresh, and reports `before`, `after`,
`actions_taken`, and `actions_skipped`.

v14.4 autopilot observability:

```bash
cargo run -- autopilot history --json
cargo run -- autopilot report --provider mock --endpoint local --model mock-small --json
cargo run -- autopilot export-status .agent/autopilot-status.json \
  --provider mock --endpoint local --model mock-small
```

Daemon ticks now write structured JSON event details. `autopilot report`
summarizes tick count, failures, backups, inbox additions, pending inbox,
embedding freshness, latest verified backup, doctor state, and recommendations.
`export-status` writes the same report to one JSON file for external monitors.

v14.5 autopilot alerts:

```bash
cargo run -- autopilot alert \
  --provider mock --endpoint local --model mock-small \
  --max-pending 100 \
  --max-failed-ticks 0 \
  --max-status-age-secs 180 \
  --max-embedding-stale 0 \
  --require-backup \
  --require-endpoint \
  --write-alert .agent/autopilot-alert.json \
  --json
```

`autopilot alert` is read-only unless `--write-alert` is provided. It returns
exit code `0` for `ok` and `2` for `warn` or `critical`, making it suitable for
launchd wrappers, shell monitors, and CI. The JSON output contains `level`,
`violations`, `recommendations`, and the full autopilot report snapshot.

v14.6 local Memory UI:

```bash
cargo run -- serve-http --host 127.0.0.1 --port 8765
open http://127.0.0.1:8765/
```

The UI is bundled into the Rust binary and needs no Node, CDN, or build step.
It supports memory search/filtering, adding cards, pending inbox approval or
rejection, evidence inspection, status changes, and deletion through the local
HTTP API.

v14.7 project-aware Memory UI:

```bash
cargo run -- serve-http --host 127.0.0.1 --port 8765
open http://127.0.0.1:8765/
curl "http://127.0.0.1:8765/projects"
curl "http://127.0.0.1:8765/memory?project=dukeelectonics&status=active"
```

The project selector discovers sibling folders that contain `.agent/memory.db`.
All UI actions pass the selected project key, so search, evidence, inbox,
status changes, and adding memory operate on that project's local memory DB.

v14.8 Memory Workbench:

```bash
cargo run -- serve-http --host 127.0.0.1 --port 8765
open http://127.0.0.1:8765/
```

The UI includes tabs for card detail, editing, inbox review, adding memory,
autopilot status, projects, and settings. It supports multi-select bulk
actions, structured evidence instead of raw JSON, persisted language/project
filters, and update/bulk endpoints for direct browser maintenance.

v14.9 web polish:

```bash
cargo run -- serve-http --host 127.0.0.1 --port 8765
open http://127.0.0.1:8765/
curl -X POST http://127.0.0.1:8765/autopilot/repair -d '{}'
curl -X POST http://127.0.0.1:8765/autopilot/run-once -d '{}'
curl -X POST http://127.0.0.1:8765/autopilot/export-status -d '{}'
```

The workbench keeps the project summary and selected memory visible while
scrolling, highlights search matches, offers one-click type filters, and lets a
selected memory be edited directly from the detail panel. The autopilot tab can
repair local automation state, run one maintenance tick, and export a monitor
status file without leaving the browser.

## v10 Stabilization

The v10 release adds runtime config loading, a real migration registry, HTTP
status/error handling, stricter MCP initialize/error metadata, and the first
production module split:

- `src/runtime_config.rs`
- `src/http_api.rs`
- `src/build_info.rs`

Runtime config:

```bash
cargo run -- --config .agent/config.toml context "task" --mode agent
DUKEMEMORY_CONFIG=.agent/config.toml cargo run -- build-info
```

HTTP errors now use status codes and JSON error envelopes:

- `400 Bad Request`
- `404 Not Found`
- `500 Internal Server Error`

The `sqlite-vec` backend is only accepted when built with `--features vec` and
when the SQLite connection exposes `vec_version()`. Otherwise the command fails
explicitly instead of silently falling back.

## v2 Agent Workflow

Review memory quality:

```bash
cargo run -- review
cargo run -- stale --days 30
cargo run -- conflicts
```

Validate links:

```bash
cargo run -- links --root /path/to/project
```

Close a work session into a compact `task_state` card:

```bash
cargo run -- session-close \
  --title "Auth session" \
  --summary "Implemented magic-link login." \
  --next "Run browser QA" \
  --next "Add password fallback copy"
```

Install the current binary:

```bash
cargo run -- install --to ~/.local/bin --force
```

Check optional vector-search support:

```bash
cargo run -- vec-status
cargo build --features vec
```

The `vec` feature is intentionally optional. The source of truth remains
structured SQLite cards plus FTS5.

## v4 Embeddings

Embedding providers:

- `ollama` for native Ollama `/api/embeddings`
- `openai` for OpenAI-compatible `/v1/embeddings`
- `mock` for deterministic local tests without network or models

Current working endpoint from this Mac:

```text
http://192.168.0.13:11434
```

The Tailscale endpoint is not currently reachable:

```text
http://100.106.127.66:11434
```

Use the LAN endpoint until Windows firewall / Tailscale interface access is
fixed.

Default embedding model:

```text
bge-m3:latest
```

Useful alternatives:

- `nomic-embed-text:latest` for speed and small memory use
- `qwen3-embedding:8b` for larger semantic capacity

Index active memory cards:

```bash
cargo run -- embed-index \
  --provider ollama \
  --endpoint http://192.168.0.13:11434 \
  --model bge-m3:latest
```

Semantic search:

```bash
cargo run -- embed-search "как решили делать авторизацию" \
  --provider ollama \
  --endpoint http://192.168.0.13:11434 \
  --model bge-m3:latest
```

Hybrid context pack:

```bash
cargo run -- context-pack "как решили делать авторизацию" \
  --semantic \
  --embed-provider ollama \
  --embed-endpoint http://192.168.0.13:11434 \
  --embed-model bge-m3:latest
```

Environment variables:

```bash
export DUKEMEMORY_EMBED_PROVIDER=ollama
export DUKEMEMORY_EMBED_ENDPOINT=http://192.168.0.13:11434
export DUKEMEMORY_EMBED_MODEL=bge-m3:latest
```

List provider models:

```bash
cargo run -- provider-list --provider ollama \
  --endpoint http://192.168.0.13:11434
```

Offline smoke test:

```bash
cargo run -- embed-index --provider mock --endpoint local --model mock-small
cargo run -- embed-search "local memory" --provider mock --endpoint local --model mock-small
cargo run -- vector-bench --provider mock --endpoint local --model mock-small
cargo run -- embed-status --provider mock --endpoint local --model mock-small
cargo run -- embed-watch --provider mock --endpoint local --model mock-small --once
```

## v5 Intelligence

Project briefing:

```bash
cargo run -- project-summary
cargo run -- decisions
cargo run -- open-questions
cargo run -- next-actions
```

Lifecycle automation:

```bash
cargo run -- lifecycle --stale-days 30 --dry-run
cargo run -- lifecycle --stale-days 30
```

Suggest cards from a transcript:

```bash
cargo run -- suggest transcript.md
cargo run -- suggest transcript.md --json
cargo run -- suggest transcript.md --to-inbox
```

Ingest transcript into reviewable inbox:

```bash
cargo run -- ingest-transcript transcript.md
cargo run -- ingest-transcript transcript.md --llm \
  --endpoint http://192.168.0.13:11434 \
  --model qwen3:14b
cargo run -- inbox-list
cargo run -- inbox-approve <inbox-id>
cargo run -- inbox-reject <inbox-id>
```

Compact task state:

```bash
cargo run -- compact --scope project --dry-run
cargo run -- compact --scope project
```

Scan stored memory for likely secrets:

```bash
cargo run -- scan-secrets
cargo run -- scan-secrets --fix-redact
cargo run -- export --redact --output memory-export.redacted.json
```

CodeGraph-aware context hints:

```bash
cargo run -- context-pack "auth flow" --with-codegraph
cargo run -- links --root /path/to/project --validate-symbols
```

This does not copy code into memory. If `.codegraph/` exists and the `codegraph`
CLI is installed, v11 runs `codegraph explore` / `codegraph node` and keeps
CodeGraph as the code source of truth.

## Rhai Rules

v14.5 uses Rhai for local scoring/lifecycle hooks. A rules file may define:

```rhai
fn score_memory(type, status, scope, title, body, task, confidence) {
  if type == "decision" { 5.0 } else { 0.0 }
}
```

Check it:

```bash
cargo run -- rhai-check .agent/rules.rhai
```

Use it:

```bash
cargo run -- context-pack "release packaging" --rules .agent/rules.rhai
cargo run -- lifecycle --rules .agent/rules.rhai --dry-run
```

## Import, Export, Backup

Export JSON:

```bash
cargo run -- export --output memory-export.json
```

Import JSON:

```bash
cargo run -- import memory-export.json
```

Replace local memory with an import:

```bash
cargo run -- import memory-export.json --replace
```

Backup the SQLite database:

```bash
cargo run -- backup .agent/memory.backup.db
```

Restore from backup:

```bash
cargo run -- --db .agent/memory.db restore .agent/memory.backup.db --force
```

## Safety

The CLI refuses to store obvious secrets by default: API keys, passwords,
tokens, private keys, and OpenAI-style `sk-` strings. Use `--allow-sensitive`
only when intentionally storing sensitive data.

## MCP-Style Stdio

v14.5 includes an expanded JSON-RPC stdio tool surface:

```bash
target/release/dukememory serve-mcp
target/release/dukememory serve-mcp --content-length
```

Supported tool calls:

- `memory_brief`
- `memory_impact`
- `memory_drift`
- `memory_add`
- `memory_remember`
- `memory_search`
- `memory_context_pack`
- `memory_agent_context`
- `memory_snapshot`
- `memory_doctrine`
- `memory_evidence`
- `memory_auto_ingest`
- `memory_get`
- `memory_review`
- `memory_doctor`
- `memory_inbox_list`

The server accepts newline-delimited JSON-RPC messages on stdin and writes
newline-delimited JSON-RPC responses on stdout. This keeps the implementation
local and dependency-light while giving agents a direct tool interface.

## Why Hybrid Memory?

FTS5 is exact, fast, explainable, and always local. Embeddings are useful for
fuzzy recall when the user describes an old decision in different words. The
retrieval path uses both only when a ready local embedding index exists:
structured SQLite cards remain the source of truth, FTS5 handles precise lookup,
and embeddings are an optional second retrieval layer that must not block normal
development.

The current vector store is provider-aware and uses a JSON fallback in SQLite.
Native `sqlite-vec` can be wired behind the same command surface later without
changing the memory model.
