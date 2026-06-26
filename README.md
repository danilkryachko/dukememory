# dukememory

Local, structured project memory for agent-driven development.

`dukememory` gives coding agents a small durable memory layer: goals,
decisions, constraints, commands, known issues, task state, and design notes.
It is built to stay local, fast, auditable, and token-light. Embeddings are
optional: plain SQLite + FTS works by default, and semantic recall improves the
result when a local embedding model is configured.

![dukememory web UI](docs/screenshot.png)

## Why

LLM chats forget project context. Long prompts waste tokens. Transcript dumps
turn into noise.

`dukememory` keeps the useful parts:

- durable project facts instead of full chat history
- tiny task briefs before coding
- file/symbol impact memory before edits
- reversible autonomous cleanup
- local-first storage in `.agent/memory.db`
- optional semantic recall through Ollama or OpenAI-compatible embeddings

## Core Ideas

| Need | Command |
| --- | --- |
| Start a task with minimal context | `dukememory brief "<task>" --budget-profile tiny` |
| Understand a file, symbol, or subsystem | `dukememory impact <target> --budget-profile tiny` |
| Get compact semantic recall | `dukememory recall "<task>" --max-chars 1200` |
| Save a durable fact | `dukememory add <type> "<title>" "<body>"` |
| Check whether memory is useful | `dukememory memory-qa --json` |
| Keep memory healthy automatically | `dukememory autonomous install --force` |
| Open the local UI | `dukememory serve-http --host 127.0.0.1 --port 8765` |

## Install

Build from source:

```bash
cargo build --release
```

Install or update the local binary:

```bash
target/release/dukememory update-install \
  --from target/release/dukememory \
  --to ~/.local/bin/dukememory
```

Check the installed version:

```bash
dukememory --version
dukememory build-info
```

## Quick Start

Initialize memory in a project:

```bash
cd /path/to/project
dukememory onboard --root . --install-autonomous
dukememory install-skill
dukememory memory-contract --write
```

Add useful project knowledge:

```bash
dukememory add product_goal \
  "Workbench first" \
  "Open directly into the working product surface, not a marketing landing page."

dukememory add command \
  "Validate build" \
  "Run npm run build before calling frontend changes done." \
  --link file:package.json

dukememory add known_issue \
  "Export needs Chrome" \
  "The print/export path is reliable only in Chrome." \
  --link file:src/export.ts
```

Use memory during coding:

```bash
dukememory brief "fix connector hover table" --budget-profile tiny
dukememory impact src/App.tsx --budget-profile tiny
dukememory drift --root . --json
```

Save the result when it is durable:

```bash
dukememory add design_note \
  "Connector hover table" \
  "The hover table must stay open while moving from connector node to table." \
  --link file:src/App.tsx

dukememory embed-index
```

## Card Types

Use structured cards. They are easier to retrieve, score, compact, and clean.

| Type | Use For |
| --- | --- |
| `product_goal` | product direction and project purpose |
| `user_preference` | stable user preferences |
| `decision` | accepted technical or product choices |
| `constraint` | rules that must be followed |
| `command` | build, test, run, deploy commands |
| `known_issue` | bugs, risks, caveats |
| `design_note` | implementation notes worth reusing |
| `task_state` | current continuation state |
| `domain_fact` | domain-specific facts |
| `note` | fallback for low-structure facts |

Statuses:

```text
active | uncertain | superseded | rejected
```

Scopes:

```text
global | user | project | repo | thread | task
```

## Retrieval

Tiny task brief:

```bash
dukememory brief "continue checkout refactor" --budget-profile tiny
```

Targeted impact:

```bash
dukememory impact src/checkout.ts --budget-profile tiny
```

Compressed recall:

```bash
dukememory recall "checkout validation" --max-chars 1200
```

Full retrieval options:

```bash
dukememory retrieve "checkout validation" \
  --strategy hybrid \
  --budget-profile tiny
```

Evidence and doctrine:

```bash
dukememory doctrine --json
dukememory evidence <memory-id> --json
```

## Embeddings

Embeddings are optional. Without them, `dukememory` still uses SQLite FTS and
local ranking. With them, hybrid retrieval can add semantic recall.

Example Ollama setup:

```bash
export DUKEMEMORY_EMBED_PROVIDER=ollama
export DUKEMEMORY_EMBED_ENDPOINT=http://localhost:11434
export DUKEMEMORY_EMBED_MODEL=bge-m3:latest

dukememory embed-index
dukememory embed-status --json
```

OpenAI-compatible endpoints are also supported through environment variables
and runtime config.

## Autonomous Memory

Autonomous maintenance is designed to be reversible. It can refresh embeddings,
create backups, clean operational tables, approve safe inbox items, compact
operational notes, and supersede safe duplicates.

Install the local launchd job on macOS:

```bash
dukememory autonomous install --force --level normal
```

Run one cycle manually:

```bash
dukememory autonomous run-once --level normal --json
```

Inspect and explain the latest cycle:

```bash
dukememory autonomous status --json
dukememory autonomous explain --json
```

Rollback the last reversible cycle:

```bash
dukememory autonomous rollback --json
```

## Quality And Observability

Check whether memory is helping:

```bash
dukememory memory-qa --json
dukememory usage-report --since-days 7 --json
dukememory usefulness-report --json
dukememory quality-report --json
dukememory eval live --json
```

Keep one compact project-wide contract:

```bash
dukememory memory-contract --write
```

Upgrade a project after installing a new `dukememory` release:

```bash
dukememory upgrade-project --root . --json
```

This refreshes workspace rules, `AGENTS.md`, the Codex skill, the memory
contract, and QA checks.

## Local Web UI

Start the UI:

```bash
dukememory serve-http --host 127.0.0.1 --port 8765
```

Open:

```text
http://127.0.0.1:8765/
```

The UI shows memory cards, evidence, inbox items, usage, quality, embeddings,
autonomous status, memory QA, project dashboards, and upgrade dry-runs.

Useful API endpoints:

```text
GET  /health
GET  /dashboard
GET  /memory-qa
GET  /memory-contract
GET  /recall?q=<query>&max_chars=1200
GET  /autonomous/status
POST /upgrade-project
```

## Codex And MCP

Install the Codex skill:

```bash
dukememory install-skill
```

Initialize workspace rules and `AGENTS.md`:

```bash
dukememory workspace-init --root . --force
```

Run the MCP server:

```bash
dukememory serve-mcp
```

Content-Length transport:

```bash
dukememory serve-mcp --content-length
```

Check Codex wiring:

```bash
dukememory codex-doctor --json
```

Agent discipline is intentionally small:

1. Read `brief` first.
2. Use `impact` for touched files or symbols.
3. Use `drift` before broad edits.
4. Write only durable outcomes.
5. Run `embed-index` after important writes.

## Storage

Default paths:

```text
.agent/memory.db
.agent/config.toml
.agent/rules.rhai
.agent/MEMORY_CONTRACT.md
```

Override the database:

```bash
dukememory --db /path/to/memory.db brief "task"
```

Useful environment variables:

```text
DUKEMEMORY_DB
DUKEMEMORY_CONFIG
DUKEMEMORY_EMBED_PROVIDER
DUKEMEMORY_EMBED_ENDPOINT
DUKEMEMORY_EMBED_MODEL
```

## Development

Run checks:

```bash
cargo fmt --check
cargo test
cargo test --features vec
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Create a release bundle:

```bash
dukememory release-bundle dist/dukememory
```

Benchmark local operations:

```bash
dukememory bench --json
```

## Design Principles

- Local first: the database lives in the project.
- Token-light: prefer tiny briefs over large context packs.
- Structured: durable cards beat transcript dumps.
- Observable: memory should show whether it is useful.
- Autonomous: maintenance should run without reminders.
- Reversible: autonomous changes need rollback paths.
- Embeddings optional: semantic recall improves behavior but is never required.

## License

Private project unless a license is added.
