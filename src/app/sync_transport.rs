use super::*;
use age::secrecy::SecretString;
use std::fs::OpenOptions;

const SYNC_LOCK_LEASE_MS: i64 = 120_000;

#[derive(Debug, Serialize, Deserialize)]
struct SyncLockMetadata {
    token: String,
    pid: u32,
    acquired_at: i64,
    expires_at: i64,
}

pub(crate) struct SyncTargetLock {
    path: PathBuf,
    token: String,
    pub(crate) wait_ms: u128,
    pub(crate) stale_recovered: bool,
}

impl Drop for SyncTargetLock {
    fn drop(&mut self) {
        let owns_lock = fs::read(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<SyncLockMetadata>(&raw).ok())
            .is_some_and(|metadata| metadata.token == self.token);
        if owns_lock {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(crate) const SYNC_PASSPHRASE_ENV: &str = "DUKEMEMORY_SYNC_PASSPHRASE";
pub(crate) const SYNC_PASSPHRASE_FILE_ENV: &str = "DUKEMEMORY_SYNC_PASSPHRASE_FILE";
pub(crate) const SYNC_ENCRYPTION_MODE: &str = "age_scrypt";

pub(crate) fn sync_passphrase_is_configured() -> bool {
    std::env::var_os(SYNC_PASSPHRASE_ENV).is_some()
        || std::env::var_os(SYNC_PASSPHRASE_FILE_ENV).is_some()
}

pub(crate) fn read_sync_passphrase() -> Result<SecretString> {
    let inline = std::env::var(SYNC_PASSPHRASE_ENV).ok();
    let file = std::env::var_os(SYNC_PASSPHRASE_FILE_ENV).map(PathBuf::from);
    if inline.is_some() && file.is_some() {
        bail!("set only one of {SYNC_PASSPHRASE_ENV} or {SYNC_PASSPHRASE_FILE_ENV}");
    }
    let value = if let Some(path) = file {
        validate_passphrase_file_permissions(&path)?;
        fs::read_to_string(&path)
            .with_context(|| format!("failed to read sync passphrase file {}", path.display()))?
            .trim_end_matches(&['\r', '\n'][..])
            .to_string()
    } else {
        inline.ok_or_else(|| {
            anyhow::anyhow!(
                "encrypted sync requires {SYNC_PASSPHRASE_ENV} or {SYNC_PASSPHRASE_FILE_ENV}"
            )
        })?
    };
    if value.chars().count() < 12 {
        bail!("sync passphrase must contain at least 12 characters");
    }
    Ok(SecretString::from(value))
}

#[cfg(unix)]
fn validate_passphrase_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .with_context(|| format!("failed to inspect sync passphrase file {}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        bail!(
            "sync passphrase file {} must not be accessible by group or other users (use chmod 600)",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_passphrase_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

pub(crate) fn encrypt_sync_payload(plaintext: &[u8]) -> Result<Vec<u8>> {
    let passphrase = read_sync_passphrase()?;
    encrypt_sync_payload_with_passphrase(plaintext, passphrase)
}

fn encrypt_sync_payload_with_passphrase(
    plaintext: &[u8],
    passphrase: SecretString,
) -> Result<Vec<u8>> {
    let recipient = age::scrypt::Recipient::new(passphrase);
    age::encrypt(&recipient, plaintext).context("failed to encrypt sync bundle with age")
}

pub(crate) fn decrypt_sync_payload(ciphertext: &[u8]) -> Result<Vec<u8>> {
    let passphrase = read_sync_passphrase()?;
    decrypt_sync_payload_with_passphrase(ciphertext, passphrase)
}

fn decrypt_sync_payload_with_passphrase(
    ciphertext: &[u8],
    passphrase: SecretString,
) -> Result<Vec<u8>> {
    let identity = age::scrypt::Identity::new(passphrase);
    age::decrypt(&identity, ciphertext).context("failed to decrypt age sync bundle")
}

pub(crate) fn is_encrypted_sync_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"age-encryption.org/v1\n")
}

pub(crate) fn sync_target_lock_path(target: &Path) -> PathBuf {
    if target.extension().is_some() {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let name = target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("dukememory-sync");
        parent.join(format!(".{name}.lock"))
    } else {
        target.join(".dukememory-sync.lock")
    }
}

pub(crate) fn sync_target_lock_status(target: &Path) -> (bool, Option<i64>, bool) {
    let path = sync_target_lock_path(target);
    let Ok(metadata) = fs::metadata(path) else {
        return (false, None, false);
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64);
    let age_ms = modified.map(|modified| now_ms().saturating_sub(modified).max(0));
    let lock_metadata = fs::read(sync_target_lock_path(target))
        .ok()
        .and_then(|raw| serde_json::from_slice::<SyncLockMetadata>(&raw).ok());
    let stale = lock_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.expires_at <= now_ms())
        || lock_metadata.is_none() && age_ms.is_some_and(|age| age >= SYNC_LOCK_LEASE_MS);
    (true, age_ms, stale)
}

pub(crate) fn acquire_sync_target_lock(target: &Path) -> Result<SyncTargetLock> {
    let started = Instant::now();
    let path = sync_target_lock_path(target);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create sync target {}", parent.display()))?;
    }
    let token = Uuid::new_v4().to_string();
    let mut stale_recovered = false;
    for _ in 0..2 {
        let acquired_at = now_ms();
        let metadata = SyncLockMetadata {
            token: token.clone(),
            pid: std::process::id(),
            acquired_at,
            expires_at: acquired_at.saturating_add(SYNC_LOCK_LEASE_MS),
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(&serde_json::to_vec_pretty(&metadata)?)?;
                file.sync_all()?;
                return Ok(SyncTargetLock {
                    path,
                    token,
                    wait_ms: started.elapsed().as_millis(),
                    stale_recovered,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let existing = fs::read(&path)
                    .ok()
                    .and_then(|raw| serde_json::from_slice::<SyncLockMetadata>(&raw).ok());
                let stale = existing
                    .as_ref()
                    .is_some_and(|metadata| metadata.expires_at <= now_ms())
                    || existing.is_none()
                        && fs::metadata(&path)
                            .and_then(|metadata| metadata.modified())
                            .ok()
                            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                            .is_some_and(|modified| {
                                now_ms().saturating_sub(modified.as_millis() as i64)
                                    >= SYNC_LOCK_LEASE_MS
                            });
                if !stale {
                    let owner = existing
                        .map(|metadata| {
                            format!("pid {} until {}", metadata.pid, metadata.expires_at)
                        })
                        .unwrap_or_else(|| "unknown owner".to_string());
                    bail!("sync target is locked by {owner}: {}", path.display());
                }
                fs::remove_file(&path).with_context(|| {
                    format!("failed to remove stale sync lock {}", path.display())
                })?;
                stale_recovered = true;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to acquire sync lock {}", path.display()));
            }
        }
    }
    bail!("failed to acquire sync lock {}", path.display())
}

pub(crate) fn write_private_atomic(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("dukememory-sync");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.write_all(content)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                temporary.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_payload_round_trip_and_tamper_detection() {
        let passphrase = SecretString::from("correct horse battery staple".to_string());
        let encrypted = encrypt_sync_payload_with_passphrase(
            b"private dukememory sync bundle",
            passphrase.clone(),
        )
        .unwrap();
        assert!(is_encrypted_sync_payload(&encrypted));
        assert!(!String::from_utf8_lossy(&encrypted).contains("private dukememory"));
        assert_eq!(
            decrypt_sync_payload_with_passphrase(&encrypted, passphrase.clone()).unwrap(),
            b"private dukememory sync bundle"
        );

        let mut tampered = encrypted;
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(decrypt_sync_payload_with_passphrase(&tampered, passphrase).is_err());
    }
}
