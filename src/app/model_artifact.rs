use anyhow::{Context, Result, bail};
use hf_hub::api::sync::Api;
use hf_hub::{Repo, RepoType};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

static VERIFIED_ARTIFACTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn download_hf_model(
    api: &Api,
    repo_id: &str,
    revision: &str,
    file_name: &str,
    expected_sha256: Option<&str>,
) -> Result<PathBuf> {
    let repo = api.repo(Repo::with_revision(
        repo_id.to_string(),
        RepoType::Model,
        revision.to_string(),
    ));
    let path = repo
        .get(file_name)
        .with_context(|| format!("failed to download {repo_id}@{revision}/{file_name}"))?;
    if let Some(expected_sha256) = expected_sha256 {
        verify_sha256_once(&path, expected_sha256)?;
    }
    Ok(path)
}

fn verify_sha256_once(path: &Path, expected_sha256: &str) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect model artifact {}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let cache_key = format!(
        "{}:{}:{}:{}",
        path.display(),
        metadata.len(),
        modified,
        expected_sha256
    );
    let verified = VERIFIED_ARTIFACTS.get_or_init(|| Mutex::new(HashSet::new()));
    if verified
        .lock()
        .map_err(|_| anyhow::anyhow!("model artifact verification cache lock poisoned"))?
        .contains(&cache_key)
    {
        return Ok(());
    }

    verify_sha256(path, expected_sha256)?;
    verified
        .lock()
        .map_err(|_| anyhow::anyhow!("model artifact verification cache lock poisoned"))?
        .insert(cache_key);
    Ok(())
}

fn verify_sha256(path: &Path, expected_sha256: &str) -> Result<()> {
    let expected_sha256 = expected_sha256.trim().to_ascii_lowercase();
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("invalid expected SHA-256 for {}", path.display());
    }
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open model artifact {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to hash model artifact {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256 {
        bail!(
            "model artifact checksum mismatch for {}: expected {}, got {}",
            path.display(),
            expected_sha256,
            actual
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_verification_accepts_expected_content_and_rejects_tampering() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("artifact.bin");
        fs::write(&path, b"dukememory-model")?;
        verify_sha256(
            &path,
            "4b4e960bd35341f1d05e3054028db70952bb97a4a71825c3df250bd4a63684f2",
        )?;
        fs::write(&path, b"tampered")?;
        assert!(
            verify_sha256(
                &path,
                "4b4e960bd35341f1d05e3054028db70952bb97a4a71825c3df250bd4a63684f2",
            )
            .is_err()
        );
        Ok(())
    }
}
