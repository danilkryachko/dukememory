# Core operation catalog

Generated from `src/operation_catalog.rs`. The catalog covers the stable core memory operations shared across CLI, MCP, and HTTP.

| Operation | CLI | MCP | HTTP | Mutation | Dry run |
| --- | --- | --- | --- | --- | --- |
| `memory.create` | `add`<br>`remember` | `memory_add`<br>`memory_remember` | `/remember` | yes | no |
| `memory.get` | `get` | `memory_get` | `/memory` | no | no |
| `memory.search` | `search` | `memory_search` | `/search` | no | no |
| `memory.update` | `update` | — | `/memory/update` | yes | no |
| `memory.status` | `status` | — | `/memory/status` | yes | no |
| `memory.delete` | `delete` | — | `/memory/delete` | yes | no |
