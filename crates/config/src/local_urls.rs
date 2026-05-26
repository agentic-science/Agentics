/// Build the local API base URL from an explicit host and port.
pub fn local_api_base_url(api_host: &str, api_port: u16) -> String {
    format!("http://{api_host}:{api_port}")
}

/// Build the local web base URL from an explicit host and port.
pub fn local_web_base_url(web_host: &str, web_port: u16) -> String {
    format!("http://{web_host}:{web_port}")
}

/// Returns whether a configured bind host is loopback-only.
pub(crate) fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<std::net::IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

/// Validates cookie name invariants for this contract.
pub(crate) fn validate_cookie_name(value: &str, field: &str) -> anyhow::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    if !value
        .bytes()
        .all(|byte| matches!(byte, b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'))
    {
        anyhow::bail!("{field} contains characters that are not valid in a cookie name");
    }
    Ok(())
}
