use super::*;

pub(crate) fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn transactional(
    conn: &Connection,
    label: &str,
    f: impl FnOnce() -> Result<()>,
) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    match f() {
        Ok(()) => {
            conn.execute_batch("COMMIT;")?;
            Ok(())
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(err).with_context(|| format!("transaction failed: {label}"))
        }
    }
}

pub(crate) fn log_event(
    conn: &Connection,
    event_type: &str,
    memory_id: Option<&str>,
    detail: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_events (event_type, memory_id, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![event_type, memory_id, detail, now_ms()],
    )?;
    Ok(())
}

pub(crate) fn validate_scope(scope: &str) -> Result<()> {
    if VALID_SCOPES.contains(&scope) {
        Ok(())
    } else {
        bail!(
            "invalid scope: {scope}. Expected one of: {}",
            VALID_SCOPES.join(", ")
        )
    }
}

pub(crate) fn tokenize(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(str::trim)
        .filter(|part| part.len() > 2)
        .map(|part| part.to_lowercase())
        .collect()
}

pub(crate) fn reject_sensitive(title: &str, body: &str, allow_sensitive: bool) -> Result<()> {
    if allow_sensitive {
        return Ok(());
    }
    let text = format!("{title}\n{body}").to_lowercase();
    let suspicious_keys = [
        "api_key",
        "apikey",
        "secret",
        "password",
        "passwd",
        "token",
        "private_key",
        "access_key",
    ];
    if suspicious_keys
        .iter()
        .any(|key| text.contains(key) && (text.contains('=') || text.contains(':')))
    {
        bail!(
            "memory looks like it may contain a secret; use --allow-sensitive to store it intentionally"
        );
    }
    if text.contains("sk-") || text.contains("-----begin private key-----") {
        bail!(
            "memory looks like it may contain a secret; use --allow-sensitive to store it intentionally"
        );
    }
    Ok(())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_millis() as i64
}
