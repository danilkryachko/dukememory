use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
struct HealthReport {
    ok: bool,
    version: String,
    schema: i64,
    db_exists: bool,
    db_bytes: u64,
    schema_ok: bool,
    backup_dir_exists: bool,
    sessions_dir_exists: bool,
    endpoint: String,
    endpoint_ok: bool,
    integrity_ok: bool,
    backup_count: usize,
    findings: Vec<HealthFinding>,
}

#[derive(Debug, Serialize)]
struct HealthFinding {
    status: String,
    component: String,
    detail: String,
}

pub(crate) fn print_health(
    conn: &Connection,
    db: &Path,
    root: &Path,
    endpoint: &str,
    json_out: bool,
) -> Result<()> {
    let mut findings = Vec::new();
    let schema_ok = verify_schema(conn).is_ok() && schema_version(conn)? == CURRENT_SCHEMA_VERSION;
    push_finding(
        &mut findings,
        schema_ok,
        "schema",
        if schema_ok {
            "schema is current"
        } else {
            "schema needs upgrade or verification failed"
        },
    );

    let db_exists = db.exists();
    let db_bytes = fs::metadata(db).map(|meta| meta.len()).unwrap_or(0);
    push_finding(
        &mut findings,
        db_exists && db_bytes > 0,
        "database",
        if db_exists {
            "database file is present"
        } else {
            "database file is missing"
        },
    );

    let backup_dir_exists = root.join(".agent/backups").exists();
    let backup_count = list_backups(&root.join(".agent/backups"))
        .map(|items| items.len())
        .unwrap_or(0);
    push_finding(
        &mut findings,
        backup_dir_exists && backup_count > 0,
        "backups",
        if backup_dir_exists && backup_count > 0 {
            "backup directory exists and contains backups"
        } else if backup_dir_exists {
            "backup directory exists but has no backups yet"
        } else {
            "backup directory is not initialized"
        },
    );

    let sessions_dir_exists = root.join(".agent/sessions").exists();
    push_finding(
        &mut findings,
        sessions_dir_exists,
        "sessions",
        if sessions_dir_exists {
            "session ingest directory exists"
        } else {
            "session ingest directory is not initialized"
        },
    );

    let endpoint_ok = model_endpoint_ok(endpoint);
    push_finding(
        &mut findings,
        endpoint_ok,
        "model_endpoint",
        if endpoint_ok {
            "embedding/model endpoint is reachable or intentionally skipped"
        } else {
            "embedding/model endpoint is not reachable"
        },
    );

    let integrity_ok = sqlite_integrity_ok(conn);
    push_finding(
        &mut findings,
        integrity_ok,
        "sqlite_integrity",
        if integrity_ok {
            "SQLite integrity_check passed"
        } else {
            "SQLite integrity_check failed"
        },
    );

    let report = HealthReport {
        ok: schema_ok && db_exists && db_bytes > 0 && integrity_ok,
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: schema_version(conn)?,
        db_exists,
        db_bytes,
        schema_ok,
        backup_dir_exists,
        sessions_dir_exists,
        endpoint: endpoint.to_string(),
        endpoint_ok,
        integrity_ok,
        backup_count,
        findings,
    };

    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("health: {}", if report.ok { "ok" } else { "warn" });
        println!("version: {}", report.version);
        println!("schema: {}", report.schema);
        println!("db_bytes: {}", report.db_bytes);
        for finding in report.findings {
            println!(
                "{}  {}  {}",
                finding.status, finding.component, finding.detail
            );
        }
    }
    Ok(())
}

pub(crate) fn sqlite_integrity_ok(conn: &Connection) -> bool {
    conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map(|value| value == "ok")
        .unwrap_or(false)
}

fn push_finding(findings: &mut Vec<HealthFinding>, ok: bool, component: &str, detail: &str) {
    findings.push(HealthFinding {
        status: if ok { "ok" } else { "warn" }.to_string(),
        component: component.to_string(),
        detail: detail.to_string(),
    });
}

fn model_endpoint_ok(endpoint: &str) -> bool {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint.eq_ignore_ascii_case("mock") || endpoint == "local" {
        return true;
    }
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .and_then(|client| client.get(url).send())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

#[derive(Debug, Serialize)]
struct BackupPolicyReport {
    created: Option<String>,
    checksum_file: Option<String>,
    manifest_file: Option<String>,
    backup_sha256: Option<String>,
    backup_integrity_ok: bool,
    source_memory_count: Option<i64>,
    backup_memory_count: Option<i64>,
    table_counts_match: bool,
    source_table_counts: Vec<TableCount>,
    backup_table_counts: Vec<TableCount>,
    verified: bool,
    pruned: Vec<String>,
    temp_pruned: Vec<String>,
    sidecar_pruned: Vec<String>,
    kept: Vec<String>,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TableCount {
    table: String,
    count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupManifest {
    format_version: u32,
    created_at: i64,
    dukememory_version: String,
    schema: i64,
    backup_file: String,
    backup_bytes: u64,
    backup_sha256: String,
    backup_integrity_ok: bool,
    source_table_counts: Vec<TableCount>,
    backup_table_counts: Vec<TableCount>,
}

struct BackupMetadata {
    checksum_file: PathBuf,
    manifest_file: PathBuf,
    backup_sha256: String,
    backup_integrity_ok: bool,
    verified: bool,
    source_table_counts: Vec<TableCount>,
    backup_table_counts: Vec<TableCount>,
}

pub(crate) fn write_backup_metadata(source: &Connection, backup_path: &Path) -> Result<()> {
    create_backup_metadata(source, backup_path).map(|_| ())
}

fn create_backup_metadata(source: &Connection, backup_path: &Path) -> Result<BackupMetadata> {
    let source_table_counts = table_counts(source)?;
    let backup = Connection::open(backup_path)
        .with_context(|| format!("failed to open backup {}", backup_path.display()))?;
    let backup_table_counts = table_counts(&backup)?;
    let backup_integrity_ok = sqlite_integrity_ok(&backup);
    let backup_sha256 = sha256_file(backup_path)?;
    let checksum_file = backup_path.with_extension("db.sha256");
    write_sidecar_file_atomically(
        &checksum_file,
        format!(
            "{}  {}\n",
            backup_sha256,
            backup_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("backup.db")
        )
        .as_bytes(),
    )?;
    let manifest = BackupManifest {
        format_version: 1,
        created_at: now_ms(),
        dukememory_version: env!("CARGO_PKG_VERSION").to_string(),
        schema: schema_version(&backup)?,
        backup_file: backup_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("backup.db")
            .to_string(),
        backup_bytes: fs::metadata(backup_path)?.len(),
        backup_sha256: backup_sha256.clone(),
        backup_integrity_ok,
        source_table_counts: source_table_counts.clone(),
        backup_table_counts: backup_table_counts.clone(),
    };
    let manifest_file = backup_manifest_path(backup_path);
    write_sidecar_file_atomically(
        &manifest_file,
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )?;
    let verified = verify_backup_file(backup_path, true)?.verified;
    if !verified {
        bail!(
            "backup metadata verification failed after write: {}",
            backup_path.display()
        );
    }
    Ok(BackupMetadata {
        checksum_file,
        manifest_file,
        backup_sha256,
        backup_integrity_ok,
        verified,
        source_table_counts,
        backup_table_counts,
    })
}

pub(crate) fn run_backup_policy(
    db: &Path,
    output_dir: &Path,
    keep: usize,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    run_backup_policy_impl(db, output_dir, keep, dry_run, json_out, false)
}

pub(crate) fn run_backup_policy_quiet(db: &Path, output_dir: &Path, keep: usize) -> Result<()> {
    run_backup_policy_impl(db, output_dir, keep, false, false, true)
}

fn run_backup_policy_impl(
    db: &Path,
    output_dir: &Path,
    keep: usize,
    dry_run: bool,
    json_out: bool,
    quiet: bool,
) -> Result<()> {
    if keep == 0 {
        bail!("--keep must be at least 1");
    }
    if !db.exists() {
        bail!("database does not exist: {}", db.display());
    }
    let backup_path = next_backup_path(output_dir);
    let mut backup_sha256 = None;
    let mut checksum_file = None;
    let mut manifest_file = None;
    let mut backup_integrity_ok = false;
    let mut source_memory_count = None;
    let mut backup_memory_count = None;
    let mut source_table_counts = Vec::new();
    let mut backup_table_counts = Vec::new();
    let mut metadata_verified = false;
    let mut temp_pruned = Vec::new();
    if !dry_run {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;
        temp_pruned = prune_backup_temp_files(output_dir)?;
        let source = Connection::open(db)
            .with_context(|| format!("failed to open source database {}", db.display()))?;
        source.busy_timeout(std::time::Duration::from_secs(15))?;
        source_memory_count = Some(memory_count(&source)?);
        sqlite_backup_to(&source, &backup_path)?;
        let metadata = create_backup_metadata(&source, &backup_path)?;
        backup_memory_count = memory_count_from_table_counts(&metadata.backup_table_counts);
        checksum_file = Some(metadata.checksum_file.display().to_string());
        manifest_file = Some(metadata.manifest_file.display().to_string());
        backup_sha256 = Some(metadata.backup_sha256);
        backup_integrity_ok = metadata.backup_integrity_ok;
        metadata_verified = metadata.verified;
        source_table_counts = metadata.source_table_counts;
        backup_table_counts = metadata.backup_table_counts;
    }

    let mut backups = list_backups(output_dir)?;
    if dry_run {
        backups.push(backup_path.clone());
    }
    backups.sort();
    backups.reverse();

    let kept = backups
        .iter()
        .take(keep)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let prune_paths = backups.into_iter().skip(keep).collect::<Vec<_>>();
    let mut pruned = Vec::new();
    for path in prune_paths {
        pruned.push(path.display().to_string());
        if !dry_run && path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
            let checksum = path.with_extension("db.sha256");
            if checksum.exists() {
                fs::remove_file(&checksum)
                    .with_context(|| format!("failed to remove {}", checksum.display()))?;
            }
            let manifest = backup_manifest_path(&path);
            if manifest.exists() {
                fs::remove_file(&manifest)
                    .with_context(|| format!("failed to remove {}", manifest.display()))?;
            }
        }
    }
    let sidecar_pruned = if dry_run {
        Vec::new()
    } else {
        prune_orphan_backup_sidecars(output_dir)?
    };

    let table_counts_match = source_table_counts == backup_table_counts;
    let report = BackupPolicyReport {
        created: Some(backup_path.display().to_string()),
        checksum_file,
        manifest_file,
        backup_sha256,
        backup_integrity_ok,
        source_memory_count,
        backup_memory_count,
        table_counts_match,
        source_table_counts,
        backup_table_counts,
        verified: !dry_run
            && metadata_verified
            && backup_integrity_ok
            && source_memory_count == backup_memory_count
            && table_counts_match,
        pruned,
        temp_pruned,
        sidecar_pruned,
        kept,
        dry_run,
    };
    if quiet {
        return Ok(());
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("backup: {}", backup_path.display());
        println!("verified: {}", report.verified);
        println!("kept: {}", report.kept.len());
        println!("pruned: {}", report.pruned.len());
        if dry_run {
            println!("dry_run: true");
        }
        if !report.temp_pruned.is_empty() {
            println!("temp_pruned: {}", report.temp_pruned.len());
        }
        if !report.sidecar_pruned.is_empty() {
            println!("sidecar_pruned: {}", report.sidecar_pruned.len());
        }
    }
    Ok(())
}

fn prune_backup_temp_files(output_dir: &Path) -> Result<Vec<String>> {
    if !output_dir.exists() {
        return Ok(Vec::new());
    }
    let mut pruned = Vec::new();
    for entry in fs::read_dir(output_dir)
        .with_context(|| format!("failed to read {}", output_dir.display()))?
    {
        let path = entry?.path();
        if !path.is_file() || !is_backup_temp_file(&path) {
            continue;
        }
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
        pruned.push(path.display().to_string());
    }
    pruned.sort();
    Ok(pruned)
}

fn is_backup_temp_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    (name.starts_with("dukememory-") && name.ends_with(".db.tmp"))
        || (name.ends_with(".db.sha256.tmp") || name.contains(".db.sha256.tmp-"))
        || (name.ends_with(".db.manifest.tmp") || name.contains(".db.manifest.tmp-"))
}

fn prune_orphan_backup_sidecars(output_dir: &Path) -> Result<Vec<String>> {
    if !output_dir.exists() {
        return Ok(Vec::new());
    }
    let mut pruned = Vec::new();
    for entry in fs::read_dir(output_dir)
        .with_context(|| format!("failed to read {}", output_dir.display()))?
    {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(backup_path) = backup_path_for_sidecar(&path) else {
            continue;
        };
        if backup_path.exists() {
            continue;
        }
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
        pruned.push(path.display().to_string());
    }
    pruned.sort();
    Ok(pruned)
}

fn backup_path_for_sidecar(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let backup_name = name
        .strip_suffix(".sha256")
        .or_else(|| name.strip_suffix(".manifest.json"))?;
    if !backup_name.starts_with("dukememory-") || !backup_name.ends_with(".db") {
        return None;
    }
    Some(
        path.parent()
            .unwrap_or_else(|| Path::new(""))
            .join(backup_name),
    )
}

fn list_backups(output_dir: &Path) -> Result<Vec<PathBuf>> {
    if !output_dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(output_dir)
        .with_context(|| format!("failed to read {}", output_dir.display()))?
    {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with("dukememory-") && name.ends_with(".db") {
            backups.push(path);
        }
    }
    Ok(backups)
}

fn next_backup_path(output_dir: &Path) -> PathBuf {
    let ts = now_ms();
    let first = output_dir.join(format!("dukememory-{ts}.db"));
    if !first.exists() {
        return first;
    }
    for suffix in 1..=999 {
        let path = output_dir.join(format!("dukememory-{ts}-{suffix}.db"));
        if !path.exists() {
            return path;
        }
    }
    output_dir.join(format!("dukememory-{}-overflow.db", now_ms()))
}

fn sqlite_backup_to(conn: &Connection, output: &Path) -> Result<()> {
    let tmp = output.with_extension("db.tmp");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("failed to remove {}", tmp.display()))?;
    }
    if output.exists() {
        fs::remove_file(output)
            .with_context(|| format!("failed to remove {}", output.display()))?;
    }
    let tmp_sql = tmp.display().to_string();
    conn.execute("VACUUM INTO ?1", params![tmp_sql])?;
    fs::rename(&tmp, output)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), output.display()))?;
    Ok(())
}

fn memory_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .map_err(Into::into)
}

fn memory_count_from_table_counts(counts: &[TableCount]) -> Option<i64> {
    counts
        .iter()
        .find(|item| item.table == "memories")
        .map(|item| item.count)
}

fn write_sidecar_file_atomically(path: &Path, content: &[u8]) -> Result<()> {
    let tmp = sidecar_tmp_path(path);
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("failed to remove {}", tmp.display()))?;
    }
    write_file(&tmp, content)?;
    fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

fn sidecar_tmp_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    path.with_extension(format!("{extension}.tmp-{}", now_ms()))
}

fn table_counts(conn: &Connection) -> Result<Vec<TableCount>> {
    let mut counts = Vec::new();
    for table in [
        "memories",
        "memory_links",
        "memory_embeddings",
        "memory_inbox",
        "memory_events",
        "memory_locks",
        "eval_cases",
        "memory_sources",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count = conn.query_row(&sql, [], |row| row.get(0))?;
        counts.push(TableCount {
            table: table.to_string(),
            count,
        });
    }
    Ok(counts)
}

#[derive(Debug, Serialize)]
struct BackupVerifyReport {
    input: String,
    strict: bool,
    checksum_file: Option<String>,
    checksum_present: bool,
    checksum_ok: Option<bool>,
    manifest_file: Option<String>,
    manifest_present: bool,
    manifest_ok: Option<bool>,
    strict_ok: bool,
    backup_sha256: String,
    integrity_ok: bool,
    schema: i64,
    table_counts: Vec<TableCount>,
    reasons: Vec<String>,
    verified: bool,
}

pub(crate) fn print_backup_verify(input: &Path, strict: bool, json_out: bool) -> Result<()> {
    let report = verify_backup_file(input, strict)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("backup: {}", report.input);
        println!("strict: {}", report.strict);
        println!("verified: {}", report.verified);
        println!("strict_ok: {}", report.strict_ok);
        println!("integrity_ok: {}", report.integrity_ok);
        println!("checksum_present: {}", report.checksum_present);
        if let Some(checksum_ok) = report.checksum_ok {
            println!("checksum_ok: {checksum_ok}");
        }
        println!("manifest_present: {}", report.manifest_present);
        if let Some(manifest_ok) = report.manifest_ok {
            println!("manifest_ok: {manifest_ok}");
        }
        println!("schema: {}", report.schema);
        println!("backup_sha256: {}", report.backup_sha256);
        for item in report.table_counts {
            println!("{}: {}", item.table, item.count);
        }
        for reason in report.reasons {
            println!("reason: {reason}");
        }
    }
    Ok(())
}

pub(crate) fn ensure_backup_verified(input: &Path, strict: bool) -> Result<()> {
    let report = verify_backup_file(input, strict)?;
    if !report.verified {
        bail!("backup verification failed: {}", input.display());
    }
    Ok(())
}

fn verify_backup_file(input: &Path, strict: bool) -> Result<BackupVerifyReport> {
    if !input.exists() {
        bail!("backup does not exist: {}", input.display());
    }
    let backup_sha256 = sha256_file(input)?;
    let checksum_file = input.with_extension("db.sha256");
    let checksum_present = checksum_file.exists();
    let checksum_ok = if checksum_present {
        let raw = fs::read_to_string(&checksum_file)
            .with_context(|| format!("failed to read {}", checksum_file.display()))?;
        let expected = raw
            .split_whitespace()
            .next()
            .with_context(|| format!("invalid checksum file {}", checksum_file.display()))?;
        Some(expected == backup_sha256)
    } else {
        None
    };
    let conn = Connection::open(input)
        .with_context(|| format!("failed to open backup {}", input.display()))?;
    let integrity_ok = sqlite_integrity_ok(&conn);
    let schema = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_versions",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let table_counts = table_counts(&conn)?;
    let manifest_file = backup_manifest_path(input);
    let manifest_present = manifest_file.exists();
    let mut reasons = Vec::new();
    if !integrity_ok {
        reasons.push("integrity_check_failed".to_string());
    }
    if checksum_present && checksum_ok == Some(false) {
        reasons.push("checksum_mismatch".to_string());
    }
    if strict && !checksum_present {
        reasons.push("checksum_missing".to_string());
    }
    let manifest_reasons = if manifest_present {
        verify_manifest_reasons(
            input,
            &manifest_file,
            &backup_sha256,
            integrity_ok,
            schema,
            &table_counts,
        )?
    } else {
        Vec::new()
    };
    let manifest_ok = if manifest_present {
        Some(manifest_reasons.is_empty())
    } else {
        None
    };
    reasons.extend(manifest_reasons);
    if strict && !manifest_present {
        reasons.push("manifest_missing".to_string());
    }
    let strict_ok = !strict
        || (checksum_present
            && checksum_ok == Some(true)
            && manifest_present
            && manifest_ok == Some(true));
    let verified =
        integrity_ok && checksum_ok.unwrap_or(true) && manifest_ok.unwrap_or(true) && strict_ok;
    Ok(BackupVerifyReport {
        input: input.display().to_string(),
        strict,
        checksum_file: checksum_present.then(|| checksum_file.display().to_string()),
        checksum_present,
        checksum_ok,
        manifest_file: manifest_present.then(|| manifest_file.display().to_string()),
        manifest_present,
        manifest_ok,
        strict_ok,
        backup_sha256,
        integrity_ok,
        schema,
        table_counts,
        reasons,
        verified,
    })
}

fn backup_manifest_path(backup: &Path) -> PathBuf {
    backup.with_extension("db.manifest.json")
}

fn verify_manifest_reasons(
    backup: &Path,
    manifest_path: &Path,
    backup_sha256: &str,
    integrity_ok: bool,
    schema: i64,
    table_counts: &[TableCount],
) -> Result<Vec<String>> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: BackupManifest = serde_json::from_str(&raw)
        .with_context(|| format!("invalid backup manifest {}", manifest_path.display()))?;
    let backup_file_ok = backup
        .file_name()
        .and_then(|value| value.to_str())
        .map(|name| name == manifest.backup_file)
        .unwrap_or(false);
    let backup_bytes = fs::metadata(backup)?.len();
    let mut reasons = Vec::new();
    if manifest.format_version != 1 {
        reasons.push("manifest_format_version_mismatch".to_string());
    }
    if !backup_file_ok {
        reasons.push("manifest_backup_file_mismatch".to_string());
    }
    if manifest.backup_bytes != backup_bytes {
        reasons.push("manifest_backup_bytes_mismatch".to_string());
    }
    if manifest.backup_sha256 != backup_sha256 {
        reasons.push("manifest_backup_sha256_mismatch".to_string());
    }
    if manifest.backup_integrity_ok != integrity_ok {
        reasons.push("manifest_integrity_status_mismatch".to_string());
    }
    if manifest.schema != schema {
        reasons.push("manifest_schema_mismatch".to_string());
    }
    if manifest.source_table_counts != table_counts {
        reasons.push("manifest_source_table_counts_mismatch".to_string());
    }
    if manifest.backup_table_counts != table_counts {
        reasons.push("manifest_backup_table_counts_mismatch".to_string());
    }
    Ok(reasons)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Serialize)]
struct CleanupReport {
    audit_deleted: usize,
    rejected_inbox_deleted: usize,
    dry_run: bool,
}

pub(crate) fn run_cleanup(
    conn: &Connection,
    audit_keep: usize,
    rejected_inbox_days: i64,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    run_cleanup_impl(
        conn,
        audit_keep,
        rejected_inbox_days,
        dry_run,
        json_out,
        false,
    )
}

pub(crate) fn run_cleanup_quiet(
    conn: &Connection,
    audit_keep: usize,
    rejected_inbox_days: i64,
) -> Result<()> {
    run_cleanup_impl(conn, audit_keep, rejected_inbox_days, false, false, true)
}

fn run_cleanup_impl(
    conn: &Connection,
    audit_keep: usize,
    rejected_inbox_days: i64,
    dry_run: bool,
    json_out: bool,
    quiet: bool,
) -> Result<()> {
    let audit_delete_count: usize = conn.query_row(
        "SELECT COUNT(*) FROM memory_events WHERE id NOT IN (SELECT id FROM memory_events ORDER BY created_at DESC, id DESC LIMIT ?1)",
        params![audit_keep.min(i64::MAX as usize) as i64],
        |row| row.get(0),
    )?;
    let cutoff = now_ms() - rejected_inbox_days.max(0) * 86_400_000;
    let rejected_count: usize = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'rejected' AND updated_at < ?1",
        params![cutoff],
        |row| row.get(0),
    )?;
    if !dry_run {
        conn.execute(
            "DELETE FROM memory_events WHERE id NOT IN (SELECT id FROM memory_events ORDER BY created_at DESC, id DESC LIMIT ?1)",
            params![audit_keep.min(i64::MAX as usize) as i64],
        )?;
        conn.execute(
            "DELETE FROM memory_inbox WHERE status = 'rejected' AND updated_at < ?1",
            params![cutoff],
        )?;
        log_event(
            conn,
            "cleanup",
            None,
            "applied operational retention cleanup",
        )?;
    }
    let report = CleanupReport {
        audit_deleted: audit_delete_count,
        rejected_inbox_deleted: rejected_count,
        dry_run,
    };
    if quiet {
        return Ok(());
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("audit_deleted: {}", report.audit_deleted);
        println!("rejected_inbox_deleted: {}", report.rejected_inbox_deleted);
        println!("dry_run: {}", report.dry_run);
    }
    Ok(())
}

pub(crate) fn write_launchd_plist(
    db: &Path,
    output: &Path,
    interval_secs: u64,
    session_dir: &Path,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let output = expand_path(output);
    if output.exists() && !force && !dry_run {
        bail!(
            "{} already exists (use --force to overwrite)",
            output.display()
        );
    }
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let plist = launchd_plist(&exe, db, interval_secs.max(1), session_dir);
    if dry_run {
        println!("{plist}");
        return Ok(());
    }
    write_file(&output, plist.as_bytes())?;
    println!("{}", output.display());
    Ok(())
}

pub(crate) struct AutopilotLaunchdRequest<'a> {
    pub(crate) db: &'a Path,
    pub(crate) output: &'a Path,
    pub(crate) interval_secs: u64,
    pub(crate) session_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) status_file: &'a Path,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
}

pub(crate) fn write_autopilot_launchd_plist(request: AutopilotLaunchdRequest<'_>) -> Result<()> {
    let output = expand_path(request.output);
    if output.exists() && !request.force && !request.dry_run {
        bail!(
            "{} already exists (use --force to overwrite)",
            output.display()
        );
    }
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let plist = autopilot_launchd_plist(
        &exe,
        request.db,
        request.interval_secs.max(1),
        request.session_dir,
        request.backup_dir,
        request.status_file,
    );
    if request.dry_run {
        println!("{plist}");
        return Ok(());
    }
    write_file(&output, plist.as_bytes())?;
    println!("{}", output.display());
    Ok(())
}

fn launchd_plist(exe: &Path, db: &Path, interval_secs: u64, session_dir: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.dukememory.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--db</string>
    <string>{}</string>
    <string>daemon</string>
    <string>--auto-ingest</string>
    <string>--session-dir</string>
    <string>{}</string>
    <string>--interval-secs</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml_escape(&exe.display().to_string()),
        xml_escape(&db.display().to_string()),
        xml_escape(&session_dir.display().to_string()),
        interval_secs,
        xml_escape(".agent/dukememory-daemon.out.log"),
        xml_escape(".agent/dukememory-daemon.err.log"),
    )
}

fn autopilot_launchd_plist(
    exe: &Path,
    db: &Path,
    interval_secs: u64,
    session_dir: &Path,
    backup_dir: &Path,
    status_file: &Path,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.dukememory.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--db</string>
    <string>{}</string>
    <string>daemon</string>
    <string>--session-dir</string>
    <string>{}</string>
    <string>--backup-dir</string>
    <string>{}</string>
    <string>--status-file</string>
    <string>{}</string>
    <string>--interval-secs</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml_escape(&exe.display().to_string()),
        xml_escape(&db.display().to_string()),
        xml_escape(&session_dir.display().to_string()),
        xml_escape(&backup_dir.display().to_string()),
        xml_escape(&status_file.display().to_string()),
        interval_secs,
        xml_escape(".agent/dukememory-daemon.out.log"),
        xml_escape(".agent/dukememory-daemon.err.log"),
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn expand_path(path: &Path) -> PathBuf {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_path_buf()
}
