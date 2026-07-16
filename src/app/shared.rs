use super::*;

pub(crate) fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn transactional<T>(
    conn: &Connection,
    label: &str,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let nested = !conn.is_autocommit();
    let savepoint = format!("dukememory_{}", Uuid::new_v4().simple());
    if nested {
        conn.execute_batch(&format!("SAVEPOINT {savepoint};"))?;
    } else {
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    }
    match f() {
        Ok(value) => {
            let finish = if nested {
                conn.execute_batch(&format!("RELEASE SAVEPOINT {savepoint};"))
            } else {
                conn.execute_batch("COMMIT;")
            };
            if let Err(err) = finish {
                let _ = if nested {
                    conn.execute_batch(&format!(
                        "ROLLBACK TO SAVEPOINT {savepoint}; RELEASE SAVEPOINT {savepoint};"
                    ))
                } else {
                    conn.execute_batch("ROLLBACK;")
                };
                return Err(err).with_context(|| format!("failed to commit transaction: {label}"));
            }
            Ok(value)
        }
        Err(err) => {
            let _ = if nested {
                conn.execute_batch(&format!(
                    "ROLLBACK TO SAVEPOINT {savepoint}; RELEASE SAVEPOINT {savepoint};"
                ))
            } else {
                conn.execute_batch("ROLLBACK;")
            };
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
    transactional(conn, "append_audit_event", || {
        let previous_event_hash = conn
            .query_row(
                "SELECT event_hash FROM memory_events ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .filter(|hash| !hash.is_empty());
        let previous_hash = if let Some(previous_hash) = previous_event_hash {
            previous_hash
        } else {
            conn.query_row(
                "SELECT anchor_hash FROM audit_checkpoints ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .filter(|hash| !hash.is_empty())
            .unwrap_or_else(|| "genesis".to_string())
        };
        let created_at = now_ms();
        conn.execute(
            "INSERT INTO memory_events (event_type, memory_id, detail, created_at, previous_hash, event_hash) VALUES (?1, ?2, ?3, ?4, ?5, '')",
            params![event_type, memory_id, detail, created_at, previous_hash],
        )?;
        let id = conn.last_insert_rowid();
        let event_hash = audit_event_hash(
            id,
            event_type,
            memory_id,
            detail,
            created_at,
            &previous_hash,
        );
        conn.execute(
            "UPDATE memory_events SET event_hash = ?1 WHERE id = ?2",
            params![event_hash, id],
        )?;
        Ok(())
    })
}

pub(crate) fn audit_event_hash(
    id: i64,
    event_type: &str,
    memory_id: Option<&str>,
    detail: &str,
    created_at: i64,
    previous_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    audit_hash_field(&mut hasher, b"dukememory-audit-event-v1");
    audit_hash_field(&mut hasher, id.to_string().as_bytes());
    audit_hash_field(&mut hasher, event_type.as_bytes());
    audit_hash_field(&mut hasher, if memory_id.is_some() { b"1" } else { b"0" });
    audit_hash_field(&mut hasher, memory_id.unwrap_or_default().as_bytes());
    audit_hash_field(&mut hasher, detail.as_bytes());
    audit_hash_field(&mut hasher, created_at.to_string().as_bytes());
    audit_hash_field(&mut hasher, previous_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn audit_hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn audit_checkpoint_hash(
    id: i64,
    created_at: i64,
    deleted_through_id: i64,
    deleted_count: i64,
    first_retained_id: Option<i64>,
    anchor_hash: &str,
    previous_checkpoint_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    audit_hash_field(&mut hasher, b"dukememory-audit-checkpoint-v1");
    audit_hash_field(&mut hasher, id.to_string().as_bytes());
    audit_hash_field(&mut hasher, created_at.to_string().as_bytes());
    audit_hash_field(&mut hasher, deleted_through_id.to_string().as_bytes());
    audit_hash_field(&mut hasher, deleted_count.to_string().as_bytes());
    audit_hash_field(
        &mut hasher,
        first_retained_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
            .as_bytes(),
    );
    audit_hash_field(&mut hasher, anchor_hash.as_bytes());
    audit_hash_field(&mut hasher, previous_checkpoint_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn validate_scope(scope: &str) -> Result<()> {
    scope.parse::<MemoryScope>().map(|_| ()).map_err(Into::into)
}

pub(crate) fn tokenize(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(str::trim)
        .filter(|part| part.len() > 2)
        .map(|part| part.to_lowercase())
        .collect()
}

struct SecretPattern {
    name: &'static str,
    regex: Regex,
}

fn secret_patterns() -> &'static [SecretPattern] {
    static PATTERNS: std::sync::OnceLock<Vec<SecretPattern>> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            ("openai_key", r"\bsk-[A-Za-z0-9_-]{8,}\b"),
            ("github_token", r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b"),
            ("gitlab_token", r"\bglpat-[A-Za-z0-9_-]{20,}\b"),
            ("slack_token", r"\bxox[baprs]-[A-Za-z0-9-]{16,}\b"),
            ("aws_access_key", r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
            ("google_api_key", r"\bAIza[A-Za-z0-9_-]{35}\b"),
            ("jwt", r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b"),
            ("authorization_header", r"(?im)^\s*authorization\s*:\s*(?:bearer\s+[A-Za-z0-9._~+/-]{12,}|basic\s+[A-Za-z0-9+/]{12,}={0,2})\s*$"),
            ("credential_url", r"(?i)\b[A-Za-z][A-Za-z0-9+.-]*://[^/\s:@]{1,128}:[^@\s/]{3,256}@"),
            ("private_key", r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"),
        ]
        .into_iter()
        .map(|(name, pattern)| SecretPattern {
            name,
            regex: Regex::new(pattern).expect("built-in secret regex must compile"),
        })
        .collect()
    })
}

fn assignment_secret_regex() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?im)(^|[\s,{])((?:api[_-]?key|apikey|secret(?:[_-]?key)?|password|passwd|token|private[_-]?key|access[_-]?key|client[_-]?secret|aws[_-]?secret[_-]?access[_-]?key)\s*[:=]\s*["']?)([^\s"',;}{]{6,})"#,
        )
        .expect("built-in assignment secret regex must compile")
    })
}

fn placeholder_secret_value(value: &str) -> bool {
    let normalized = value
        .trim_matches(|character: char| matches!(character, '"' | '\'' | '<' | '>' | '[' | ']'))
        .to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "redacted"
                | "example"
                | "sample"
                | "placeholder"
                | "changeme"
                | "change_me"
                | "replace_me"
                | "not-a-secret"
                | "not_a_secret"
        )
        || normalized.starts_with("your_")
        || normalized.starts_with("your-")
        || normalized.starts_with("${")
        || normalized.chars().all(|character| character == 'x')
}

pub(crate) fn sensitive_text_patterns(text: &str) -> Vec<&'static str> {
    let mut names = BTreeSet::new();
    for pattern in secret_patterns() {
        if pattern.regex.is_match(text) {
            names.insert(pattern.name);
        }
    }
    if assignment_secret_regex()
        .captures_iter(text)
        .filter_map(|captures| captures.get(3))
        .any(|value| !placeholder_secret_value(value.as_str()))
    {
        names.insert("assignment_secret");
    }
    names.into_iter().collect()
}

pub(crate) fn redact_sensitive_patterns(text: &str) -> String {
    static PRIVATE_KEY_BLOCK: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let private_key_block = PRIVATE_KEY_BLOCK.get_or_init(|| {
        Regex::new(
            r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
        )
        .expect("built-in private key regex must compile")
    });
    let mut redacted = private_key_block
        .replace_all(text, "[REDACTED]")
        .into_owned();
    redacted = assignment_secret_regex()
        .replace_all(&redacted, |captures: &regex::Captures<'_>| {
            let value = captures.get(3).map_or("", |value| value.as_str());
            if placeholder_secret_value(value) {
                captures
                    .get(0)
                    .map_or("", |matched| matched.as_str())
                    .to_string()
            } else {
                format!(
                    "{}{}[REDACTED]",
                    captures.get(1).map_or("", |part| part.as_str()),
                    captures.get(2).map_or("", |part| part.as_str())
                )
            }
        })
        .into_owned();
    for pattern in secret_patterns() {
        redacted = pattern
            .regex
            .replace_all(&redacted, "[REDACTED]")
            .into_owned();
    }
    redacted
}

pub(crate) fn reject_sensitive(title: &str, body: &str, allow_sensitive: bool) -> Result<()> {
    if allow_sensitive {
        return Ok(());
    }
    let text = format!("{title}\n{body}");
    if !sensitive_text_patterns(&text).is_empty() {
        bail!(
            "memory looks like it may contain a secret; use --allow-sensitive to store it intentionally"
        );
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {} for hashing", path.display()))?;
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

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_millis() as i64
}

#[cfg(test)]
mod secret_tests {
    use super::*;

    #[test]
    fn secret_detection_requires_adjacent_assignment_and_ignores_placeholders() {
        assert!(sensitive_text_patterns("Document the secret format: use the vault").is_empty());
        assert!(sensitive_text_patterns("password: example").is_empty());
        assert!(sensitive_text_patterns("api_key=${API_KEY}").is_empty());
        assert_eq!(
            sensitive_text_patterns("api_key: real-value-12345"),
            vec!["assignment_secret"]
        );
    }

    #[test]
    fn provider_tokens_urls_and_headers_are_detected_and_redacted() {
        let github_token = format!("{}{}", "ghp_", "a".repeat(36));
        let text = format!(
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz\npostgres://duke:correct-horse@localhost/db\n{github_token}"
        );
        let patterns = sensitive_text_patterns(&text);
        assert!(patterns.contains(&"authorization_header"));
        assert!(patterns.contains(&"credential_url"));
        assert!(patterns.contains(&"github_token"));
        let redacted = redact_sensitive_patterns(&text);
        assert!(!redacted.contains("correct-horse"));
        assert!(!redacted.contains("ghp_"));
        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn shared_file_hash_is_binary_safe_and_deterministic() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"abc").unwrap();
        assert_eq!(
            sha256_file(file.path()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
