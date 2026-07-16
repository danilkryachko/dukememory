# DukeMemory release evidence fixture

This file is a deterministic, non-production corpus used only by the release
evidence gate. Every marker below is synthetic and safe to publish.

- `evidence_cli_01` verifies a DukeMemory CLI workflow.
- `evidence_mcp_02` verifies an MCP memory tool workflow.
- `evidence_http_03` verifies an HTTP endpoint workflow.
- `evidence_memory_04` verifies a durable memory-card workflow.
- `evidence_source_05` verifies indexed source-chunk retrieval.
- `evidence_temporal_06` verifies temporal evidence terminology.
- `evidence_holdout_07` is an untouched release holdout marker.
- `evidence_holdout_08` is an untouched release holdout marker.
- `evidence_holdout_09` is an untouched release holdout marker.
- `evidence_holdout_10` is an untouched release holdout marker.
- `evidence_holdout_11` is an untouched release holdout marker.
- `evidence_negative_12` verifies a missing-evidence and abstention scenario label.

Русская контрольная фраза подтверждает, что multilingual retrieval remains in
the release evidence corpus. The fixture also mentions packing, nodes, edges,
and relationships so the diagnostic matrix covers graph-memory vocabulary
without creating a graph-RAG case.
