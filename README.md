# dukememory

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)
[![MCP server](https://img.shields.io/badge/MCP-server-0f766e.svg)](#mcp-and-codex)
[![Local first](https://img.shields.io/badge/local--first-SQLite-0f766e.svg)](#local-first)
[![Brand](https://img.shields.io/badge/brand-protected-6b7280.svg)](TRADEMARKS.md)
![Views](https://komarev.com/ghpvc/?username=danilkryachko-dukememory&label=views&color=0f766e&style=flat-square)

**Local-first memory for AI coding agents.**

[GitHub](https://github.com/danilkryachko/dukememory)

`dukememory` is a Rust CLI, MCP server, and Codex skill that gives Codex,
Claude, Cursor, and other AI coding agents durable project memory. It stores
decisions, constraints, commands, known issues, task state, user preferences,
and design notes in local SQLite, with optional semantic search through
embeddings.

It is built for one job: give agents the smallest useful context before coding,
without dumping chat history into every prompt or slowing development down.

![dukememory. web UI](docs/screenshot.png)

## Why

Coding agents forget important project context. Long prompts waste tokens.
Transcript-based memory quickly turns into noise.

`dukememory` gives them a compact, searchable memory layer:

- **Local-first storage** in `.agent/memory.db` with SQLite and FTS.
- **Agent-native access** through an MCP server, Codex skill, CLI, and web UI.
- **Structured memory cards** for decisions, constraints, commands, issues, and task state.
- **Small context briefs** before coding, including file and symbol impact checks.
- **Optional semantic recall** with Ollama or OpenAI-compatible embeddings.
- **Autonomous maintenance** for freshness, backups, repair hints, gap review, and safe cleanup.
- **Grounded answers** from memory with cited card ids and explicit gaps.
- **One-command Codex wiring** so future chats know memory is installed.
- **Lightweight control surfaces** for health scoring, explainable recall, intent maps, retrieval probes, safe supersede, diff apply, governance policy, context routing, sync dry-runs, and release gates.

## What It Remembers

| Memory | Examples |
| --- | --- |
| Goals | product direction, project purpose |
| Decisions | accepted architecture or UX choices |
| Constraints | rules the agent must keep following |
| Commands | build, test, deploy, setup commands |
| Known issues | bugs, risks, caveats, fragile paths |
| Task state | where work stopped and what is next |
| Design notes | implementation details worth reusing |

## How It Works

1. Store durable facts as typed memory cards.
2. Retrieve a compact `brief` at the start of a task.
3. Retrieve `impact` memory for files, symbols, or subsystems before editing.
4. Use SQLite FTS by default, or add embeddings for semantic recall.
5. Keep memory healthy with observable, reversible autonomous maintenance.
6. Review changed files against memory before saving new durable context.

The result is less repeated explanation, fewer forgotten constraints, and lower
context cost.

## Install

```bash
cargo build --release

target/release/dukememory update-install \
  --from target/release/dukememory \
  --to ~/.local/bin/dukememory
```

## Quick Start

```bash
cd /path/to/project

dukememory onboard --root . --install-autonomous
dukememory install-skill
dukememory memory-contract --write
```

## Daily Commands

```bash
dukememory brief "fix checkout validation" --budget-profile tiny
dukememory impact src/checkout.ts --budget-profile tiny
dukememory recall "checkout validation" --max-chars 1200
dukememory recall "checkout validation" --recent --json
dukememory recall "checkout validation" --changed-since-days 7 --json
dukememory drift --root . --json
dukememory context-governor "fix checkout validation" --target src/checkout.ts --json
dukememory answer "what should we remember about checkout validation?" --json
dukememory explain-recall "checkout validation" --json
dukememory memory-health-score --json
dukememory memory-eval-story --json
dukememory autonomous-usefulness --json
dukememory autonomous-supervisor --json
dukememory fleet-supervisor --json
dukememory fleet-supervisor-watch-install --dry-run --json
dukememory benchmark-polish --json
dukememory recall-benchmark-suite --json
dukememory import-review docs/project-notes.md --json
dukememory memory-upload docs/project-notes.md --json
dukememory memanto-gap-report --json
dukememory memory-diff-review --json
```

Save durable knowledge:

```bash
dukememory add decision \
  "Checkout validation stays client-side first" \
  "Server validation remains authoritative; client validation improves feedback." \
  --link file:src/checkout.ts

dukememory embed-index
```

## Local First

`dukememory` stores data in the project by default:

```text
.agent/memory.db
.agent/config.toml
.agent/MEMORY_CONTRACT.md
```

No cloud service is required. Embeddings are optional.

## Local-First Sync

Remote or VDS sync is optional and remains local-first: agents keep reading the
local SQLite database, while push/pull moves reviewable sync bundles.

```bash
dukememory remote-sync-control --target /mnt/vds/dukememory --json
dukememory vds-sync-pack --target /mnt/vds/dukememory --json
dukememory sync push /mnt/vds/dukememory --dry-run --json
dukememory sync status /mnt/vds/dukememory --json
dukememory sync pull /mnt/vds/dukememory --policy manual --dry-run --json
```

`web-control-center-v5` exposes the same model for UI buttons: preview first,
apply only guarded reversible actions, and keep rollback hints visible.

## Embeddings

```bash
export DUKEMEMORY_EMBED_PROVIDER=ollama
export DUKEMEMORY_EMBED_ENDPOINT=http://localhost:11434
export DUKEMEMORY_EMBED_MODEL=bge-m3:latest

dukememory embed-index
dukememory embed-status --json
```

## Web UI

```bash
dukememory serve-http --host 127.0.0.1 --port 8765
```

Open `http://127.0.0.1:8765/`.

Use it to search memory, inspect evidence, review inbox items, watch usage,
check autonomous health, explain recall, inspect the project intent map, run
retrieval probes, tune ranking, route project memory, and review gaps.

For one compact health view:

```bash
dukememory ops-status --json
```

It combines usage, usefulness, quality, embeddings, autonomous maintenance, and
local-first multi-device readiness. Memory gaps become reviewable suggestions
instead of noisy automatic writes.

## MCP And Codex

```bash
dukememory serve-mcp
dukememory install-skill
dukememory connect-codex --apply --json
dukememory codex-doctor --json
```

Agent rule: read `brief`, use `impact`, run `drift` before broad edits, write
only durable outcomes, then re-index embeddings after important writes.

## Autonomous Maintenance

```bash
dukememory autonomous install --force --level normal
dukememory autonomous-watch-install --dry-run --json
dukememory watch-control --json
dukememory autonomous status --json
dukememory autonomous rollback --json
```

## Control Surfaces

```bash
dukememory context-governor "ship auth fix" --target src/auth.ts --json
dukememory memory-router "auth decisions" --include-siblings --json
dukememory memory-health-score --json
dukememory explain-recall "auth decisions" --json
dukememory project-intent-map --json
dukememory memory-test-harness --json
dukememory agent-audit-v2 --json
dukememory memory-control-center-v2 --json
dukememory auto-supersede-v2 --json
dukememory memory-diff-apply --json
dukememory recall-benchmark-suite --json
dukememory release-gate-v2 --json
dukememory memory-governance-policy --json
dukememory autonomous-loop-v2 --json
dukememory governance-enforce --json
dukememory memory-quality-ci --json
dukememory fleet-dashboard-v2 --json
dukememory remote-sync-apply-flow --target /mnt/vds/dukememory --json
dukememory mcp-tool-surface-v2 --json
dukememory autopilot-v3 --json
dukememory self-learning-retrieval --json
dukememory project-role-profile --json
dukememory inbox-ai-reviewer --json
dukememory web-control-center-v3 --json
dukememory remote-sync-apply --target /mnt/vds/dukememory --json
dukememory mcp-quality-tools --json
dukememory remote-sync-control --target /mnt/vds/dukememory --json
dukememory web-control-center-v4 --json
dukememory mcp-discipline-v2 --json
dukememory feedback-loop-v2 --json
dukememory upgrade-all-projects-v2 --dry-run --json
dukememory vds-sync-pack --target /mnt/vds/dukememory --json
dukememory web-control-center-v5 --json
dukememory quality-autopilot-v31 --json
dukememory memory-router-v2 "project memory" --include-siblings --json
dukememory benchmark-profiles --json
dukememory install-polish --json
dukememory memory-effectiveness-lab --json
dukememory auto-context-budgeter-v2 "project memory" --json
dukememory memory-contract-v2 --json
dukememory cross-project-learning "project memory" --json
dukememory agent-trace --json
dukememory vds-sync-hardening --target /mnt/vds/dukememory --json
dukememory install-quality --json
dukememory web-control-center-v6 --json
dukememory answer "project memory" --json
dukememory connect-codex --json
dukememory memory-type-guide --json
dukememory memory-eval-story --json
dukememory import-review README.md --json
dukememory memory-upload README.md --json
dukememory memanto-gap-report --json
dukememory web-control-center-v7 --json
dukememory autonomous-usefulness --json
dukememory benchmark-polish --json
dukememory web-control-center-v8 --json
dukememory autonomous-supervisor --json
dukememory web-control-center-v9 --json
dukememory fleet-supervisor --json
dukememory web-control-center-v10 --json
dukememory fleet-supervisor-watch-install --dry-run --json
dukememory web-control-center-v11 --json
dukememory auto-ranking-tune --apply --json
dukememory ranking-profile --profile balanced --apply --json
dukememory project-template --kind rust-cli --apply --json
dukememory sync-profile --profile local-first-backup --run-dry-run --json
dukememory remote-sync-wizard --target /mnt/vds/dukememory --json
dukememory remote-sync-v2 --target /mnt/vds/dukememory --json
dukememory autonomy-control-center --json
dukememory upgrade-all-projects --json
dukememory release-gate --run --json
```

These commands keep memory useful without making it heavy: health scoring shows
whether memory is worth trusting, explainable recall shows why cards were
selected, intent maps define project direction, probes measure retrieval quality,
safe supersede and diff apply keep durable cards clean, governance policy bounds
autonomous writes, sync stays local-first, and release gate v2 catches memory
regressions before publishing.

## Development

```bash
cargo fmt --check
cargo test
cargo test --features vec
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## License

Apache-2.0.

## Brand

The code is licensed under Apache-2.0, but the `dukememory` name, wordmark,
screenshots, and project branding are not licensed for use in derivative
products or services. See [TRADEMARKS.md](TRADEMARKS.md).
