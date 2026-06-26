# dukememory.

**Local AI agent memory for coding workflows.**

SQLite project memory, MCP server, Codex skill, local embeddings, semantic
recall, and reversible autonomous maintenance.

`dukememory` helps coding agents remember the durable parts of a project:
goals, decisions, constraints, commands, known issues, task state, and design
notes. It stays local, fast, auditable, and token-light.

![dukememory. web UI](docs/screenshot.png)

## Core

- Local storage: `.agent/memory.db`
- Search: SQLite FTS by default
- Semantic recall: optional Ollama or OpenAI-compatible embeddings
- Agent access: CLI, HTTP UI, MCP server, Codex skill
- Maintenance: autonomous, observable, rollback-friendly

## Install

```bash
cargo build --release

target/release/dukememory update-install \
  --from target/release/dukememory \
  --to ~/.local/bin/dukememory
```

## Start A Project

```bash
cd /path/to/project

dukememory onboard --root . --install-autonomous
dukememory install-skill
dukememory memory-contract --write
```

## Use Memory

```bash
dukememory brief "fix checkout validation" --budget-profile tiny
dukememory impact src/checkout.ts --budget-profile tiny
dukememory recall "checkout validation" --max-chars 1200
dukememory drift --root . --json
```

Save only durable knowledge:

```bash
dukememory add decision \
  "Checkout validation stays client-side first" \
  "Server validation remains authoritative; client validation improves feedback." \
  --link file:src/checkout.ts

dukememory embed-index
```

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

## MCP And Codex

```bash
dukememory serve-mcp
dukememory install-skill
dukememory codex-doctor --json
```

Agent rule: read `brief`, use `impact`, run `drift` before broad edits, write
only durable outcomes, then re-index embeddings after important writes.

## Autonomous Maintenance

```bash
dukememory autonomous install --force --level normal
dukememory autonomous status --json
dukememory autonomous rollback --json
```

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
