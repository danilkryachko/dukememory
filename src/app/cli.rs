use super::*;

#[derive(Parser)]
#[command(name = "dukememory")]
#[command(version)]
#[command(about = "Local structured memory for agent projects")]
pub(crate) struct Cli {
    #[arg(long, env = "DUKEMEMORY_DB", default_value = DEFAULT_DB)]
    pub(crate) db: PathBuf,

    #[arg(long, env = "DUKEMEMORY_CONFIG")]
    pub(crate) config: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Initialize .agent/ config and database.
    Init {
        #[arg(long, default_value = DEFAULT_CONFIG)]
        config: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Add a typed memory card.
    Add {
        #[arg(value_enum)]
        memory_type: MemoryType,
        title: String,
        body: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, value_enum, default_value_t = MemoryStatus::Active)]
        status: MemoryStatus,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        supersedes: Option<String>,
        #[arg(long, default_value_t = 1.0)]
        confidence: f64,
        #[arg(long = "link")]
        links: Vec<String>,
        #[arg(long)]
        allow_sensitive: bool,
    },
    /// Get one memory card.
    Get {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Update fields on an existing card.
    Update {
        id: String,
        #[arg(long = "type", value_enum)]
        memory_type: Option<MemoryType>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, value_enum)]
        status: Option<MemoryStatus>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        confidence: Option<f64>,
        #[arg(long = "link")]
        links: Vec<String>,
        #[arg(long)]
        replace_links: bool,
        #[arg(long)]
        allow_sensitive: bool,
    },
    /// Delete one memory card.
    Delete { id: String },
    /// Search memory with SQLite FTS5.
    Search {
        query: String,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long, default_value = "active")]
        status: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// List recent memories.
    List {
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Change memory status.
    Status {
        id: String,
        #[arg(value_enum)]
        status: MemoryStatus,
    },
    /// Return a small relevant memory pack.
    ContextPack {
        task: String,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long, default_value = "active,uncertain")]
        status: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long, default_value_t = 4000)]
        max_chars: usize,
        #[arg(long, value_enum)]
        budget_profile: Option<BudgetProfile>,
        #[arg(long, default_value_t = 3)]
        include_recent: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        with_codegraph: bool,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long)]
        semantic: bool,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        embed_provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        embed_endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        embed_model: String,
    },
    /// Agent-native context planner with memory, semantic recall, and CodeGraph.
    Context {
        task: String,
        #[arg(long, value_enum, default_value_t = ContextMode::Agent)]
        mode: ContextMode,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long, default_value_t = 5000)]
        max_chars: usize,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        embed_provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        embed_endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        embed_model: String,
        #[arg(long)]
        budget: Option<usize>,
        #[arg(long, value_enum)]
        budget_profile: Option<BudgetProfile>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Plain)]
        format: OutputFormat,
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// Return a tiny verified task brief for a coding agent.
    Brief {
        task: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        budget: Option<usize>,
        #[arg(long, value_enum, default_value_t = BudgetProfile::Tiny)]
        budget_profile: BudgetProfile,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Show lightweight impact memory for one file, symbol, or topic.
    Impact {
        target: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        budget: Option<usize>,
        #[arg(long, value_enum, default_value_t = BudgetProfile::Tiny)]
        budget_profile: BudgetProfile,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Export memories as JSON.
    Export {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        redact: bool,
    },
    /// Import memories from JSON export.
    Import {
        input: PathBuf,
        #[arg(long)]
        replace: bool,
    },
    /// Copy the database to a backup file.
    Backup { output: PathBuf },
    /// Replace the database with a backup file.
    Restore {
        input: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        strict: bool,
        #[arg(long, default_value = ".agent/restore-rollbacks")]
        rollback_dir: PathBuf,
        #[arg(long, default_value = ".agent/restore-journal")]
        journal_dir: PathBuf,
        #[arg(long)]
        no_rollback: bool,
    },
    /// Show memory database stats.
    Stats,
    /// Review memory quality: stale, uncertain, low confidence, duplicates.
    Review {
        #[arg(long, default_value_t = 30)]
        stale_days: i64,
        #[arg(long)]
        json: bool,
    },
    /// List active memories older than a threshold.
    Stale {
        #[arg(long, default_value_t = 30)]
        days: i64,
        #[arg(long)]
        json: bool,
    },
    /// Find likely conflicting active memories.
    Conflicts {
        #[arg(long)]
        json: bool,
    },
    /// Validate links attached to memories.
    Links {
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        validate_symbols: bool,
        #[arg(long)]
        json: bool,
    },
    /// Store a session closing summary as task_state memory.
    SessionClose {
        #[arg(long)]
        title: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        next: Vec<String>,
        #[arg(long, default_value = "thread")]
        scope: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        allow_sensitive: bool,
    },
    /// Copy the release/debug binary to a directory.
    Install {
        #[arg(long, default_value = "~/.local/bin")]
        to: String,
        #[arg(long)]
        force: bool,
    },
    /// Install the Codex skill that makes agents use dukememory automatically.
    InstallSkill {
        #[arg(long, default_value = "~/.codex/skills")]
        path: String,
        #[arg(long)]
        force: bool,
    },
    /// Safely update an installed dukememory binary.
    UpdateInstall {
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long, default_value = "~/.local/bin/dukememory")]
        to: String,
        #[arg(long, default_value = ".agent/install-backups")]
        backup_dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print optional vector-search support status.
    VecStatus,
    /// Serve a small JSON-RPC MCP-style stdio tool surface.
    ServeMcp {
        #[arg(long)]
        content_length: bool,
    },
    /// Print a compact project briefing.
    ProjectSummary {
        #[arg(long, default_value_t = 8000)]
        max_chars: usize,
        #[arg(long)]
        json: bool,
    },
    /// List active decisions.
    Decisions {
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List open questions and uncertain memory.
    OpenQuestions {
        #[arg(long)]
        json: bool,
    },
    /// List next actions from task_state cards.
    NextActions {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Apply lifecycle automation rules.
    Lifecycle {
        #[arg(long, default_value_t = 30)]
        stale_days: i64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// Scan stored memory for likely secrets.
    ScanSecrets {
        #[arg(long)]
        fix_redact: bool,
        #[arg(long)]
        json: bool,
    },
    /// Suggest memory cards from a transcript/text file.
    Suggest {
        input: PathBuf,
        #[arg(long)]
        to_inbox: bool,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        json: bool,
    },
    /// Ingest a transcript/text file into pending inbox suggestions.
    IngestTranscript {
        input: PathBuf,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        llm: bool,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_LLM_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = "qwen3:14b", env = "DUKEMEMORY_LLM_MODEL")]
        model: String,
    },
    /// Scan agent session files into pending inbox suggestions without reprocessing duplicates.
    AutoIngest {
        #[arg(long, default_value = ".agent/sessions")]
        input: PathBuf,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        llm: bool,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_LLM_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = "qwen3:14b", env = "DUKEMEMORY_LLM_MODEL")]
        model: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// List pending memory inbox items.
    InboxList {
        #[arg(long, default_value = "pending")]
        status: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Approve one inbox item and turn it into a memory card.
    InboxApprove {
        id: String,
        #[arg(long)]
        allow_sensitive: bool,
    },
    /// Reject one inbox item.
    InboxReject { id: String },
    /// Compact terminal review surface for maintenance.
    ReviewTui {
        #[arg(long, default_value_t = 30)]
        stale_days: i64,
    },
    /// Remember plain user text as a typed memory card.
    Remember {
        text: String,
        #[arg(long = "type", value_enum)]
        memory_type: Option<MemoryType>,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        allow_sensitive: bool,
    },
    /// Search memory in a user-friendly way.
    WhatDoWeKnow {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Print current next actions.
    WhatNext {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Mark matching memory as rejected.
    Forget {
        query: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Run health checks over memory, embeddings, links, CodeGraph, and secrets.
    Doctor {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        fix_redact: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        self_check: bool,
    },
    /// Print a compact project snapshot for agents.
    Snapshot {
        #[arg(long, default_value_t = 8000)]
        max_chars: usize,
        #[arg(long)]
        with_codegraph: bool,
        #[arg(long)]
        json: bool,
    },
    /// Compact task_state memories into a summary card.
    Compact {
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        dry_run: bool,
    },
    /// Compact old task/thread state with supersedes preservation.
    CompactV2 {
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        dry_run: bool,
    },
    /// Check a Rhai rules file.
    RhaiCheck { rules: PathBuf },
    /// Check extended Rhai policy hooks.
    PolicyCheck { rules: PathBuf },
    /// Apply extended Rhai policy hooks to stored memory.
    PolicyApply {
        rules: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Build/update local embedding vectors for memory cards.
    EmbedIndex {
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        force: bool,
    },
    /// Semantic search over embedded memory cards.
    EmbedSearch {
        query: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, default_value_t = VectorBackend::Json)]
        backend: VectorBackend,
    },
    /// List embedding models from a provider endpoint.
    ProviderList {
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long)]
        json: bool,
    },
    /// Benchmark local vector scoring over already indexed embeddings.
    VectorBench {
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
    },
    /// Show embedding freshness and indexed vector counts.
    EmbedStatus {
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Run one or more incremental embedding passes.
    EmbedWatch {
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long)]
        once: bool,
    },
    /// Print shell completions for bash, zsh, or fish.
    Completions {
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Print a small manpage-style reference.
    Man,
    /// Print audit events.
    Audit {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Report whether agents read memory and whether it appears useful.
    UsageReport {
        #[arg(long, default_value_t = 7)]
        since_days: i64,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Score memory usefulness and suggest safe cleanup actions.
    UsefulnessReport {
        #[arg(long, default_value_t = 30)]
        since_days: i64,
        #[arg(long, default_value_t = 30)]
        stale_days: i64,
        #[arg(long, default_value_t = 3)]
        hot_threshold: usize,
        #[arg(long)]
        json: bool,
    },
    /// Score every memory card for usefulness, token savings, risk, and evidence quality.
    QualityReport {
        #[arg(long, default_value_t = 30)]
        since_days: i64,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Record lightweight agent feedback for memory ids returned by a task.
    Feedback {
        #[arg(long = "id")]
        ids: Vec<String>,
        #[arg(long, value_enum)]
        rating: FeedbackRating,
        #[arg(long, default_value = "agent")]
        command: String,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        json: bool,
    },
    /// Choose the smallest useful memory budget for a task.
    BudgetPlan {
        task: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print a structured project memory profile.
    ProjectProfile {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return compressed recall optimized for small token budgets.
    Recall {
        query: String,
        #[arg(long, default_value_t = 1200)]
        max_chars: usize,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Initialize project memory, profile, embeddings, and optional autonomous daemon.
    Onboard {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        install_autonomous: bool,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Print health, quality, and autonomous status for discovered projects.
    Dashboard {
        #[arg(long)]
        json: bool,
    },
    /// Print one operational readiness report for UI, autonomy, embeddings, and sync.
    OpsStatus {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value_t = 7)]
        since_days: i64,
        #[arg(long)]
        json: bool,
    },
    /// Group and safely process inbox suggestions.
    InboxV2 {
        #[command(subcommand)]
        command: InboxV2Command,
    },
    /// Tune autonomous policy from feedback and rollback history.
    PolicyTune {
        #[arg(long, default_value = ".agent/autonomous-policy.json")]
        output: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Score whether project memory is useful, noisy, complete, and healthy.
    MemoryQa {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value_t = 7)]
        since_days: i64,
        #[arg(long)]
        json: bool,
    },
    /// Render or write a compact project memory contract.
    MemoryContract {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        write: bool,
        #[arg(long)]
        json: bool,
    },
    /// Update project memory wiring, skill, rules, contract, and health checks.
    UpgradeProject {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long, default_value = "~/.local/bin/dukememory")]
        to: String,
        #[arg(long, default_value = ".agent/install-backups")]
        backup_dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Diagnose Codex MCP configuration for dukememory.
    CodexDoctor {
        #[arg(long, default_value = "~/.codex/config.toml")]
        config: String,
        #[arg(long)]
        json: bool,
    },
    /// Initialize workspace helper files for dukememory.
    WorkspaceInit {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Write a portable support bundle with memory export and diagnostics.
    Bundle {
        output: PathBuf,
        #[arg(long)]
        redact: bool,
    },
    /// Run a local maintenance daemon loop.
    Daemon {
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long)]
        once: bool,
        #[arg(long)]
        auto_ingest: bool,
        #[arg(long)]
        no_autopilot: bool,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value_t = 10)]
        backup_keep: usize,
        #[arg(long, default_value_t = 86_400)]
        backup_every_secs: u64,
        #[arg(long, default_value_t = 5000)]
        cleanup_audit_keep: usize,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
    },
    /// Manage fully automatic local memory autopilot.
    Autopilot {
        #[command(subcommand)]
        command: AutopilotCommand,
    },
    /// Run fully autonomous memory maintenance with rollback.
    Autonomous {
        #[command(subcommand)]
        command: AutonomousCommand,
    },
    /// Serve a small local HTTP API.
    ServeHttp {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        #[arg(long)]
        once: bool,
    },
    /// Mark vector backend preference/migration status.
    VecMigrate {
        #[arg(long, value_enum, default_value_t = VectorBackend::Json)]
        backend: VectorBackend,
    },
    /// List likely merge candidates.
    MergeCandidates {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Apply a merge by superseding duplicate memory cards into one summary.
    MergeApply {
        primary_id: String,
        duplicate_id: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Resolve likely contradictions by superseding older matching decisions.
    ResolveContradictions {
        #[arg(long)]
        dry_run: bool,
    },
    /// Print active decision doctrine, supersession chains, and likely conflicts.
    Doctrine {
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show provenance and supporting metadata for one memory card.
    Evidence {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Detect cheap local memory drift before coding.
    Drift {
        #[arg(long)]
        changed_only: bool,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Manage project profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Terminal review UI alias for v8 maintenance.
    ReviewUi {
        #[arg(long, default_value_t = 30)]
        stale_days: i64,
    },
    /// LLM-assisted local maintenance suggestions.
    Maintain {
        #[arg(long)]
        llm: bool,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_LLM_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = "qwen3:14b", env = "DUKEMEMORY_LLM_MODEL")]
        model: String,
    },
    /// Export/import portable sync bundles.
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// Schema migration/status commands.
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    /// Local lock commands for daemon/HTTP/MCP safety.
    Lock {
        #[command(subcommand)]
        command: LockCommand,
    },
    /// Retrieve memory with FTS or hybrid ranking.
    Retrieve {
        query: String,
        #[arg(long, value_enum, default_value_t = RetrievalStrategy::Hybrid)]
        strategy: RetrievalStrategy,
        #[arg(long, value_enum, default_value_t = OutputFormat::Plain)]
        format: OutputFormat,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long)]
        budget: Option<usize>,
        #[arg(long, value_enum)]
        budget_profile: Option<BudgetProfile>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
    },
    /// Local retrieval evaluation harness.
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
    /// Print build and runtime information.
    BuildInfo,
    /// Build a release-ready local distribution directory.
    ReleaseBundle {
        #[arg(default_value = "dist/dukememory")]
        output: PathBuf,
    },
    /// Benchmark local memory operations without requiring a model server.
    Bench {
        #[arg(long)]
        json: bool,
    },
    /// Store durable self-knowledge about this memory system.
    SelfHost {
        #[arg(long)]
        force: bool,
    },
    /// Check permanent-use readiness across DB, schema, backups, and model endpoint.
    Health {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_HEALTH_ENDPOINT")]
        endpoint: String,
        #[arg(long)]
        json: bool,
    },
    /// Create a timestamped DB backup and prune old backups.
    BackupPolicy {
        #[arg(long, default_value = ".agent/backups")]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        keep: usize,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Verify a backup before restore.
    BackupVerify {
        input: PathBuf,
        #[arg(long)]
        strict: bool,
        #[arg(long)]
        json: bool,
    },
    /// Clean operational tables using retention limits.
    Cleanup {
        #[arg(long, default_value_t = 5000)]
        audit_keep: usize,
        #[arg(long, default_value_t = 30)]
        rejected_inbox_days: i64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Write a macOS launchd plist for an always-on local daemon.
    DaemonInstall {
        #[arg(
            long,
            default_value = "~/Library/LaunchAgents/com.dukememory.daemon.plist"
        )]
        output: PathBuf,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Run SQLite integrity checks.
    Integrity {
        #[arg(long)]
        json: bool,
    },
    /// Optimize SQLite indexes/FTS and optionally vacuum the database.
    Optimize {
        #[arg(long)]
        vacuum: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProfileCommand {
    List {
        #[arg(long, default_value = ".agent/profiles")]
        dir: PathBuf,
    },
    Use {
        name: String,
        #[arg(long, default_value = ".agent/profiles")]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
pub(crate) enum SyncCommand {
    Export {
        output: PathBuf,
        #[arg(long)]
        redact: bool,
    },
    Import {
        input: PathBuf,
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum ContextMode {
    Fast,
    Agent,
    Deep,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum VectorBackend {
    Json,
    SqliteVec,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum OutputFormat {
    Plain,
    Json,
    Markdown,
    Agent,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum RetrievalStrategy {
    Fts,
    Hybrid,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum BudgetProfile {
    Tiny,
    Normal,
    Deep,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum FeedbackRating {
    Useful,
    Useless,
    Missing,
}

#[derive(Subcommand)]
pub(crate) enum SchemaCommand {
    Status,
    Verify,
    Upgrade,
}

#[derive(Subcommand)]
pub(crate) enum LockCommand {
    Status,
    Clear {
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum AutopilotCommand {
    /// Show the latest daemon autopilot status JSON.
    Status {
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Check whether autopilot is ready for unattended use.
    Doctor {
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 180)]
        max_status_age_secs: u64,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long)]
        repair: bool,
        #[arg(long)]
        json: bool,
    },
    /// Safely repair autopilot prerequisites, then print before/after status.
    Repair {
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        backup_keep: usize,
        #[arg(long, default_value_t = 5000)]
        cleanup_audit_keep: usize,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Show recent structured autopilot daemon events.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Print an autopilot observability summary.
    Report {
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        history_limit: usize,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Export current autopilot status, doctor, and history to a JSON file.
    ExportStatus {
        output: PathBuf,
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        history_limit: usize,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
    },
    /// Evaluate autopilot health against monitoring thresholds.
    Alert {
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        history_limit: usize,
        #[arg(long, default_value_t = 100)]
        max_pending: usize,
        #[arg(long, default_value_t = 0)]
        max_failed_ticks: usize,
        #[arg(long, default_value_t = 180)]
        max_status_age_secs: u64,
        #[arg(long, default_value_t = 0)]
        max_embedding_stale: usize,
        #[arg(long)]
        require_backup: bool,
        #[arg(long)]
        require_endpoint: bool,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        write_alert: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Run one autopilot tick, then print status.
    RunOnce {
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value_t = 10)]
        backup_keep: usize,
        #[arg(long, default_value_t = 0)]
        backup_every_secs: u64,
        #[arg(long, default_value_t = 5000)]
        cleanup_audit_keep: usize,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Install a macOS launchd plist for autopilot daemon mode.
    Install {
        #[arg(
            long,
            default_value = "~/Library/LaunchAgents/com.dukememory.daemon.plist"
        )]
        output: PathBuf,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        #[arg(long, default_value = ".agent/sessions")]
        session_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value = ".agent/daemon-status.json")]
        status_file: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum AutonomousCommand {
    /// Print the last autonomous status report.
    Status {
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Run one autonomous maintenance cycle.
    RunOnce {
        #[arg(long, value_enum, default_value_t = AutonomousLevel::Normal)]
        level: AutonomousLevel,
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/autonomous-rollbacks")]
        rollback_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        backup_keep: usize,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Run autonomous maintenance forever.
    Daemon {
        #[arg(long, value_enum, default_value_t = AutonomousLevel::Normal)]
        level: AutonomousLevel,
        #[arg(long, default_value_t = 300)]
        interval_secs: u64,
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/autonomous-rollbacks")]
        rollback_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        backup_keep: usize,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
    },
    /// Logically roll back the last autonomous maintenance cycle.
    Rollback {
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Explain the latest autonomous status as a compact change diff.
    Explain {
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Install a macOS launchd plist for autonomous daemon mode.
    Install {
        #[arg(
            long,
            default_value = "~/Library/LaunchAgents/com.dukememory.autonomous.plist"
        )]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = AutonomousLevel::Normal)]
        level: AutonomousLevel,
        #[arg(long, default_value_t = 300)]
        interval_secs: u64,
        #[arg(long, default_value = ".agent/autonomous-status.json")]
        status_file: PathBuf,
        #[arg(long, default_value = ".agent/autonomous-rollbacks")]
        rollback_dir: PathBuf,
        #[arg(long, default_value = ".agent/backups")]
        backup_dir: PathBuf,
        #[arg(long, default_value = DEFAULT_EMBED_PROVIDER, env = "DUKEMEMORY_EMBED_PROVIDER")]
        provider: String,
        #[arg(long, default_value = DEFAULT_EMBED_ENDPOINT, env = "DUKEMEMORY_EMBED_ENDPOINT")]
        endpoint: String,
        #[arg(long, default_value = DEFAULT_EMBED_MODEL, env = "DUKEMEMORY_EMBED_MODEL")]
        model: String,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum AutonomousLevel {
    Conservative,
    Normal,
    Aggressive,
}

impl std::fmt::Display for AutonomousLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conservative => write!(f, "conservative"),
            Self::Normal => write!(f, "normal"),
            Self::Aggressive => write!(f, "aggressive"),
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum EvalCommand {
    AddCase {
        name: String,
        query: String,
        expected: String,
        #[arg(long, default_value_t = 4000)]
        budget: usize,
    },
    Run {
        #[arg(long)]
        json: bool,
    },
    Live {
        #[arg(long, default_value_t = 7)]
        since_days: i64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum InboxV2Command {
    Report {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    AutoApply {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum MemoryType {
    ProductGoal,
    UserPreference,
    Decision,
    DesignNote,
    KnownIssue,
    Command,
    TaskState,
    DomainFact,
    Constraint,
    Note,
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ProductGoal => "product_goal",
            Self::UserPreference => "user_preference",
            Self::Decision => "decision",
            Self::DesignNote => "design_note",
            Self::KnownIssue => "known_issue",
            Self::Command => "command",
            Self::TaskState => "task_state",
            Self::DomainFact => "domain_fact",
            Self::Constraint => "constraint",
            Self::Note => "note",
        };
        f.write_str(value)
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(crate) enum MemoryStatus {
    Active,
    Superseded,
    Rejected,
    Uncertain,
}

impl fmt::Display for MemoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Rejected => "rejected",
            Self::Uncertain => "uncertain",
        };
        f.write_str(value)
    }
}
