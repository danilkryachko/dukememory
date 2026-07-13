use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

pub(super) fn resolve_auth_token(
    inline_token: Option<&str>,
    token_file: Option<&Path>,
) -> Result<Option<String>> {
    if inline_token.is_some() && token_file.is_some() {
        bail!("use either --auth-token or --auth-token-file, not both");
    }
    if let Some(path) = token_file {
        validate_token_file_permissions(path)?;
        let token = fs::read_to_string(path)
            .with_context(|| format!("failed to read HTTP token file {}", path.display()))?;
        let token = token.trim();
        if token.is_empty() {
            bail!("HTTP token file {} is empty", path.display());
        }
        return Ok(Some(token.to_string()));
    }
    Ok(inline_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned))
}

#[cfg(unix)]
fn validate_token_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .with_context(|| format!("failed to inspect HTTP token file {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        bail!(
            "HTTP token file {} must not be accessible by group or others (use chmod 600)",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_token_file_permissions(path: &Path) -> Result<()> {
    fs::metadata(path)
        .with_context(|| format!("failed to inspect HTTP token file {}", path.display()))?;
    Ok(())
}

pub(super) fn token_matches(expected: &str, provided: &str) -> bool {
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .iter()
        .zip(provided)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

pub(super) fn origin_allowed(origin: &str, host: Option<&str>) -> bool {
    let origin = origin.trim().trim_end_matches('/');
    if origin.is_empty() || origin.eq_ignore_ascii_case("null") {
        return false;
    }
    if std::env::var("DUKEMEMORY_HTTP_ALLOWED_ORIGINS")
        .ok()
        .is_some_and(|allowed| {
            allowed
                .split(',')
                .map(str::trim)
                .map(|item| item.trim_end_matches('/'))
                .any(|item| !item.is_empty() && item.eq_ignore_ascii_case(origin))
        })
    {
        return true;
    }
    let Some(host) = host.map(str::trim).filter(|host| !host.is_empty()) else {
        return false;
    };
    ["http://", "https://"].iter().any(|scheme| {
        origin.strip_prefix(scheme).is_some_and(|authority| {
            !authority.contains('/') && authority.eq_ignore_ascii_case(host)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_must_match_request_host() {
        assert!(origin_allowed(
            "http://127.0.0.1:8765",
            Some("127.0.0.1:8765")
        ));
        assert!(origin_allowed(
            "https://memory.example",
            Some("memory.example")
        ));
        assert!(!origin_allowed(
            "https://evil.example",
            Some("memory.example")
        ));
        assert!(!origin_allowed("null", Some("127.0.0.1:8765")));
    }
}
