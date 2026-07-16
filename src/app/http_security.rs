use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const READ_TOKEN_FILE_ENV: &str = "DUKEMEMORY_HTTP_READ_TOKEN_FILE";
const RATE_LIMIT_ENV: &str = "DUKEMEMORY_HTTP_RATE_LIMIT_PER_MINUTE";
const RATE_LIMIT_MAX_CLIENTS_ENV: &str = "DUKEMEMORY_HTTP_RATE_LIMIT_MAX_CLIENTS";
const TRUSTED_PROXY_CIDRS_ENV: &str = "DUKEMEMORY_HTTP_TRUSTED_PROXY_CIDRS";
const MAX_CONCURRENT_REQUESTS_ENV: &str = "DUKEMEMORY_HTTP_MAX_CONCURRENT_REQUESTS";
const MAX_CONCURRENT_PER_CLIENT_ENV: &str = "DUKEMEMORY_HTTP_MAX_CONCURRENT_PER_CLIENT";
const TRUSTED_PROXY_AUTH_ENV: &str = "DUKEMEMORY_HTTP_TRUSTED_PROXY_AUTH";
const OAUTH_AUTHORIZATION_SERVERS_ENV: &str = "DUKEMEMORY_OAUTH_AUTHORIZATION_SERVERS";
const DEFAULT_RATE_LIMIT_PER_MINUTE: u32 = 600;
const MAX_RATE_LIMIT_CLIENTS: usize = 2048;
const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 4;
const DEFAULT_MAX_CONCURRENT_PER_CLIENT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HttpAuthContext {
    pub(super) principal: String,
    scopes: HashSet<String>,
}

impl HttpAuthContext {
    pub(super) fn public() -> Self {
        Self {
            principal: "public".to_string(),
            scopes: HashSet::from(["memory:read".to_string()]),
        }
    }

    pub(super) fn allows(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }
}

fn full_scopes() -> HashSet<String> {
    [
        "memory:read",
        "memory:write",
        "memory:maintenance",
        "memory:filesystem",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[derive(Debug, Clone)]
pub(super) struct HttpAuthPolicy {
    full_token: Option<String>,
    read_token: Option<String>,
    trusted_proxy_auth: bool,
    protected_resource: Option<String>,
    authorization_servers: Vec<String>,
}

impl HttpAuthPolicy {
    pub(super) fn from_environment(full_token: Option<&str>) -> Result<Self> {
        let read_token = std::env::var_os(READ_TOKEN_FILE_ENV)
            .map(PathBuf::from)
            .map(|path| read_private_token_file(&path))
            .transpose()?;
        let mut policy = Self::new(full_token.map(ToOwned::to_owned), read_token)?;
        policy.trusted_proxy_auth = env_flag(TRUSTED_PROXY_AUTH_ENV)?;
        let trusted_proxy_cidrs_configured = std::env::var(TRUSTED_PROXY_CIDRS_ENV)
            .ok()
            .is_some_and(|values| values.split(',').any(|value| !value.trim().is_empty()));
        policy.protected_resource = std::env::var("DUKEMEMORY_PUBLIC_ORIGIN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| validate_oauth_uri(&value, "DUKEMEMORY_PUBLIC_ORIGIN"))
            .transpose()?;
        policy.authorization_servers = std::env::var(OAUTH_AUTHORIZATION_SERVERS_ENV)
            .ok()
            .into_iter()
            .flat_map(|values| {
                values
                    .split(',')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|value| !value.is_empty())
            .map(|value| validate_oauth_uri(&value, OAUTH_AUTHORIZATION_SERVERS_ENV))
            .collect::<Result<Vec<_>>>()?;
        validate_trusted_proxy_configuration(
            policy.trusted_proxy_auth,
            trusted_proxy_cidrs_configured,
            policy.protected_resource.is_some(),
            !policy.authorization_servers.is_empty(),
        )?;
        Ok(policy)
    }

    fn new(full_token: Option<String>, read_token: Option<String>) -> Result<Self> {
        if full_token
            .as_deref()
            .zip(read_token.as_deref())
            .is_some_and(|(full, read)| token_matches(full, read))
        {
            bail!("full and read-only HTTP tokens must be different");
        }
        Ok(Self {
            full_token,
            read_token,
            trusted_proxy_auth: false,
            protected_resource: None,
            authorization_servers: Vec::new(),
        })
    }

    pub(super) fn configured(&self) -> bool {
        self.full_token.is_some() || self.read_token.is_some() || self.trusted_proxy_auth
    }

    pub(super) fn authorize(&self, provided: Option<&str>) -> Option<HttpAuthContext> {
        if !self.configured() {
            return Some(HttpAuthContext {
                principal: "local-unauthenticated".to_string(),
                scopes: full_scopes(),
            });
        }
        let provided = provided?;
        if self
            .full_token
            .as_deref()
            .is_some_and(|expected| token_matches(expected, provided))
        {
            Some(HttpAuthContext {
                principal: token_principal("full", provided),
                scopes: full_scopes(),
            })
        } else if self
            .read_token
            .as_deref()
            .is_some_and(|expected| token_matches(expected, provided))
        {
            Some(HttpAuthContext {
                principal: token_principal("read", provided),
                scopes: HashSet::from(["memory:read".to_string()]),
            })
        } else {
            None
        }
    }

    pub(super) fn authorize_request(
        &self,
        provided: Option<&str>,
        peer: Option<IpAddr>,
        headers: &HashMap<String, String>,
        security_policy: &HttpSecurityPolicy,
    ) -> Result<Option<HttpAuthContext>> {
        if provided.is_some() || !self.trusted_proxy_auth {
            return Ok(self.authorize(provided));
        }
        let Some(peer) = peer.filter(|peer| security_policy.trusted_proxy(*peer)) else {
            return Ok(None);
        };
        let principal = headers
            .get("x-dukememory-principal")
            .map(String::as_str)
            .map(validate_proxy_principal)
            .transpose()?;
        let scopes = headers
            .get("x-dukememory-scopes")
            .map(String::as_str)
            .unwrap_or_default()
            .split_ascii_whitespace()
            .filter(|scope| {
                matches!(
                    *scope,
                    "memory:read" | "memory:write" | "memory:maintenance" | "memory:filesystem"
                )
            })
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let Some(principal) = principal else {
            return Ok(None);
        };
        if scopes.is_empty() {
            return Ok(None);
        }
        Ok(Some(HttpAuthContext {
            principal: token_principal("proxy", &format!("{peer}:{principal}")),
            scopes,
        }))
    }

    pub(super) fn protected_resource_metadata(&self) -> Option<serde_json::Value> {
        let resource = self.protected_resource.as_deref()?;
        if self.authorization_servers.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "resource": resource,
            "authorization_servers": &self.authorization_servers,
            "bearer_methods_supported": ["header"],
            "scopes_supported": [
                "memory:read",
                "memory:write",
                "memory:maintenance",
                "memory:filesystem"
            ]
        }))
    }

    pub(super) fn resource_metadata_url(&self) -> Option<String> {
        self.protected_resource_metadata()?;
        let mut resource = reqwest::Url::parse(self.protected_resource.as_deref()?).ok()?;
        let resource_path = resource.path().trim_matches('/');
        let metadata_path = if resource_path.is_empty() {
            "/.well-known/oauth-protected-resource".to_string()
        } else {
            format!("/.well-known/oauth-protected-resource/{resource_path}")
        };
        resource.set_path(&metadata_path);
        Some(resource.to_string().trim_end_matches('/').to_string())
    }

    pub(super) fn resource_metadata_path_matches(&self, path: &str) -> bool {
        self.resource_metadata_url()
            .and_then(|url| reqwest::Url::parse(&url).ok())
            .is_some_and(|url| url.path() == path)
    }

    #[cfg(test)]
    pub(super) fn oauth_test_policy() -> Self {
        Self {
            full_token: None,
            read_token: None,
            trusted_proxy_auth: true,
            protected_resource: Some("https://memory.example.com".to_string()),
            authorization_servers: vec!["https://issuer.example.com".to_string()],
        }
    }
}

fn validate_trusted_proxy_configuration(
    enabled: bool,
    cidrs_configured: bool,
    resource_configured: bool,
    authorization_servers_configured: bool,
) -> Result<()> {
    if enabled && (!cidrs_configured || !resource_configured || !authorization_servers_configured) {
        bail!(
            "{TRUSTED_PROXY_AUTH_ENV} requires {TRUSTED_PROXY_CIDRS_ENV}, DUKEMEMORY_PUBLIC_ORIGIN, and {OAUTH_AUTHORIZATION_SERVERS_ENV}"
        );
    }
    Ok(())
}

fn validate_oauth_uri(value: &str, name: &str) -> Result<String> {
    let url = reqwest::Url::parse(value.trim()).with_context(|| format!("invalid {name} URL"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("{name} entries must be absolute https URLs without credentials, query, or fragment");
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn validate_proxy_principal(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'@' | b'/')
        })
    {
        bail!("invalid trusted proxy principal");
    }
    Ok(value)
}

fn env_flag(name: &str) -> Result<bool> {
    match std::env::var(name).ok().as_deref().map(str::trim) {
        None | Some("") | Some("0" | "false" | "no" | "off") => Ok(false),
        Some("1" | "true" | "yes" | "on") => Ok(true),
        Some(_) => bail!("{name} must be a boolean"),
    }
}

fn token_principal(capability: &str, token: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
    format!("token:{capability}:{}", &digest[..24])
}

struct RateLimitWindow {
    started_at: Instant,
    requests: u32,
}

pub(super) struct HttpRateLimiter {
    limit: u32,
    window: Duration,
    max_clients: usize,
    clients: std::sync::Mutex<HashMap<IpAddr, RateLimitWindow>>,
}

impl HttpRateLimiter {
    pub(super) fn from_environment() -> Result<Self> {
        let limit = std::env::var(RATE_LIMIT_ENV)
            .ok()
            .map(|value| {
                value
                    .parse::<u32>()
                    .with_context(|| format!("{RATE_LIMIT_ENV} must be an integer"))
            })
            .transpose()?
            .unwrap_or(DEFAULT_RATE_LIMIT_PER_MINUTE);
        if limit == 0 {
            bail!("{RATE_LIMIT_ENV} must be greater than zero");
        }
        let max_clients = std::env::var(RATE_LIMIT_MAX_CLIENTS_ENV)
            .ok()
            .map(|value| {
                value
                    .parse::<usize>()
                    .with_context(|| format!("{RATE_LIMIT_MAX_CLIENTS_ENV} must be an integer"))
            })
            .transpose()?
            .unwrap_or(MAX_RATE_LIMIT_CLIENTS);
        if max_clients == 0 {
            bail!("{RATE_LIMIT_MAX_CLIENTS_ENV} must be greater than zero");
        }
        Ok(Self::new(limit, Duration::from_secs(60), max_clients))
    }

    fn new(limit: u32, window: Duration, max_clients: usize) -> Self {
        Self {
            limit,
            window,
            max_clients: max_clients.max(1),
            clients: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn retry_after_seconds(&self, client: IpAddr) -> Result<Option<u64>> {
        let now = Instant::now();
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP rate limiter lock was poisoned"))?;
        if !clients.contains_key(&client) && clients.len() >= self.max_clients {
            clients.retain(|_, entry| now.duration_since(entry.started_at) < self.window);
        }
        if !clients.contains_key(&client)
            && clients.len() >= self.max_clients
            && let Some(oldest) = clients
                .iter()
                .min_by_key(|(_, entry)| entry.started_at)
                .map(|(address, _)| *address)
        {
            clients.remove(&oldest);
        }
        let entry = clients.entry(client).or_insert(RateLimitWindow {
            started_at: now,
            requests: 0,
        });
        let elapsed = now.duration_since(entry.started_at);
        if elapsed >= self.window {
            entry.started_at = now;
            entry.requests = 0;
        }
        if entry.requests >= self.limit {
            return Ok(Some(
                self.window
                    .saturating_sub(now.duration_since(entry.started_at))
                    .as_secs()
                    .max(1),
            ));
        }
        entry.requests += 1;
        Ok(None)
    }

    #[cfg(test)]
    fn client_count(&self) -> usize {
        self.clients
            .lock()
            .map(|clients| clients.len())
            .unwrap_or(0)
    }
}

#[derive(Default)]
struct ConcurrencyState {
    total: usize,
    clients: HashMap<IpAddr, usize>,
}

pub(super) struct HttpConcurrencyLimiter {
    maximum: usize,
    maximum_per_client: usize,
    state: std::sync::Mutex<ConcurrencyState>,
}

pub(super) struct HttpConcurrencyPermit<'a> {
    limiter: &'a HttpConcurrencyLimiter,
    client: IpAddr,
}

impl HttpConcurrencyLimiter {
    pub(super) fn from_environment() -> Result<Self> {
        let maximum =
            positive_usize_env(MAX_CONCURRENT_REQUESTS_ENV, DEFAULT_MAX_CONCURRENT_REQUESTS)?;
        let maximum_per_client = positive_usize_env(
            MAX_CONCURRENT_PER_CLIENT_ENV,
            DEFAULT_MAX_CONCURRENT_PER_CLIENT,
        )?;
        if maximum_per_client > maximum {
            bail!("{MAX_CONCURRENT_PER_CLIENT_ENV} must not exceed {MAX_CONCURRENT_REQUESTS_ENV}");
        }
        Ok(Self::new(maximum, maximum_per_client))
    }

    fn new(maximum: usize, maximum_per_client: usize) -> Self {
        Self {
            maximum: maximum.max(1),
            maximum_per_client: maximum_per_client.max(1).min(maximum.max(1)),
            state: std::sync::Mutex::new(ConcurrencyState::default()),
        }
    }

    pub(super) fn try_acquire(&self, client: IpAddr) -> Result<Option<HttpConcurrencyPermit<'_>>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP concurrency limiter lock was poisoned"))?;
        let client_count = state.clients.get(&client).copied().unwrap_or(0);
        if state.total >= self.maximum || client_count >= self.maximum_per_client {
            return Ok(None);
        }
        state.total += 1;
        *state.clients.entry(client).or_default() += 1;
        Ok(Some(HttpConcurrencyPermit {
            limiter: self,
            client,
        }))
    }

    fn release(&self, client: IpAddr) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.total = state.total.saturating_sub(1);
        if let Some(count) = state.clients.get_mut(&client) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.clients.remove(&client);
            }
        }
    }
}

impl Drop for HttpConcurrencyPermit<'_> {
    fn drop(&mut self) {
        self.limiter.release(self.client);
    }
}

fn positive_usize_env(name: &str, default: usize) -> Result<usize> {
    let value = std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("{name} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default);
    if value == 0 {
        bail!("{name} must be greater than zero");
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IpCidr {
    network: IpAddr,
    prefix: u8,
}

impl IpCidr {
    fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (address, prefix) = value
            .split_once('/')
            .map_or((value, None), |(address, prefix)| (address, Some(prefix)));
        let network = address
            .parse::<IpAddr>()
            .with_context(|| format!("invalid trusted proxy address: {value}"))?;
        let maximum = if network.is_ipv4() { 32 } else { 128 };
        let prefix = prefix
            .map(|prefix| {
                prefix
                    .parse::<u8>()
                    .with_context(|| format!("invalid trusted proxy prefix: {value}"))
            })
            .transpose()?
            .unwrap_or(maximum);
        if prefix > maximum {
            bail!("trusted proxy prefix exceeds {maximum} bits: {value}");
        }
        Ok(Self { network, prefix })
    }

    fn contains(self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => prefix_matches(
                u32::from(network) as u128,
                u32::from(address) as u128,
                self.prefix,
                32,
            ),
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                prefix_matches(u128::from(network), u128::from(address), self.prefix, 128)
            }
            _ => false,
        }
    }
}

fn prefix_matches(network: u128, address: u128, prefix: u8, bits: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    let shift = u32::from(bits - prefix);
    (network >> shift) == (address >> shift)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HttpAuthority {
    host: String,
    port: Option<u16>,
}

#[derive(Debug, Clone)]
pub(super) struct HttpSecurityPolicy {
    allowed_hosts: HashSet<HttpAuthority>,
    allowed_origins: HashSet<String>,
    trusted_proxies: Vec<IpCidr>,
    enforce_host_allowlist: bool,
}

impl HttpSecurityPolicy {
    pub(super) fn from_environment(requested_host: &str, bound: SocketAddr) -> Result<Self> {
        let allowed_origins = std::env::var("DUKEMEMORY_HTTP_ALLOWED_ORIGINS").ok();
        let public_origin = std::env::var("DUKEMEMORY_PUBLIC_ORIGIN").ok();
        let trusted_proxies = std::env::var(TRUSTED_PROXY_CIDRS_ENV).ok();
        Self::new(
            requested_host,
            bound,
            allowed_origins.as_deref(),
            public_origin.as_deref(),
            trusted_proxies.as_deref(),
        )
    }

    fn new(
        requested_host: &str,
        bound: SocketAddr,
        allowed_origins: Option<&str>,
        public_origin: Option<&str>,
        trusted_proxies: Option<&str>,
    ) -> Result<Self> {
        let mut policy = Self {
            allowed_hosts: HashSet::new(),
            allowed_origins: HashSet::new(),
            trusted_proxies: trusted_proxies
                .into_iter()
                .flat_map(|values| values.split(','))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(IpCidr::parse)
                .collect::<Result<Vec<_>>>()?,
            enforce_host_allowlist: !bound.ip().is_unspecified(),
        };

        if !bound.ip().is_unspecified() {
            policy.add_bound_authority(&bound.ip().to_string(), bound.port());
        }
        if bound.ip().is_loopback() {
            policy.add_bound_authority("localhost", bound.port());
        }
        if super::is_loopback_host(requested_host) {
            policy.add_bound_authority(requested_host, bound.port());
        }

        for origin in allowed_origins
            .into_iter()
            .flat_map(|origins| origins.split(','))
            .chain(public_origin)
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
        {
            policy.add_configured_origin(origin)?;
        }
        if !policy.allowed_hosts.is_empty() {
            policy.enforce_host_allowlist = true;
        }
        Ok(policy)
    }

    fn add_bound_authority(&mut self, host: &str, port: u16) {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return;
        }
        self.allowed_hosts.insert(HttpAuthority {
            host: host.clone(),
            port: Some(port),
        });
        // Preserve the existing local API convention where Host omits the
        // ephemeral listener port. The hostname itself must still be an exact
        // loopback match, so this does not reopen DNS rebinding.
        self.allowed_hosts.insert(HttpAuthority {
            host: host.clone(),
            port: None,
        });
        for origin in [
            format_origin("http", &host, None),
            format_origin("http", &host, Some(port)),
        ] {
            self.allowed_origins.insert(origin);
        }
    }

    fn add_configured_origin(&mut self, origin: &str) -> Result<()> {
        let (origin, authority, default_port) = parse_origin(origin)?;
        self.allowed_origins.insert(origin);
        self.allowed_hosts.insert(authority.clone());
        if authority.port.is_none() {
            self.allowed_hosts.insert(HttpAuthority {
                host: authority.host,
                port: default_port,
            });
        }
        Ok(())
    }

    pub(super) fn host_allowed(&self, host: Option<&str>) -> bool {
        if !self.enforce_host_allowlist {
            return true;
        }
        host.and_then(parse_host_header)
            .is_some_and(|authority| self.allowed_hosts.contains(&authority))
    }

    pub(super) fn origin_allowed(&self, origin: &str) -> bool {
        parse_origin(origin)
            .ok()
            .is_some_and(|(origin, _, _)| self.allowed_origins.contains(&origin))
    }

    pub(super) fn client_ip(
        &self,
        peer: IpAddr,
        headers: &HashMap<String, String>,
    ) -> Result<IpAddr> {
        if !self.trusted_proxy(peer) {
            return Ok(peer);
        }
        let Some(forwarded) = headers.get("x-forwarded-for") else {
            return Ok(peer);
        };
        let mut chain = forwarded
            .split(',')
            .map(parse_forwarded_ip)
            .collect::<Result<Vec<_>>>()?;
        chain.push(peer);
        Ok(chain
            .into_iter()
            .rev()
            .find(|address| !self.trusted_proxy(*address))
            .unwrap_or(peer))
    }

    pub(super) fn trusted_proxy(&self, address: IpAddr) -> bool {
        self.trusted_proxies
            .iter()
            .any(|network| network.contains(address))
    }
}

fn parse_forwarded_ip(value: &str) -> Result<IpAddr> {
    let value = value.trim();
    if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_control()) {
        bail!("invalid X-Forwarded-For client address");
    }
    value
        .parse::<IpAddr>()
        .with_context(|| "invalid X-Forwarded-For client address")
}

fn parse_origin(value: &str) -> Result<(String, HttpAuthority, Option<u16>)> {
    let url = reqwest::Url::parse(value.trim()).context("invalid HTTP origin")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("HTTP origin must be an exact http(s) origin without credentials or a path");
    }
    let host = url
        .host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .context("HTTP origin must include a host")?;
    Ok((
        url.origin().ascii_serialization(),
        HttpAuthority {
            host,
            port: url.port(),
        },
        url.port_or_known_default(),
    ))
}

fn parse_host_header(value: &str) -> Option<HttpAuthority> {
    let value = value.trim();
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://{value}")).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    Some(HttpAuthority {
        host: url.host_str()?.trim_end_matches('.').to_ascii_lowercase(),
        port: url.port(),
    })
}

fn format_origin(scheme: &str, host: &str, port: Option<u16>) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

pub(super) fn resolve_auth_token(
    inline_token: Option<&str>,
    token_file: Option<&Path>,
) -> Result<Option<String>> {
    if inline_token.is_some() && token_file.is_some() {
        bail!("use either --auth-token or --auth-token-file, not both");
    }
    if let Some(path) = token_file {
        return read_private_token_file(path).map(Some);
    }
    Ok(inline_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned))
}

pub(super) fn read_private_token_file(path: &Path) -> Result<String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("failed to open private HTTP token file {}", path.display()))?;
    validate_token_file_permissions(&file, path)?;
    let mut token = String::new();
    file.read_to_string(&mut token)
        .with_context(|| format!("failed to read HTTP token file {}", path.display()))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("HTTP token file {} is empty", path.display());
    }
    Ok(token.to_string())
}

#[cfg(unix)]
fn validate_token_file_permissions(file: &fs::File, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = file
        .metadata()
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
fn validate_token_file_permissions(file: &fs::File, path: &Path) -> Result<()> {
    file.metadata()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_policy_rejects_dns_rebinding_even_when_origin_matches_host() {
        let policy = HttpSecurityPolicy::new(
            "127.0.0.1",
            "127.0.0.1:8765".parse().unwrap(),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(policy.host_allowed(Some("127.0.0.1:8765")));
        assert!(policy.host_allowed(Some("localhost")));
        assert!(policy.origin_allowed("http://127.0.0.1:8765"));
        assert!(policy.origin_allowed("http://localhost"));
        assert!(!policy.host_allowed(Some("attacker.example")));
        assert!(!policy.origin_allowed("http://attacker.example"));
        assert!(!policy.origin_allowed("null"));
    }

    #[test]
    fn configured_public_origin_is_exact_and_supplies_an_allowed_host() {
        let policy = HttpSecurityPolicy::new(
            "127.0.0.1",
            "127.0.0.1:8765".parse().unwrap(),
            Some("https://memory.example.com"),
            None,
            None,
        )
        .unwrap();
        assert!(policy.host_allowed(Some("memory.example.com")));
        assert!(policy.host_allowed(Some("memory.example.com:443")));
        assert!(policy.origin_allowed("https://memory.example.com"));
        assert!(!policy.host_allowed(Some("memory.example.com.attacker.test")));
        assert!(!policy.origin_allowed("https://memory.example.com/path"));
    }

    #[test]
    fn malformed_configured_origin_fails_closed() {
        assert!(
            HttpSecurityPolicy::new(
                "127.0.0.1",
                "127.0.0.1:8765".parse().unwrap(),
                Some("https://memory.example.com/path"),
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn auth_policy_distinguishes_full_and_read_only_capabilities() {
        let policy = HttpAuthPolicy::new(
            Some("full-secret-token".to_string()),
            Some("read-secret-token".to_string()),
        )
        .unwrap();
        let full = policy.authorize(Some("full-secret-token")).unwrap();
        let read = policy.authorize(Some("read-secret-token")).unwrap();
        assert!(full.allows("memory:read"));
        assert!(full.allows("memory:write"));
        assert!(full.allows("memory:maintenance"));
        assert!(full.allows("memory:filesystem"));
        assert!(read.allows("memory:read"));
        assert!(!read.allows("memory:write"));
        assert_ne!(full.principal, read.principal);
        assert!(full.principal.starts_with("token:full:"));
        assert!(read.principal.starts_with("token:read:"));
        assert_eq!(policy.authorize(Some("wrong-token")), None);
        assert_eq!(policy.authorize(None), None);
        assert!(
            HttpAuthPolicy::new(
                Some("same-secret-token".to_string()),
                Some("same-secret-token".to_string())
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_token_reader_uses_one_inode_and_rejects_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("token-target");
        let link = directory.path().join("token-link");
        fs::write(&target, "private-secret-token\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &link).unwrap();
        assert_eq!(
            read_private_token_file(&target).unwrap(),
            "private-secret-token"
        );
        assert!(read_private_token_file(&link).is_err());
    }

    #[test]
    fn trusted_proxy_auth_binds_scopes_and_principal_without_accepting_spoofed_headers() {
        let mut auth = HttpAuthPolicy::new(None, None).unwrap();
        auth.trusted_proxy_auth = true;
        let security = HttpSecurityPolicy::new(
            "127.0.0.1",
            "127.0.0.1:8765".parse().unwrap(),
            None,
            None,
            Some("10.0.0.0/8"),
        )
        .unwrap();
        let read_headers = HashMap::from([
            (
                "x-dukememory-principal".to_string(),
                "oidc:user-123".to_string(),
            ),
            ("x-dukememory-scopes".to_string(), "memory:read".to_string()),
        ]);
        let read = auth
            .authorize_request(
                None,
                Some("10.0.0.8".parse().unwrap()),
                &read_headers,
                &security,
            )
            .unwrap()
            .unwrap();
        assert!(read.allows("memory:read"));
        assert!(!read.allows("memory:write"));
        assert!(read.principal.starts_with("token:proxy:"));
        assert!(
            auth.authorize_request(
                None,
                Some("192.0.2.8".parse().unwrap()),
                &read_headers,
                &security,
            )
            .unwrap()
            .is_none()
        );

        let mut write_headers = read_headers;
        write_headers.insert(
            "x-dukememory-scopes".to_string(),
            "memory:read memory:write".to_string(),
        );
        let write = auth
            .authorize_request(
                None,
                Some("10.0.0.8".parse().unwrap()),
                &write_headers,
                &security,
            )
            .unwrap()
            .unwrap();
        assert!(write.allows("memory:read"));
        assert!(write.allows("memory:write"));
        assert!(!write.allows("memory:maintenance"));
    }

    #[test]
    fn oauth_protected_resource_metadata_is_https_and_scope_explicit() {
        let mut auth = HttpAuthPolicy::new(None, None).unwrap();
        auth.protected_resource = Some("https://memory.example.com/api/v1".to_string());
        auth.authorization_servers = vec!["https://issuer.example.com/tenant".to_string()];
        let metadata = auth.protected_resource_metadata().unwrap();
        assert_eq!(metadata["resource"], "https://memory.example.com/api/v1");
        assert_eq!(
            metadata["authorization_servers"][0],
            "https://issuer.example.com/tenant"
        );
        assert!(
            metadata["scopes_supported"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("memory:read"))
        );
        assert!(
            metadata["scopes_supported"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("memory:maintenance"))
        );
        assert!(
            metadata["scopes_supported"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("memory:filesystem"))
        );
        assert_eq!(
            auth.resource_metadata_url().as_deref(),
            Some("https://memory.example.com/.well-known/oauth-protected-resource/api/v1")
        );
        assert!(
            auth.resource_metadata_path_matches("/.well-known/oauth-protected-resource/api/v1")
        );
        assert!(validate_oauth_uri("http://issuer.example.com", "issuer").is_err());
        assert!(validate_oauth_uri("https://issuer.example.com?bad=1", "issuer").is_err());
    }

    #[test]
    fn trusted_proxy_auth_requires_an_explicit_proxy_allowlist() {
        let error = validate_trusted_proxy_configuration(true, false, true, true)
            .unwrap_err()
            .to_string();
        assert!(error.contains(TRUSTED_PROXY_CIDRS_ENV));
        assert!(validate_trusted_proxy_configuration(true, true, true, true).is_ok());
    }

    #[test]
    fn rate_limiter_is_bounded_per_client_and_resets_by_window() {
        let limiter = HttpRateLimiter::new(2, Duration::from_secs(60), 2);
        let first: IpAddr = "127.0.0.1".parse().unwrap();
        let second: IpAddr = "127.0.0.2".parse().unwrap();
        let third: IpAddr = "127.0.0.3".parse().unwrap();
        assert_eq!(limiter.retry_after_seconds(first).unwrap(), None);
        assert_eq!(limiter.retry_after_seconds(first).unwrap(), None);
        assert!(limiter.retry_after_seconds(first).unwrap().is_some());
        assert_eq!(limiter.retry_after_seconds(second).unwrap(), None);
        assert_eq!(limiter.retry_after_seconds(third).unwrap(), None);
        assert_eq!(limiter.client_count(), 2);

        let resetting = HttpRateLimiter::new(1, Duration::ZERO, 2);
        assert_eq!(resetting.retry_after_seconds(first).unwrap(), None);
        assert_eq!(resetting.retry_after_seconds(first).unwrap(), None);
    }

    #[test]
    fn forwarded_client_is_used_only_for_explicit_trusted_proxy_cidrs() {
        let policy = HttpSecurityPolicy::new(
            "127.0.0.1",
            "127.0.0.1:8765".parse().unwrap(),
            None,
            None,
            Some("10.0.0.0/8, 2001:db8::/32"),
        )
        .unwrap();
        let headers = HashMap::from([(
            "x-forwarded-for".to_string(),
            "198.51.100.9, 10.0.0.7".to_string(),
        )]);
        assert_eq!(
            policy
                .client_ip("10.0.0.8".parse().unwrap(), &headers)
                .unwrap(),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            policy
                .client_ip("192.0.2.4".parse().unwrap(), &headers)
                .unwrap(),
            "192.0.2.4".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn malformed_trusted_proxy_configuration_and_forwarding_fail_closed() {
        assert!(
            HttpSecurityPolicy::new(
                "127.0.0.1",
                "127.0.0.1:8765".parse().unwrap(),
                None,
                None,
                Some("10.0.0.0/99"),
            )
            .is_err()
        );
        let policy = HttpSecurityPolicy::new(
            "127.0.0.1",
            "127.0.0.1:8765".parse().unwrap(),
            None,
            None,
            Some("10.0.0.0/8"),
        )
        .unwrap();
        let headers =
            HashMap::from([("x-forwarded-for".to_string(), "not-an-address".to_string())]);
        assert!(
            policy
                .client_ip("10.0.0.8".parse().unwrap(), &headers)
                .is_err()
        );
    }

    #[test]
    fn concurrency_limiter_enforces_global_and_per_client_bounds() {
        let limiter = HttpConcurrencyLimiter::new(3, 2);
        let first: IpAddr = "192.0.2.1".parse().unwrap();
        let second: IpAddr = "192.0.2.2".parse().unwrap();
        let first_permit = limiter.try_acquire(first).unwrap().unwrap();
        let second_permit = limiter.try_acquire(first).unwrap().unwrap();
        assert!(limiter.try_acquire(first).unwrap().is_none());
        let third_permit = limiter.try_acquire(second).unwrap().unwrap();
        assert!(limiter.try_acquire(second).unwrap().is_none());
        drop(first_permit);
        assert!(limiter.try_acquire(second).unwrap().is_some());
        drop(second_permit);
        drop(third_permit);
    }
}
