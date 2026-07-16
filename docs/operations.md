# Operation catalog

Generated from `src/operation_catalog.rs`. This is the stable operation contract shared by CLI, MCP, and HTTP.

Every JSON entry also exposes stable `input_schema` and `output_schema` identifiers used by MCP.

| Operation | Category | Summary | CLI | MCP | HTTP | Stability | Authorization | Mutation | Dry run | Idempotent | Destructive | Open world |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `catalog.list` | `catalog` | List stable cross-surface operations | `operations` | `memory_operations` | `/operations` | `stable` | `project_read` | no | no | yes | no | no |
| `memory.create` | `memory` | Create durable memory | `add`<br>`remember` | `memory_add`<br>`memory_remember` | `/remember` | `stable` | `project_write` | yes | no | no | no | no |
| `memory.get` | `memory` | Read memory cards | `get` | `memory_get` | `/memory` | `stable` | `project_read` | no | no | yes | no | no |
| `memory.search` | `memory` | Search memory | `search` | `memory_search` | `/search` | `stable` | `project_read` | no | no | yes | no | no |
| `memory.update` | `memory` | Update a memory card | `update` | `memory_update` | `/memory/update` | `stable` | `project_write` | yes | no | yes | no | no |
| `memory.status` | `memory` | Change memory status | `status` | `memory_set_status` | `/memory/status` | `stable` | `project_write` | yes | no | yes | no | no |
| `memory.delete` | `memory` | Delete a memory card | `delete` | `memory_delete` | `/memory/delete` | `stable` | `project_maintenance` | yes | no | yes | yes | no |
| `memory.feedback` | `memory` | Record retrieval usefulness feedback | `feedback` | `memory_feedback` | `/feedback` | `stable` | `project_write` | yes | no | no | no | no |
| `memory.doctrine` | `memory` | Read active project decisions | `doctrine` | `memory_doctrine` | `/doctrine` | `stable` | `project_read` | no | no | yes | no | no |
| `memory.evidence` | `memory` | Read provenance for one memory card | `evidence` | `memory_evidence` | `/evidence` | `stable` | `project_read` | no | no | yes | no | no |
| `evidence.observe` | `evidence` | Record a bitemporal evidence observation | `observe` | `memory_observe` | — | `preview` | `project_write` | yes | no | no | no | no |
| `evidence.list` | `evidence` | Read evidence observations as-of two times | `observations` | `memory_observations` | — | `preview` | `project_read` | no | no | yes | no | no |
| `graph.temporal` | `graph` | Read the bitemporal memory graph | `temporal-graph` | `memory_temporal_graph` | — | `preview` | `project_read` | no | no | yes | no | no |
| `evidence.autopilot` | `evidence` | Create reversible bitemporal evidence from explicit durable-id references | `evidence-autopilot` | `memory_evidence_autopilot` | `/evidence-autopilot`<br>`/evidence-autopilot/apply`<br>`/evidence-autopilot/rollback` | `preview` | `project_write` | yes | yes | no | yes | no |
| `memory.drift` | `memory` | Detect memory drift against project files | `drift` | `memory_drift` | `/drift` | `stable` | `project_filesystem` | no | no | yes | no | yes |
| `retrieval.brief` | `retrieval` | Build a tiny verified task brief | `brief` | `memory_brief` | `/brief` | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.impact` | `retrieval` | Find memory relevant to a target | `impact` | `memory_impact` | `/impact` | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.context` | `retrieval` | Build a bounded context pack | `context-pack` | `memory_context_pack` | — | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.agent_context` | `retrieval` | Build agent-native project context | `context` | `memory_agent_context` | — | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.budget_plan` | `retrieval` | Choose the smallest useful context budget | `budget-plan` | `memory_budget_plan` | `/budget-plan` | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.recall` | `retrieval` | Return compressed temporal recall | `recall` | `memory_recall` | `/recall` | `stable` | `project_read` | no | no | yes | no | no |
| `retrieval.rag_answer` | `retrieval` | Answer from grounded project memory | `rag-answer` | `memory_rag_answer` | — | `stable` | `project_read` | no | no | yes | no | yes |
| `retrieval.graph_rag_answer` | `retrieval` | Answer with graph-expanded evidence | `graph-rag` | `memory_graph_rag_answer` | — | `preview` | `project_read` | no | no | yes | no | yes |
| `memory.doctor` | `operations` | Run compact memory health checks | `doctor` | `memory_doctor` | `/doctor` | `stable` | `project_read` | no | no | yes | no | no |
| `control.status` | `control` | Read the cached project control snapshot | — | `memory_status` | — | `stable` | `project_read` | no | no | yes | no | no |
| `control.should_write` | `control` | Decide whether a durable memory write is warranted | — | `memory_should_write` | — | `stable` | `project_read` | no | no | yes | no | no |
| `control.after_task` | `control` | Return after-task memory guidance | — | `memory_after_task` | — | `stable` | `project_read` | no | no | yes | no | no |
| `control.project_health` | `control` | Read compact project memory health | — | `memory_project_health` | — | `stable` | `project_read` | no | no | yes | no | no |
| `rag.ingest` | `rag` | Index local source files | `rag-ingest` | `memory_rag_ingest` | `/rag-ingest` | `stable` | `project_filesystem` | yes | yes | yes | no | yes |
| `rag.sources` | `rag` | Inspect indexed RAG sources | `rag-sources` | `memory_rag_sources` | `/rag-sources` | `stable` | `project_read` | no | no | yes | no | no |
| `rag.eval` | `rag` | Evaluate grounded RAG retrieval | `eval rag` | `memory_rag_eval` | `/rag-eval` | `stable` | `project_maintenance` | yes | no | yes | no | no |
| `rag.graph_eval` | `rag` | Evaluate graph-RAG relationships | `eval graph-rag` | `memory_graph_rag_eval` | `/graph-rag-eval` | `preview` | `project_read` | no | no | yes | no | no |
| `memory.advanced_eval` | `evaluation` | Audit causal, poisoning, global, and temporal memory signals | `eval advanced` | `memory_advanced_eval` | `/advanced-eval` | `preview` | `project_read` | no | no | yes | no | no |
| `release.gate_v2` | `release` | Run V2 release readiness checks | `release-gate-v2` | `memory_release_gate_v2` | `/release-gate-v2` | `deprecated` | `project_maintenance` | yes | no | yes | no | yes |
| `release.gate_v3` | `release` | Run V3 release readiness checks | `release-gate-v3` | `memory_release_gate_v3` | `/release-gate-v3` | `stable` | `project_maintenance` | yes | no | yes | no | yes |
| `deployment.profile` | `deployment` | Validate local or reverse-proxy deployment security and observability | `deployment-profile` | `memory_deployment_profile` | `/deployment-profile` | `preview` | `project_filesystem` | no | no | yes | no | yes |
| `agent_session.start` | `agent_session` | Start an evidence-backed session | `agent-session start` | `memory_session_start` | `/agent-sessions/start` | `stable` | `project_write` | yes | no | no | no | no |
| `agent_session.context` | `agent_session` | Load audited session context | `agent-session context` | `memory_session_context` | `/agent-sessions/context` | `stable` | `project_read` | no | no | yes | no | no |
| `agent_session.claim` | `agent_session` | Claim a worker lease | `agent-session claim` | `memory_session_claim` | `/agent-sessions/claim` | `stable` | `project_write` | yes | no | no | no | no |
| `agent_session.renew` | `agent_session` | Renew a worker lease | `agent-session renew` | `memory_session_renew` | `/agent-sessions/renew` | `stable` | `project_write` | yes | no | no | no | no |
| `agent_session.release` | `agent_session` | Release a worker lease | `agent-session release` | `memory_session_release` | `/agent-sessions/release` | `stable` | `project_write` | yes | no | no | no | no |
| `agent_session.event` | `agent_session` | Record a retry-safe lifecycle event | `agent-session event` | `memory_session_event` | `/agent-sessions/event` | `stable` | `project_write` | yes | no | yes | no | no |
| `agent_session.recover` | `agent_session` | Inspect or claim stale sessions | `agent-session recover` | `memory_session_recover` | `/agent-sessions/recover` | `stable` | `project_write` | yes | yes | no | no | no |
| `agent_session.finish` | `agent_session` | Finish a session with evidence | `agent-session finish` | `memory_session_finish` | `/agent-sessions/finish` | `stable` | `project_write` | yes | no | no | no | no |
| `agent_session.status` | `agent_session` | Inspect session status | `agent-session status` | `memory_session_status` | `/agent-sessions` | `stable` | `project_read` | no | no | yes | no | no |
| `agent_session.trace` | `agent_session` | Trace memory influence to outcome | `agent-session trace` | `memory_session_trace` | `/agent-sessions/trace` | `stable` | `project_read` | no | no | yes | no | no |
| `agent_session.cleanup` | `agent_session` | Apply session retention policy | `agent-session cleanup` | `memory_session_cleanup` | `/agent-sessions/cleanup` | `stable` | `project_maintenance` | yes | yes | yes | yes | no |
