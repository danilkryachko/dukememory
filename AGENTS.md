<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tools** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them. `codegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

<!-- DUKEMEMORY_START -->
## dukememory.

This repository has local project memory installed in `.agent/memory.db`.

For every new chat or coding task in this repository:
- Use the Codex skill `$dukememory-use` when available.
- Confirm the current project root before reading or writing memory; never write this project's durable facts into another `.agent/memory.db`.
- Default to read-only memory use unless there is a durable fact worth saving.
- Start with project memory before broad exploration. Prefer MCP `memory_brief` with a tiny budget for the user's task.
- Memory use is mandatory when `.agent/memory.db` exists: if MCP tools are not available, use the CLI fallback from the project root instead of skipping memory.
- CLI brief fallback: `dukememory brief "<task>" --budget-profile tiny`.
- When a touched file, symbol, subsystem, command, UI area, or error is known, call MCP `memory_impact` or run `dukememory impact <target> --budget-profile tiny`.
- For architectural/policy questions, call MCP `memory_doctrine`; use MCP `memory_evidence` for critical memory ids before relying on them.
- Before broad edits/refactors/dependency changes/schema changes/release work, call MCP `memory_drift` or run `dukememory drift --root .`.
- Persist only durable decisions, constraints, user preferences, project commands, known issues, and task state with MCP `memory_remember`/`memory_add` or the `dukememory remember`/`dukememory add` CLI.
- Before adding decisions, check MCP `memory_doctrine` or `dukememory doctrine --json`; use MCP `memory_evidence` for high-impact or surprising memory before relying on it.
- Do not save transient scratch notes, large logs, secrets, full file dumps, or obvious facts from nearby code.
- After a batch of important memory writes, run `dukememory embed-index` once so embeddings stay ready.
- Keep operational memory compact. Do not create recursive "compact of compact" summaries; prefer `dukememory memory-contract --write` for durable project-wide context.
- Memory maintenance is autonomous by default: `dukememory autonomous run-once --level normal` may refresh embeddings, backups, cleanup, safe inbox approvals, compact stale operational notes, and supersede safe duplicates without hard deletion.
- Roll back the last autonomous maintenance cycle with `dukememory autonomous rollback`; autonomous mode must keep rollback metadata and avoid hard delete by default.
- Before the final response after substantial work, run the same end routine: save useful durable outcomes or task state, then refresh embeddings once after writes.
- If memory was read or written, the final response must include a short human-readable receipt in the user's language. English example: `Memory: read brief+impact; matched 6 cards; saved task_state abc123.` Russian example: `Память: прочитал brief+impact по 6 карточкам; сохранил task_state abc123.` If nothing durable was saved, say that naturally in the user's language. Do not paste long raw id lists.
- To inspect whether memory is being used and reused, run `dukememory usage-report --since-days 7`.
- To inspect memory quality and cleanup candidates, run `dukememory usefulness-report`.
- To inspect autonomous maintenance, run `dukememory autonomous status --json`.
- To inspect evidence-backed memory quality, run `dukememory quality-report --json`.
- To inspect memory ROI and write pressure, run `dukememory roi-report --json`.
- To inspect whether agents follow memory discipline, run `dukememory agent-audit --json`.
- To explain which memory cards influenced recent agent behavior, run `dukememory decision-trace --json`.
- To materialize autonomous inferred feedback, run `dukememory auto-feedback --json` or preview with `--dry-run`.
- To keep memory token-light, run `dukememory cost-guard --json`.
- To choose the smallest full read flow, run `dukememory context-governor "<task>" --json`; add `--target <file-or-symbol>` before focused edits.
- To choose the smallest useful context budget, run `dukememory budget-plan "<task>" --json`.
- To route memory across nearby projects without mixing facts, run `dukememory memory-router "<query>" --include-siblings --json`; treat non-current routes as advisory.
- To inspect one end-to-end project memory health score, run `dukememory memory-health-score --json`.
- To explain why specific cards would be recalled, run `dukememory explain-recall "<query>" --json`.
- To inspect goals, decisions, constraints, commands, risks, active tasks, and the compact contract, run `dukememory project-intent-map --json`.
- To run lightweight retrieval quality probes against durable memory, run `dukememory memory-test-harness --json`.
- To audit read discipline, semantic effectiveness, write pressure, feedback, and explainability, run `dukememory agent-audit-v2 --json`.
- To aggregate health, intent, probes, audit, recall explanations, and autonomy, run `dukememory memory-control-center-v2 --json`.
- To safely supersede duplicate/obsolete cards, run `dukememory auto-supersede-v2 --json`; use `--apply` only for high-confidence reversible status changes.
- To write high-confidence changed-file memory candidates, run `dukememory memory-diff-apply --json`; use `--apply` only after reviewing write-ready cards.
- To detect retrieval regressions, run `dukememory recall-benchmark-suite --json`; use `--write-baseline` after reviewing stable probes.
- To gate releases with health, recall benchmark, audit v2, and control-center checks, run `dukememory release-gate-v2 --json`.
- To configure local-first VDS/remote sync safely, run `dukememory remote-sync-wizard --json`; use `--target` and `DUKEMEMORY_SYNC_PASSPHRASE` before `--apply`.
- To inspect or write autonomous memory governance policy, run `dukememory memory-governance-policy --json`; use `--apply` to write `.agent/memory-governance.json`.
- To run the V2 autonomous memory loop with governance and quality gates, run `dukememory autonomous-loop-v2 --json`; use `--apply` only when governance is ready.
- To enforce autonomous memory governance, run `dukememory governance-enforce --json`; use `--apply` to log a clean enforcement pass.
- To run a CI-friendly memory quality gate, run `dukememory memory-quality-ci --json`.
- To inspect all discovered project memories with V2 quality metrics, run `dukememory fleet-dashboard-v2 --json`.
- To plan guarded remote sync apply, run `dukememory remote-sync-apply-flow --json`; use `--target` and `DUKEMEMORY_SYNC_PASSPHRASE` before `--apply`.
- To inspect MCP V2 memory tool exposure, run `dukememory mcp-tool-surface-v2 --json`.
- To run the V3 autonomous memory autopilot, run `dukememory autopilot-v3 --json`; use `--apply` for guarded reversible actions.
- To tune retrieval from live usefulness, run `dukememory self-learning-retrieval --json`; use `--apply` to write the selected ranking profile.
- To detect/apply project-specific memory defaults, run `dukememory project-role-profile --json`; use `--apply` after reviewing inferred kind.
- To review inbox suggestions with confidence explanations, run `dukememory inbox-ai-reviewer --json`; use `--apply` only for safe high-confidence groups.
- To inspect the simplified web control model, run `dukememory web-control-center-v3 --json`.
- To apply guarded local-first remote sync planning, run `dukememory remote-sync-apply --json`; use `--target` and `DUKEMEMORY_SYNC_PASSPHRASE` before `--apply`.
- To inspect MCP helper tools for memory discipline, run `dukememory mcp-quality-tools --json`.
- To inspect local-first VDS/remote sync readiness and real push/pull dry-runs, run `dukememory remote-sync-control --json`; pass `--target PATH` for target status.
- To inspect the actionable web control model, run `dukememory web-control-center-v4 --json`.
- To enforce startup/write/after-task memory discipline, run `dukememory mcp-discipline-v2 --json`; use `--apply` to repair wiring.
- To inspect autonomous usefulness feedback, safe supersede, diff apply, and recall benchmark quality, run `dukememory feedback-loop-v2 --json`.
- To inspect all installed project memories with richer version/action summaries, run `dukememory upgrade-all-projects-v2 --json`.
- To inspect a local-first VDS sync pack with dry-run/apply/verify commands, run `dukememory vds-sync-pack --json`; pass `--target PATH` before `--apply`.
- To inspect the 0.24 web control model, run `dukememory web-control-center-v5 --json`.
- To inspect safe feedback, quality, cost, health, diff apply, supersede, and benchmark gates, run `dukememory quality-autopilot-v31 --json`.
- To route cross-project memory without writing outside the current project, run `dukememory memory-router-v2 "<query>" --include-siblings --json`.
- To select project-aware retrieval benchmark profiles, run `dukememory benchmark-profiles --json`.
- To inspect README, screenshot, license, package metadata, and GitHub install readiness, run `dukememory install-polish --json`.
- To measure whether recent memory reads actually helped agent work, run `dukememory memory-effectiveness-lab --json`.
- To choose the smallest useful memory flow for a task, run `dukememory auto-context-budgeter-v2 "<task>" --json`.
- To inspect or write the compact project contract v2, run `dukememory memory-contract-v2 --json`; use `--write` after releases or architecture changes.
- To surface sibling-project hints without writing outside the current project, run `dukememory cross-project-learning "<query>" --json`.
- To inspect recent agent reads, influence, feedback, and durable writes, run `dukememory agent-trace --json`.
- To verify local-first VDS sync target, latency, dry-runs, and rollback readiness, run `dukememory vds-sync-hardening --json`.
- To verify install, skill, AGENTS, doctor, and future-chat memory readiness, run `dukememory install-quality --json`.
- To inspect the 0.25 web control model, run `dukememory web-control-center-v6 --json`.
- To answer from grounded project memory with citations, run `dukememory answer "<question>" --json`.
- To verify or repair Codex future-chat memory wiring, run `dukememory connect-codex --json`; use `--apply` after review.
- To explain memory card types, filters, and guardrails, run `dukememory memory-type-guide --json`.
- To inspect reproducible local recall/effectiveness evaluation, run `dukememory memory-eval-story --json`.
- To turn a text file into reviewed inbox candidates, run `dukememory import-review FILE --json`; use `--apply` only for safe durable input.
- To upload a local text/markdown/json/csv file into reviewed inbox candidates, run `dukememory memory-upload FILE --json`; use `--apply` only after reviewing the source.
- To inspect Memanto-style capability coverage without changing memory, run `dukememory memanto-gap-report --json`.
- To inspect the 0.26 web control model, run `dukememory web-control-center-v7 --json`.
- To plan autonomous memory usefulness improvements, run `dukememory autonomous-usefulness --json`; use `--apply` only for reversible feedback materialization.
- To inspect polished local benchmark evidence, run `dukememory benchmark-polish --json`.
- To inspect the 0.27 web control model, run `dukememory web-control-center-v8 --json`.
- To plan safe autonomous repair, run `dukememory autonomous-supervisor --json`; use `--apply` to run safe repairs in order.
- To inspect the 0.28 web control model, run `dukememory web-control-center-v9 --json`.
- To plan safe autonomous repair across all discovered project memories, run `dukememory fleet-supervisor --json`; use `--apply` for reversible fleet maintenance.
- To inspect the 0.29 web control model, run `dukememory web-control-center-v10 --json`.
- To preview periodic fleet maintenance, run `dukememory fleet-supervisor-watch-install --dry-run --json`; omit `--dry-run` to write the launchd plist.
- To inspect the 0.30 web control model, run `dukememory web-control-center-v11 --json`.
- To get compressed token-light recall, run `dukememory recall "<task>" --max-chars 1200`; use `--recent`, `--as-of YYYY-MM-DD`, `--as-of-days-ago N`, `--changed-since YYYY-MM-DD`, or `--changed-since-days N` for temporal recall.
- To inspect one memory card's facts, audit events, and real agent read influence, run `dukememory memory-timeline <memory-id> --json`.
- To review duplicate, stale, active-superseded, and contradiction-prone memory groups without mutating memory, run `dukememory memory-conflict-review --json`.
- To inspect live memory usefulness from reads and feedback, run `dukememory eval live --json`.
- To inspect all local projects, run `dukememory dashboard --json`.
- To inspect the full memory intelligence surface, run `dukememory intelligence-dashboard --json`.
- To diff changed files against memory links and stale facts, run `dukememory project-diff --changed-only --json`.
- To preview local-first VDS/remote sync readiness, run `dukememory remote-sync-dry-run --json`.
- To verify installed project memory wiring, run `dukememory doctor-project --json`.
- To repair installed project memory wiring, run `dukememory doctor-project --fix --json`.
- To run a local release readiness gate, run `dukememory release-gate --json`; use `--run` when it should execute fmt/check/test/build.
- To replay recent memory influence, run `dukememory memory-replay --json`.
- To inspect or repair all installed project memories, run `dukememory project-watch --json` or `dukememory project-watch --fix --json`.
- To run one autonomous memory control loop, inspect with `dukememory autonomous-loop --json`; apply reversible fixes with `dukememory autonomous-loop --apply --json`.
- To run the same loop periodically, use `dukememory autonomous-loop --watch --apply --interval-secs 3600 --json`.
- To install a local launchd watch plist without guessing shell setup, preview with `dukememory autonomous-watch-install --dry-run --json`.
- To inspect autonomous actions, skipped work, failures, and rollback availability, run `dukememory action-journal --json`.
- To rank useful/noisy memory and materialize safe inferred feedback, run `dukememory usefulness-engine --json` or `dukememory usefulness-engine --apply --json`.
- To choose retrieval strictness, run `dukememory ranking-profile --profile balanced|strict|recall-heavy|precision-heavy --json`; use `--apply` only for durable project policy.
- To adapt retrieval strictness from live quality signals, run `dukememory auto-ranking-tune --json`; use `--apply` only for durable project policy.
- To seed project-type defaults, run `dukememory project-template --kind rust-cli|frontend-app|game-mod|electronics-cad|docs-research --json`; use `--apply` only after review.
- To inspect or enable the autonomous watch loop, run `dukememory watch-control --json`; use `--apply` only when launchd should be updated.
- To inspect the autonomy cockpit, run `dukememory autonomy-control-center --json`.
- To measure local/VDS sync latency while keeping reads local-first, run `dukememory sync-latency --json`.
- To choose a safe sync mode, run `dukememory sync-profile --profile local-first-backup --run-dry-run --json` before push/pull.
- To enforce memory wiring for future chats, run `dukememory agent-enforce --json` or `dukememory agent-enforce --fix --json`.
- To review changed files for durable memory updates, run `dukememory memory-diff-review --json`.
- To plan encrypted local-first VDS/remote sync, run `dukememory remote-sync-v2 --json`; use `--target` and `DUKEMEMORY_SYNC_PASSPHRASE` before `--apply`.
- To sync memory safely, preview first with `dukememory sync export bundle.json --dry-run --json` and `dukememory sync import bundle.json --policy manual --dry-run --json`.
- To use a local-first remote/VDS connector, run `dukememory sync push TARGET --dry-run --json`, `dukememory sync pull TARGET --dry-run --json`, and `dukememory sync status TARGET --json`.
- To safely group and process inbox suggestions, run `dukememory inbox-v2 report --json`.
- To check whether memory is useful or noisy, run `dukememory memory-qa --json`.
- To refresh project-wide memory instructions and the compact contract, run `dukememory upgrade-project --json`.
- To refresh all discovered project memories, run `dukememory upgrade-all-projects --json`.
- After a task, agents may record lightweight memory utility feedback with `dukememory feedback --id <memory-id> --rating useful|useless|missing`.

Keep memory use lightweight: prefer `brief`/`impact`; do not dump large context packs unless needed.
<!-- DUKEMEMORY_END -->
