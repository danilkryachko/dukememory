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
    pub(crate) mcp: DeploymentMcpProfile,
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
    pub(crate) read_only_bearer_token_configured: bool,
    pub(crate) trusted_proxy_auth: bool,
    pub(crate) trusted_proxy_cidrs_configured: bool,
    pub(crate) oauth_authorization_servers_configured: bool,
    pub(crate) rate_limit_per_minute: u32,
    pub(crate) rate_limit_max_clients: usize,
    pub(crate) max_concurrent_requests: usize,
    pub(crate) max_concurrent_per_client: usize,
    pub(crate) allowed_origins_configured: bool,
    pub(crate) public_origin: Option<String>,
    pub(crate) public_origin_https: bool,
    pub(crate) tls_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentMcpProfile {
    pub(crate) max_concurrent_tasks: usize,
    pub(crate) max_concurrent_tasks_per_owner: usize,
    pub(crate) authenticated_task_ownership: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeploymentObservabilityProfile {
    pub(crate) json_access_logs: bool,
    pub(crate) request_ids: bool,
    pub(crate) metrics_endpoint: String,
    pub(crate) otlp_exporter: String,
    pub(crate) client_identifier_protection: String,
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
    let read_token_file_status = std::env::var_os("DUKEMEMORY_HTTP_READ_TOKEN_FILE")
        .map(PathBuf::from)
        .map(|path| deployment_secret_file_ready(&path));
    let trusted_proxy_auth = deployment_env_flag("DUKEMEMORY_HTTP_TRUSTED_PROXY_AUTH");
    let trusted_proxy_cidrs_configured = std::env::var("DUKEMEMORY_HTTP_TRUSTED_PROXY_CIDRS")
        .ok()
        .is_some_and(|value| value.split(',').any(|entry| !entry.trim().is_empty()));
    let authorization_servers =
        std::env::var("DUKEMEMORY_OAUTH_AUTHORIZATION_SERVERS").unwrap_or_default();
    let oauth_authorization_servers_configured = {
        let servers = authorization_servers
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        !servers.is_empty() && servers.iter().all(|value| deployment_https_url(value))
    };
    let rate_limit = std::env::var("DUKEMEMORY_HTTP_RATE_LIMIT_PER_MINUTE")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose();
    let rate_limit_per_minute = rate_limit
        .as_ref()
        .ok()
        .and_then(|value| *value)
        .unwrap_or(600);
    let (rate_limit_max_clients, rate_limit_max_clients_valid) =
        deployment_positive_usize("DUKEMEMORY_HTTP_RATE_LIMIT_MAX_CLIENTS", 2_048);
    let (max_concurrent_requests, max_concurrent_requests_valid) =
        deployment_positive_usize("DUKEMEMORY_HTTP_MAX_CONCURRENT_REQUESTS", 4);
    let (max_concurrent_per_client, max_concurrent_per_client_valid) =
        deployment_positive_usize("DUKEMEMORY_HTTP_MAX_CONCURRENT_PER_CLIENT", 4);
    let (max_concurrent_tasks, max_concurrent_tasks_valid) =
        deployment_positive_usize("DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS", 32);
    let (max_concurrent_tasks_per_owner, max_concurrent_tasks_per_owner_valid) =
        deployment_positive_usize("DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS_PER_OWNER", 4);
    let allowed_origins = std::env::var("DUKEMEMORY_HTTP_ALLOWED_ORIGINS").unwrap_or_default();
    let public_origin = request
        .public_origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let public_origin_https = public_origin.as_deref().is_some_and(deployment_https_url);
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
    let telemetry_identifier_protection = std::env::var("DUKEMEMORY_TELEMETRY_IDENTIFIERS")
        .unwrap_or_else(|_| "plain".to_string())
        .trim()
        .to_string();
    let telemetry_hash_key_status = std::env::var_os("DUKEMEMORY_TELEMETRY_HASH_KEY_FILE")
        .map(PathBuf::from)
        .map(|path| {
            deployment_secret_file_ready(&path)
                && fs::read_to_string(path)
                    .ok()
                    .is_some_and(|key| key.trim().len() >= 16)
        });
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
            if !bearer_token_configured && !trusted_proxy_auth {
                blockers.push(
                    "reverse-proxy mode requires a bearer token or trusted proxy authentication"
                        .to_string(),
                );
            }
            if trusted_proxy_auth && !trusted_proxy_cidrs_configured {
                blockers.push(
                    "trusted proxy authentication requires DUKEMEMORY_HTTP_TRUSTED_PROXY_CIDRS"
                        .to_string(),
                );
            }
            if trusted_proxy_auth && !oauth_authorization_servers_configured {
                blockers.push(
                    "trusted proxy authentication requires valid HTTPS authorization servers"
                        .to_string(),
                );
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
    if !matches!(
        telemetry_identifier_protection.as_str(),
        "plain" | "hash" | "omit"
    ) {
        blockers.push("DUKEMEMORY_TELEMETRY_IDENTIFIERS must be plain, hash, or omit".to_string());
    }
    if telemetry_identifier_protection == "hash" && telemetry_hash_key_status != Some(true) {
        blockers.push(
            "DUKEMEMORY_TELEMETRY_IDENTIFIERS=hash requires a valid mode-600 DUKEMEMORY_TELEMETRY_HASH_KEY_FILE"
                .to_string(),
        );
    }
    if read_token_file_status == Some(false) {
        blockers.push(
            "read-only HTTP token file is missing, empty, or has unsafe permissions".to_string(),
        );
    }
    if rate_limit.is_err() || rate_limit_per_minute == 0 {
        blockers
            .push("DUKEMEMORY_HTTP_RATE_LIMIT_PER_MINUTE must be a positive integer".to_string());
    }
    for (valid, name) in [
        (
            rate_limit_max_clients_valid,
            "DUKEMEMORY_HTTP_RATE_LIMIT_MAX_CLIENTS",
        ),
        (
            max_concurrent_requests_valid,
            "DUKEMEMORY_HTTP_MAX_CONCURRENT_REQUESTS",
        ),
        (
            max_concurrent_per_client_valid,
            "DUKEMEMORY_HTTP_MAX_CONCURRENT_PER_CLIENT",
        ),
        (
            max_concurrent_tasks_valid,
            "DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS",
        ),
        (
            max_concurrent_tasks_per_owner_valid,
            "DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS_PER_OWNER",
        ),
    ] {
        if !valid {
            blockers.push(format!("{name} must be a positive integer"));
        }
    }
    if max_concurrent_per_client > max_concurrent_requests {
        blockers.push(
            "DUKEMEMORY_HTTP_MAX_CONCURRENT_PER_CLIENT must not exceed the global HTTP limit"
                .to_string(),
        );
    }
    if max_concurrent_tasks_per_owner > max_concurrent_tasks {
        blockers.push(
            "DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS_PER_OWNER must not exceed the global MCP task limit"
                .to_string(),
        );
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
    if otlp::environment_status() != "otlp_http_json_logs_traces_metrics" {
        recommendations.push(
            "set OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_EXPORTER_OTLP_PROTOCOL=http/json to export bounded logs, traces, and metrics"
                .to_string(),
        );
    }
    if matches!(request.mode, DeploymentMode::ReverseProxy)
        && telemetry_identifier_protection == "plain"
    {
        recommendations.push(
            "set DUKEMEMORY_TELEMETRY_IDENTIFIERS=hash or omit before exporting public client addresses"
                .to_string(),
        );
    }
    let ok = blockers.is_empty();
    DeploymentProfileReport {
        version: 2,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        mode: request.mode.as_str().to_string(),
        http: DeploymentHttpProfile {
            host: request.host.to_string(),
            loopback_bind,
            bearer_token_configured,
            read_only_bearer_token_configured: read_token_file_status == Some(true),
            trusted_proxy_auth,
            trusted_proxy_cidrs_configured,
            oauth_authorization_servers_configured,
            rate_limit_per_minute,
            rate_limit_max_clients,
            max_concurrent_requests,
            max_concurrent_per_client,
            allowed_origins_configured,
            public_origin,
            public_origin_https,
            tls_mode: "reverse_proxy_required_for_public_access".to_string(),
        },
        mcp: DeploymentMcpProfile {
            max_concurrent_tasks,
            max_concurrent_tasks_per_owner,
            authenticated_task_ownership: true,
        },
        observability: DeploymentObservabilityProfile {
            json_access_logs: true,
            request_ids: true,
            metrics_endpoint: "/metrics".to_string(),
            otlp_exporter: otlp::environment_status().to_string(),
            client_identifier_protection: telemetry_identifier_protection,
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

fn deployment_env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
}

fn deployment_positive_usize(name: &str, default: usize) -> (usize, bool) {
    match std::env::var(name) {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(value) if value > 0 => (value, true),
            _ => (default, false),
        },
        Err(_) => (default, true),
    }
}

fn deployment_https_url(value: &str) -> bool {
    reqwest::Url::parse(value.trim()).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
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
        assert_eq!(local.observability.otlp_exporter, "disabled");
        assert_eq!(local.observability.client_identifier_protection, "plain");
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
