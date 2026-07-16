use super::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

pub(crate) fn blocking_http_client(
    raw_url: &str,
    timeout: Duration,
) -> Result<(reqwest::blocking::Client, reqwest::Url)> {
    let (url, host, addresses, loopback) = validate_http_target(raw_url)?;
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&host, &addresses);
    if loopback {
        builder = builder.no_proxy();
    }
    Ok((builder.build()?, url))
}

fn validate_http_target(raw_url: &str) -> Result<(reqwest::Url, String, Vec<SocketAddr>, bool)> {
    dukememory::protocol::validate_egress_url_shape(raw_url)?;
    let url = reqwest::Url::parse(raw_url).context("invalid HTTP endpoint URL")?;
    let host = url
        .host_str()
        .context("egress endpoint must include a host")?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let port = url
        .port_or_known_default()
        .context("egress endpoint must include a valid port")?;
    let explicitly_allowed = configured_allowed_hosts().contains(&host);
    let localhost_name = host == "localhost";
    let literal_ip = host.parse::<IpAddr>().ok();
    let mut addresses = if let Some(ip) = literal_ip {
        vec![SocketAddr::new(ip, port)]
    } else {
        (host.as_str(), port)
            .to_socket_addrs()
            .with_context(|| format!("failed to resolve egress host {host}"))?
            .collect::<Vec<_>>()
    };
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        bail!("egress host {host} resolved to no addresses");
    }

    let all_loopback = addresses.iter().all(|address| address.ip().is_loopback());
    if localhost_name && !all_loopback {
        bail!("localhost egress endpoint resolved outside the loopback network");
    }
    if !explicitly_allowed && !localhost_name {
        for address in &addresses {
            if !address.ip().is_loopback() && !is_public_ip(address.ip()) {
                bail!(
                    "egress endpoint resolves to blocked address {}; add the exact host to DUKEMEMORY_EGRESS_ALLOW_HOSTS only if this private destination is intentional",
                    address.ip()
                );
            }
            if address.ip().is_loopback() && literal_ip.is_none() {
                bail!(
                    "egress hostname {host} resolves to loopback; use localhost or explicitly allow the host"
                );
            }
        }
    }

    Ok((url, host, addresses, all_loopback))
}

fn configured_allowed_hosts() -> BTreeSet<String> {
    std::env::var("DUKEMEMORY_EGRESS_ALLOW_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
        .collect()
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    if address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_unspecified()
    {
        return false;
    }
    !matches!(
        octets,
        [0, ..]
            | [100, 64..=127, ..]
            | [192, 0, 0, ..]
            | [192, 88, 99, ..]
            | [198, 18..=19, ..]
            | [240..=255, ..]
    )
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if address.is_loopback() || address.is_multicast() || address.is_unspecified() {
        return false;
    }
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    let unique_local = segments[0] & 0xfe00 == 0xfc00;
    let link_local = segments[0] & 0xffc0 == 0xfe80;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    !(unique_local || link_local || documentation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egress_policy_allows_explicit_loopback_model_endpoints() {
        let (_, host, addresses, loopback) =
            validate_http_target("http://localhost:11434/api/chat").unwrap();
        assert_eq!(host, "localhost");
        assert!(loopback);
        assert!(addresses.iter().all(|address| address.ip().is_loopback()));

        assert!(validate_http_target("http://127.0.0.1:11434/api/chat").is_ok());
    }

    #[test]
    fn egress_policy_blocks_private_and_metadata_addresses() {
        for endpoint in [
            "http://10.0.0.1/model",
            "http://172.16.0.1/model",
            "http://192.168.1.1/model",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.0.1/model",
            "http://[fe80::1]/model",
        ] {
            assert!(
                validate_http_target(endpoint).is_err(),
                "endpoint={endpoint}"
            );
        }
    }

    #[test]
    fn egress_policy_rejects_unsafe_url_shapes() {
        assert!(validate_http_target("file:///etc/passwd").is_err());
        assert!(validate_http_target("http://user:secret@localhost:11434").is_err());
        assert!(validate_http_target("http://localhost:11434/#fragment").is_err());
    }
}
