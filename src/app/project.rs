use super::*;

const MEMORY_CONTRACT_MAX_CHARS: usize = 1100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OnboardReport {
    ok: bool,
    root: String,
    db: String,
    actions: Vec<String>,
    profile: ProjectProfileSnapshot,
    embedding: Option<EmbeddingIndexReport>,
    autonomous_plist: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryContractReport {
    pub(crate) version: u32,
    pub(crate) root: String,
    pub(crate) path: String,
    pub(crate) written: bool,
    pub(crate) memory_id: Option<String>,
    pub(crate) max_chars: usize,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeProjectReport {
    version: String,
    ok: bool,
    root: String,
    dry_run: bool,
    actions: Vec<String>,
    install: Option<InstallUpdateReport>,
    install_ux: InstallUxSummary,
    qa: Option<MemoryQaReport>,
    contract: Option<MemoryContractReport>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallUxSummary {
    binary_target: String,
    codex_skill: String,
    agents_block: bool,
    project_config: bool,
    embedding_ready: bool,
    future_chats_ready: bool,
    mcp_note: String,
    next_steps: Vec<String>,
}

pub(crate) fn print_onboard(
    root: &Path,
    install_autonomous: bool,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = onboard_project(root, install_autonomous, provider, endpoint, model)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("onboard: {}", if report.ok { "ok" } else { "warn" });
        println!("root: {}", report.root);
        for action in report.actions {
            println!("- {action}");
        }
    }
    Ok(())
}

pub(crate) fn onboard_project(
    root: &Path,
    install_autonomous: bool,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<OnboardReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    fs::create_dir_all(root.join(".agent"))?;
    let db = root.join(".agent").join("memory.db");
    let conn = open_db(&db)?;
    let mut actions = Vec::new();
    write_project_config(
        &root.join(".agent").join("config.toml"),
        &db,
        provider,
        endpoint,
        model,
    )?;
    actions.push("config".to_string());
    write_workspace_rules(&root, true)?;
    upsert_project_agents(&root)?;
    actions.push("workspace_init".to_string());
    let profile = project_profile_snapshot(&conn, &root, "project")?;
    if profile.memory_count == 0 {
        let title = format!(
            "{} project memory profile",
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Project")
        );
        add_memory(
            &conn,
            AddMemory {
                id: None,
                memory_type: "product_goal".to_string(),
                title,
                body: "Project memory was onboarded automatically; future agents should keep recall token-light and save only durable decisions, constraints, commands, risks, and task state.".to_string(),
                scope: "project".to_string(),
                status: "active".to_string(),
                source: Some("onboard".to_string()),
                supersedes: None,
                confidence: 0.8,
                links: Vec::new(),
            },
        )?;
        actions.push("seed_project_goal".to_string());
    }
    let embedding =
        embeddings::embed_index(&conn, provider, endpoint, model, &[], None, false).ok();
    if embedding.is_some() {
        actions.push("embed_index".to_string());
    }
    let autonomous_plist = if install_autonomous {
        let output = expand_tilde("~/Library/LaunchAgents/com.dukememory.autonomous.plist");
        write_autonomous_launchd_plist(AutonomousLaunchdRequest {
            db: &db,
            output: &output,
            level: AutonomousLevel::Normal,
            interval_secs: 300,
            status_file: &root.join(".agent").join("autonomous-status.json"),
            rollback_dir: &root.join(".agent").join("autonomous-rollbacks"),
            backup_dir: &root.join(".agent").join("backups"),
            provider,
            endpoint,
            model,
            force: true,
            dry_run: false,
        })?;
        actions.push("autonomous_install".to_string());
        Some(output.display().to_string())
    } else {
        None
    };
    Ok(OnboardReport {
        ok: true,
        root: root.display().to_string(),
        db: db.display().to_string(),
        actions,
        profile: project_profile_snapshot(&conn, &root, "project")?,
        embedding,
        autonomous_plist,
    })
}

pub(crate) fn discover_project_dbs(default_db: &Path) -> Result<Vec<PathBuf>> {
    let mut dbs = Vec::new();
    app_push_unique_db(&mut dbs, default_db);
    if let Some(root) = app_project_root_for_db(default_db)
        && let Some(parent) = root.parent()
    {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let candidate = entry.path().join(".agent").join("memory.db");
                if candidate.exists() {
                    app_push_unique_db(&mut dbs, &candidate);
                }
            }
        }
    }
    Ok(dbs)
}

pub(crate) fn print_memory_contract(
    conn: &Connection,
    root: &Path,
    write: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_contract_report(conn, root, write)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.content);
        if report.written {
            println!("written: {}", report.path);
        }
        if let Some(id) = &report.memory_id {
            println!("memory_id: {id}");
        }
    }
    Ok(())
}

pub(crate) fn memory_contract_report(
    conn: &Connection,
    root: &Path,
    write: bool,
) -> Result<MemoryContractReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = root.join(".agent").join("MEMORY_CONTRACT.md");
    let content = render_memory_contract(conn, &root, MEMORY_CONTRACT_MAX_CHARS)?;
    let mut memory_id = None;
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_file(&path, content.as_bytes())?;
        memory_id = Some(upsert_memory_contract_card(conn, &content)?);
    }
    Ok(MemoryContractReport {
        version: 1,
        root: root.display().to_string(),
        path: path.display().to_string(),
        written: write,
        memory_id,
        max_chars: MEMORY_CONTRACT_MAX_CHARS,
        content,
    })
}

fn render_memory_contract(conn: &Connection, root: &Path, max_chars: usize) -> Result<String> {
    let profile = project_profile_snapshot(conn, root, "project")?;
    let mut out = String::new();
    out.push_str("# dukememory. Project Contract\n\n");
    out.push_str(&format!("Root: {}\n", root.display()));
    out.push_str(&format!(
        "Memory: {} cards, {} decisions, {} constraints, {} commands, recommended budget: {}.\n",
        profile.memory_count,
        profile.decisions,
        profile.constraints,
        profile.commands,
        profile.recommended_budget
    ));
    out.push_str(&format!(
        "Embeddings: provider={}, model={}\n\n",
        profile.embedding_provider, profile.embedding_model
    ));
    out.push_str("Rules:\n");
    out.push_str("- Start with `dukememory brief \"<task>\" --budget-profile tiny`.\n");
    out.push_str(
        "- Use `dukememory impact <target> --budget-profile tiny` before focused edits.\n",
    );
    out.push_str("- Save only durable decisions, constraints, commands, risks, and task state.\n");
    out.push_str("- Keep recall small; use `dukememory recall \"<task>\" --max-chars 1200` only when brief is not enough.\n");
    out.push_str("- Autonomous maintenance is allowed only when reversible.\n\n");
    append_contract_section(conn, &mut out, "Goals", &["product_goal".to_string()], 2)?;
    append_contract_section(
        conn,
        &mut out,
        "Decisions",
        &["decision".to_string(), "constraint".to_string()],
        3,
    )?;
    append_contract_section(conn, &mut out, "Commands", &["command".to_string()], 3)?;
    append_contract_section(
        conn,
        &mut out,
        "Known Risks",
        &["known_issue".to_string()],
        3,
    )?;
    Ok(truncate_chars(&out, max_chars))
}

fn append_contract_section(
    conn: &Connection,
    out: &mut String,
    title: &str,
    types: &[String],
    limit: usize,
) -> Result<()> {
    let rows = query_memories(
        conn,
        None,
        types,
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        limit,
    )?;
    if rows.is_empty() {
        return Ok(());
    }
    out.push_str(title);
    out.push_str(":\n");
    for row in rows {
        out.push_str(&format!(
            "- {} [{}]: {}\n",
            truncate_chars(&row.title, 72),
            row.memory_type,
            truncate_chars(&one_line_summary(&row.body), 140)
        ));
    }
    out.push('\n');
    Ok(())
}

fn upsert_memory_contract_card(conn: &Connection, content: &str) -> Result<String> {
    let existing = query_memories(
        conn,
        None,
        &["design_note".to_string()],
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        100,
    )?
    .into_iter()
    .find(|memory| memory.title == "Project memory contract");
    if let Some(memory) = existing {
        conn.execute(
            r#"
            UPDATE memories SET
                type = 'design_note',
                scope = 'project',
                title = 'Project memory contract',
                body = ?1,
                status = 'active',
                source = 'memory_contract',
                updated_at = ?2,
                confidence = 0.95
            WHERE id = ?3
            "#,
            params![content, now_ms(), memory.id],
        )?;
        conn.execute(
            "DELETE FROM memory_links WHERE memory_id = ?1",
            params![memory.id],
        )?;
        insert_links(
            conn,
            &memory.id,
            &parse_links(&["file:.agent/MEMORY_CONTRACT.md".to_string()])?,
        )?;
        log_event(
            conn,
            "memory_updated",
            Some(&memory.id),
            "updated project memory contract",
        )?;
        Ok(memory.id)
    } else {
        add_memory(
            conn,
            AddMemory {
                id: None,
                memory_type: "design_note".to_string(),
                title: "Project memory contract".to_string(),
                body: content.to_string(),
                scope: "project".to_string(),
                status: "active".to_string(),
                source: Some("memory_contract".to_string()),
                supersedes: None,
                confidence: 0.95,
                links: vec!["file:.agent/MEMORY_CONTRACT.md".to_string()],
            },
        )
    }
}

pub(crate) fn print_upgrade_project(
    conn: &Connection,
    root: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = upgrade_project_report(conn, root, from, to, backup_dir, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("upgrade_project: {}", if report.ok { "ok" } else { "warn" });
        for action in &report.actions {
            println!("- {action}");
        }
        for error in &report.errors {
            println!("error: {error}");
        }
    }
    Ok(())
}

pub(crate) fn upgrade_project_report(
    conn: &Connection,
    root: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
) -> Result<UpgradeProjectReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    let mut install = None;
    let source = from
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_exe()?);
    let target = resolve_install_target(to);
    let same_binary = source
        .canonicalize()
        .ok()
        .zip(target.canonicalize().ok())
        .is_some_and(|(source, target)| source == target);
    if same_binary {
        actions.push(format!("binary already running from {}", target.display()));
    } else {
        match update_install(
            Some(&source),
            to,
            backup_dir,
            DEFAULT_INSTALL_BACKUP_KEEP,
            dry_run,
        ) {
            Ok(report) => {
                actions.push(if report.changed {
                    format!("binary update prepared: {}", report.target)
                } else {
                    format!("binary already current: {}", report.target)
                });
                install = Some(report);
            }
            Err(err) => errors.push(format!("binary update failed: {err}")),
        }
    }
    if dry_run {
        actions.push("would refresh .agent/config.toml".to_string());
        actions.push("would refresh workspace rules and AGENTS.md".to_string());
        actions.push("would refresh Codex skill".to_string());
        actions.push("would write memory contract".to_string());
    } else {
        let db = root.join(".agent").join("memory.db");
        let (provider, endpoint, model) = read_project_embedding_config(&root);
        match write_project_config(
            &root.join(".agent").join("config.toml"),
            &db,
            &provider,
            &endpoint,
            &model,
        ) {
            Ok(()) => actions.push(format!(
                "project config refreshed: {}",
                root.join(".agent").join("config.toml").display()
            )),
            Err(err) => errors.push(format!("project config refresh failed: {err}")),
        }
        match write_workspace_rules(&root, true) {
            Ok(path) => actions.push(format!("workspace rules refreshed: {}", path.display())),
            Err(err) => errors.push(format!("workspace rules failed: {err}")),
        }
        match upsert_project_agents(&root) {
            Ok(()) => actions.push("AGENTS.md dukememory. block refreshed".to_string()),
            Err(err) => errors.push(format!("AGENTS.md refresh failed: {err}")),
        }
        match write_codex_skill(&expand_tilde("~/.codex/skills"), true) {
            Ok(path) => actions.push(format!("Codex skill refreshed: {}", path.display())),
            Err(err) => errors.push(format!("Codex skill refresh failed: {err}")),
        }
    }
    let contract = match memory_contract_report(conn, &root, !dry_run) {
        Ok(report) => {
            if report.written {
                actions.push(format!("memory contract written: {}", report.path));
            }
            Some(report)
        }
        Err(err) => {
            errors.push(format!("memory contract failed: {err}"));
            None
        }
    };
    let qa = match memory_qa_report(conn, &root, 7) {
        Ok(report) => Some(report),
        Err(err) => {
            errors.push(format!("memory qa failed: {err}"));
            None
        }
    };
    let agents_content = fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let skill_path = expand_tilde("~/.codex/skills/dukememory-use/SKILL.md");
    let skill_content = fs::read_to_string(&skill_path).unwrap_or_default();
    let project_config = root.join(".agent/config.toml").exists();
    let agents_block = agents_content.contains("DUKEMEMORY_START")
        && agents_content.contains("agent-enforce")
        && agents_content.contains("autonomous-loop");
    let skill_ready = skill_content.contains("dukememory-use")
        && skill_content.contains("agent-enforce")
        && skill_content.contains("sync-latency");
    let embedding_ready = qa
        .as_ref()
        .is_some_and(|report| report.embedding_missing == 0 && report.embedding_stale == 0);
    let future_chats_ready = project_config && agents_block && skill_ready && embedding_ready;
    let mut next_steps = Vec::new();
    if !future_chats_ready {
        next_steps.push("run dukememory agent-enforce --fix --json".to_string());
    }
    if !embedding_ready {
        next_steps.push("run dukememory embed-index".to_string());
    }
    next_steps
        .push("restart Codex if MCP tools were already loaded before this upgrade".to_string());
    let install_ux = InstallUxSummary {
        binary_target: target.display().to_string(),
        codex_skill: skill_path.display().to_string(),
        agents_block,
        project_config,
        embedding_ready,
        future_chats_ready,
        mcp_note: "AGENTS and skill are refreshed immediately; MCP tool lists may require a Codex restart to reload.".to_string(),
        next_steps,
    };
    let ok = errors.is_empty() && qa.as_ref().is_none_or(|report| report.ok || dry_run);
    Ok(UpgradeProjectReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        ok,
        root: root.display().to_string(),
        dry_run,
        actions,
        install,
        install_ux,
        qa,
        contract,
        errors,
    })
}

pub(crate) fn workspace_init(root: &Path, force: bool) -> Result<()> {
    let rules = write_workspace_rules(root, force)?;
    upsert_project_agents(root)?;
    println!("{}", rules.display());
    Ok(())
}

pub(crate) fn write_workspace_rules(root: &Path, force: bool) -> Result<PathBuf> {
    let agent_dir = root.join(".agent");
    fs::create_dir_all(&agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;
    let rules = agent_dir.join("rules.rhai");
    if rules.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            rules.display()
        );
    }
    write_file(
        &rules,
        br#"fn score_memory(type, status, scope, title, body, task, confidence) {
  if status == "active" { confidence * 2.0 } else { 0.0 }
}

fn should_include(type, status, scope, title, body, task, confidence) {
  status != "rejected"
}

fn should_redact(type, status, scope, title, body, confidence) {
  body.contains("api_key") || body.contains("token =")
}
"#,
    )?;
    Ok(rules)
}

pub(crate) fn project_root_from_config(config: &Path) -> Option<PathBuf> {
    let parent = config.parent()?;
    if parent.file_name().and_then(|value| value.to_str()) == Some(".agent") {
        parent.parent().map(Path::to_path_buf)
    } else {
        parent.canonicalize().ok()
    }
}

pub(crate) fn upsert_project_agents(root: &Path) -> Result<()> {
    let path = root.join("AGENTS.md");
    let block = project_memory_agents_block();
    let content = if path.exists() {
        let current = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if current.contains("<!-- DUKEMEMORY_START -->") {
            replace_marked_block(
                &current,
                "<!-- DUKEMEMORY_START -->",
                "<!-- DUKEMEMORY_END -->",
                block,
            )
        } else if current.trim().is_empty() {
            block.to_string()
        } else {
            format!("{}\n\n{}", current.trim_end(), block)
        }
    } else {
        block.to_string()
    };
    let content = if content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    };
    write_file(&path, content.as_bytes())?;
    Ok(())
}

fn replace_marked_block(content: &str, start: &str, end: &str, replacement: &str) -> String {
    let Some(start_pos) = content.find(start) else {
        return format!("{}\n\n{}", content.trim_end(), replacement);
    };
    let Some(end_rel) = content[start_pos..].find(end) else {
        return format!("{}\n\n{}", content.trim_end(), replacement);
    };
    let end_pos = start_pos + end_rel + end.len();
    let mut out = String::new();
    out.push_str(content[..start_pos].trim_end());
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(replacement);
    let tail = content[end_pos..].trim_start();
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    out
}

fn project_memory_agents_block() -> &'static str {
    r#"<!-- DUKEMEMORY_START -->
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
- To inspect the 0.26 web control model, run `dukememory web-control-center-v7 --json`.
- To plan autonomous memory usefulness improvements, run `dukememory autonomous-usefulness --json`; use `--apply` only for reversible feedback materialization.
- To inspect polished local benchmark evidence, run `dukememory benchmark-polish --json`.
- To inspect the 0.27 web control model, run `dukememory web-control-center-v8 --json`.
- To get compressed token-light recall, run `dukememory recall "<task>" --max-chars 1200`.
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
<!-- DUKEMEMORY_END -->"#
}
