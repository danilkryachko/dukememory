# dukememory.

Local, token-light project memory for coding agents.

`dukememory` stores the project context that should survive across chats:
goals, decisions, constraints, commands, known issues, task state, and design
notes. It runs locally on SQLite + FTS, with optional embeddings for semantic
recall.

![dukememory. web UI](docs/screenshot.png)

## What It Does

- gives agents a tiny `brief` before coding
- shows `impact` memory for files, symbols, or subsystems
- saves structured cards instead of noisy transcripts
- keeps memory in `.agent/memory.db`
- supports local embeddings through Ollama or OpenAI-compatible endpoints
- exposes a local web UI and MCP server
- runs reversible autonomous maintenance

## Install

```bash
cargo build --release

target/release/dukememory update-install \
  --from target/release/dukememory \
  --to ~/.local/bin/dukememory

dukememory --version
```

## Quick Start

```bash
cd /path/to/project

dukememory onboard --root . --install-autonomous
dukememory install-skill
dukememory memory-contract --write
```

Use it during development:

```bash
dukememory brief "fix checkout validation" --budget-profile tiny
dukememory impact src/checkout.ts --budget-profile tiny
dukememory recall "checkout validation" --max-chars 1200
dukememory drift --root . --json
```

Save durable knowledge:

```bash
dukememory add decision \
  "Checkout validation stays client-side first" \
  "Server validation remains authoritative; client validation improves feedback." \
  --link file:src/checkout.ts

dukememory embed-index
```

## Memory Cards

Common types: `product_goal`, `decision`, `constraint`, `command`,
`known_issue`, `design_note`, `task_state`, `user_preference`.

The useful rule: save decisions, rules, commands, risks, and continuation state.
Do not save full chat transcripts.

## Embeddings

Embeddings are optional. Without them, retrieval still works through SQLite FTS.

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

## Codex And MCP

```bash
dukememory install-skill
dukememory workspace-init --root . --force
dukememory serve-mcp
dukememory codex-doctor --json
```

Agent flow: `brief` first, `impact` for touched files, `drift` before broad
edits, write only durable outcomes, then re-index embeddings after important
writes.

## Autonomous Maintenance

```bash
dukememory autonomous install --force --level normal
dukememory autonomous status --json
dukememory autonomous rollback --json
```

Maintenance is designed to be auditable and reversible.

## Development

```bash
cargo fmt --check
cargo test
cargo test --features vec
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## License

No license has been added yet.
