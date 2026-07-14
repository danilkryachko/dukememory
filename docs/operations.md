# Operation catalog

Generated from `src/operation_catalog.rs`. This is the stable operation contract shared by CLI, MCP, and HTTP.

| Operation | Category | Summary | CLI | MCP | HTTP | Mutation | Dry run |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `memory.create` | `memory` | Create durable memory | `add`<br>`remember` | `memory_add`<br>`memory_remember` | `/remember` | yes | no |
| `memory.get` | `memory` | Read memory cards | `get` | `memory_get` | `/memory` | no | no |
| `memory.search` | `memory` | Search memory | `search` | `memory_search` | `/search` | no | no |
| `memory.update` | `memory` | Update a memory card | `update` | — | `/memory/update` | yes | no |
| `memory.status` | `memory` | Change memory status | `status` | — | `/memory/status` | yes | no |
| `memory.delete` | `memory` | Delete a memory card | `delete` | — | `/memory/delete` | yes | no |
| `retrieval.brief` | `retrieval` | Build a tiny verified task brief | `brief` | `memory_brief` | `/brief` | no | no |
| `retrieval.impact` | `retrieval` | Find memory relevant to a target | `impact` | `memory_impact` | `/impact` | no | no |
| `retrieval.context` | `retrieval` | Build a bounded context pack | `context-pack` | `memory_context_pack` | — | no | no |
| `retrieval.rag_answer` | `retrieval` | Answer from grounded project memory | `rag-answer` | `memory_rag_answer` | — | no | no |
| `retrieval.graph_rag_answer` | `retrieval` | Answer with graph-expanded evidence | `graph-rag` | `memory_graph_rag_answer` | — | no | no |
| `memory.doctor` | `operations` | Run compact memory health checks | `doctor` | `memory_doctor` | `/doctor` | no | no |
| `rag.ingest` | `rag` | Index local source files | `rag-ingest` | `memory_rag_ingest` | `/rag-ingest` | yes | yes |
| `rag.sources` | `rag` | Inspect indexed RAG sources | `rag-sources` | `memory_rag_sources` | `/rag-sources` | no | no |
| `rag.eval` | `rag` | Evaluate grounded RAG retrieval | `eval rag` | `memory_rag_eval` | `/rag-eval` | no | no |
| `rag.graph_eval` | `rag` | Evaluate graph-RAG relationships | `eval graph-rag` | `memory_graph_rag_eval` | `/graph-rag-eval` | no | no |
| `release.gate_v2` | `release` | Run V2 release readiness checks | `release-gate-v2` | `memory_release_gate_v2` | `/release-gate-v2` | no | no |
| `release.gate_v3` | `release` | Run V3 release readiness checks | `release-gate-v3` | `memory_release_gate_v3` | `/release-gate-v3` | no | no |
| `agent_session.start` | `agent_session` | Start an evidence-backed session | `agent-session start` | `memory_session_start` | `/agent-sessions/start` | yes | no |
| `agent_session.context` | `agent_session` | Load audited session context | `agent-session context` | `memory_session_context` | `/agent-sessions/context` | no | no |
| `agent_session.claim` | `agent_session` | Claim a worker lease | `agent-session claim` | `memory_session_claim` | `/agent-sessions/claim` | yes | no |
| `agent_session.renew` | `agent_session` | Renew a worker lease | `agent-session renew` | `memory_session_renew` | `/agent-sessions/renew` | yes | no |
| `agent_session.release` | `agent_session` | Release a worker lease | `agent-session release` | `memory_session_release` | `/agent-sessions/release` | yes | no |
| `agent_session.event` | `agent_session` | Record a retry-safe lifecycle event | `agent-session event` | `memory_session_event` | `/agent-sessions/event` | yes | no |
| `agent_session.recover` | `agent_session` | Inspect or claim stale sessions | `agent-session recover` | `memory_session_recover` | `/agent-sessions/recover` | yes | yes |
| `agent_session.finish` | `agent_session` | Finish a session with evidence | `agent-session finish` | `memory_session_finish` | `/agent-sessions/finish` | yes | no |
| `agent_session.status` | `agent_session` | Inspect session status | `agent-session status` | `memory_session_status` | `/agent-sessions` | no | no |
| `agent_session.trace` | `agent_session` | Trace memory influence to outcome | `agent-session trace` | `memory_session_trace` | `/agent-sessions/trace` | no | no |
| `agent_session.cleanup` | `agent_session` | Apply session retention policy | `agent-session cleanup` | `memory_session_cleanup` | `/agent-sessions/cleanup` | yes | yes |
