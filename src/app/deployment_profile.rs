use super::*;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DeploymentMode {
    Local,
    ReverseProxy,
}

impl DeploymentMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::ReverseProxy => "reverse_proxy",
        }
    }

    pub(crate) fn parse(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("local") {
            "local" => Ok(Self::Local),
            "reverse-proxy" | "reverse_proxy" => Ok(Self::ReverseProxy),
            other => {
                bail!("unsupported deployment mode `{other}`; expected local or reverse-proxy")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentProfileReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) mode: String,
    pub(crate) http: DeploymentHttpProfile,
    pub(crate) observability: DeploymentObservabilityProfile,
    pub(crate) encryption: DeploymentEncryptionProfile,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentHttpProfile {
    pub(crate) host: String,
    pub(crate) loopback_bind: bool,
    pub(crate) bearer_token_configured: bool,
    pub(crate) allowed_origins_configured: bool,
    pub(crate) public_origin: Option<String>,
    pub(crate) public_origin_https: bool,
    pub(crate) tls_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentObservabilityProfile {
    pub(crate) json_access_logs: bool,
    pub(crate) request_ids: bool,
    pub(crate) metrics_endpoint: String,
    pub(crate) otlp_exporter: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentEncryptionProfile {
    pub(crate) database_at_rest: String,
    pub(crate) database_file_permissions: String,
    pub(crate) sync_target: Option<String>,
    pub(crate) sync_bundle_encryption: String,
    pub(crate) sync_passphrase_ready: bool,
}

pub(crate) struct DeploymentProfileRequest<'a> {
    pub(crate) root: &'a Path,
    pub(crate) mode: DeploymentMode,
    pub(crate) host: &'a str,
    pub(crate) token_file: Option<&'a Path>,
    pub(crate) public_origin: Option<&'a str>,
    pub(crate) sync_target: Option<&'a Path>,
}

pub(crate) fn print_deployment_profile(
    request: DeploymentProfileRequest<'_>,
    json_out: bool,
) -> Result<()> {
    let report = deployment_profile_report(request);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Deployment Profile");
        println!("status: {}", report.status);
        println!("mode: {}", report.mode);
        for blocker in &report.blockers {
            println!("blocker: {blocker}");
        }
    }
    Ok(())
}

pub(crate) fn deployment_profile_report(
    request: DeploymentProfileRequest<'_>,
) -> DeploymentProfileReport {
    let root = request
        .root
        .canonicalize()
        .unwrap_or_else(|_| request.root.to_path_buf());
    let loopback_bind = http_server::is_loopback_host(request.host);
    let inline_token = std::env::var("DUKEMEMORY_HTTP_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let token_file_status = request.token_file.map(deployment_secret_file_ready);
    let bearer_token_configured = inline_token.is_some() || token_file_status == Some(true);
    let allowed_origins = std::env::var("DUKEMEMORY_HTTP_ALLOWED_ORIGINS").unwrap_or_default();
    let public_origin = request
        .public_origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let public_origin_https = public_origin
        .as_deref()
        .is_some_and(|origin| origin.starts_with("https://"));
    let allowed_origins_configured = public_origin.as_deref().is_some_and(|origin| {
        allowed_origins
            .split(',')
            .map(str::trim)
            .map(|value| value.trim_end_matches('/'))
            .any(|value| value == origin.trim_end_matches('/'))
    });
    let sync_passphrase_ready = if request.sync_target.is_some() {
        sync_passphrase_is_configured() && read_sync_passphrase().is_ok()
    } else {
        sync_passphrase_is_configured()
    };
    let mut blockers = Vec::new();
    match request.mode {
        DeploymentMode::Local => {
            if !loopback_bind {
                blockers.push("local mode must bind to a loopback address".to_string());
            }
        }
        DeploymentMode::ReverseProxy => {
            if !loopback_bind {
                blockers.push(
                    "reverse-proxy mode keeps DukeMemory on loopback and exposes only the TLS proxy"
                        .to_string(),
                );
            }
            if !bearer_token_configured {
                blockers.push("reverse-proxy mode requires a bearer token".to_string());
            }
            if !public_origin_https {
                blockers.push("reverse-proxy mode requires an https public origin".to_string());
            }
            if !allowed_origins_configured {
                blockers.push(
                    "public origin must be listed in DUKEMEMORY_HTTP_ALLOWED_ORIGINS".to_string(),
                );
            }
        }
    }
    if token_file_status == Some(false) {
        blockers.push("HTTP token file is missing, empty, or has unsafe permissions".to_string());
    }
    if request.sync_target.is_some() && !sync_passphrase_ready {
        blockers.push(
            "remote sync target requires a valid passphrase or mode-600 passphrase file"
                .to_string(),
        );
    }
    blockers.sort();
    blockers.dedup();
    let mut recommendations = vec![
        "keep the built-in listener on loopback and terminate TLS at Caddy or nginx".to_string(),
        "use encrypted host storage for the SQLite database; application-level database encryption is not implemented"
            .to_string(),
    ];
    if std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some() {
        recommendations.push(
            "OTEL_EXPORTER_OTLP_ENDPOINT is set, but DukeMemory currently emits local JSON logs and metrics only; configure journal/collector ingestion explicitly"
                .to_string(),
        );
    }
    let ok = blockers.is_empty();
    DeploymentProfileReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        mode: request.mode.as_str().to_string(),
        http: DeploymentHttpProfile {
            host: request.host.to_string(),
            loopback_bind,
            bearer_token_configured,
            allowed_origins_configured,
            public_origin,
            public_origin_https,
            tls_mode: "reverse_proxy_required_for_public_access".to_string(),
        },
        observability: DeploymentObservabilityProfile {
            json_access_logs: true,
            request_ids: true,
            metrics_endpoint: "/metrics".to_string(),
            otlp_exporter: "not_implemented".to_string(),
        },
        encryption: DeploymentEncryptionProfile {
            database_at_rest: "host_managed".to_string(),
            database_file_permissions: "0600_on_unix".to_string(),
            sync_target: request.sync_target.map(|path| path.display().to_string()),
            sync_bundle_encryption: SYNC_ENCRYPTION_MODE.to_string(),
            sync_passphrase_ready,
        },
        blockers,
        recommendations,
    }
}

fn deployment_secret_file_ready(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn local_profile_is_ready_and_public_profile_fails_closed() {
        let root = tempdir().unwrap();
        let local = deployment_profile_report(DeploymentProfileRequest {
            root: root.path(),
            mode: DeploymentMode::Local,
            host: "127.0.0.1",
            token_file: None,
            public_origin: None,
            sync_target: None,
        });
        assert!(local.ok);
        assert_eq!(local.observability.otlp_exporter, "not_implemented");
        assert_eq!(local.encryption.database_at_rest, "host_managed");

        let public = deployment_profile_report(DeploymentProfileRequest {
            root: root.path(),
            mode: DeploymentMode::ReverseProxy,
            host: "0.0.0.0",
            token_file: None,
            public_origin: Some("http://memory.example"),
            sync_target: None,
        });
        assert!(!public.ok);
        assert!(
            public
                .blockers
                .iter()
                .any(|blocker| blocker.contains("loopback"))
        );
        assert!(
            public
                .blockers
                .iter()
                .any(|blocker| blocker.contains("https public origin"))
        );
    }
}
