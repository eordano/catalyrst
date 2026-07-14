use std::net::{IpAddr, SocketAddr};

const INTERNAL_HOST_SUFFIXES: [&str; 8] = [
    ".localhost",
    ".local",
    ".internal",
    ".intranet",
    ".lan",
    ".home",
    ".corp",
    ".home.arpa",
];

/// True for anything a server must not be talked into fetching: reserved and private IP
/// ranges, single-label names, and the reserved internal DNS suffixes.
///
/// Two check classes are merged because neither alone is sufficient. The `IpAddr` range
/// checks catch every literal form the standard parser accepts (including IPv4-mapped
/// IPv6), but reject nothing that is not a literal; the string checks catch names that
/// never reach a resolver (`router`, `nas.local`) and the URL-spec IPv4 forms
/// (`0x7f000001`, `0177.0.0.1`, `127.1`) that `IpAddr` refuses to parse.
pub fn is_private_or_internal_host(host: &str) -> bool {
    let host = host.trim();
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let normalized = unbracketed.trim_end_matches('.').to_ascii_lowercase();

    if normalized.is_empty() {
        return true;
    }
    if let Ok(ip) = normalized.parse::<IpAddr>() {
        return is_blocked_ip(ip);
    }
    if let Some(ip) = spec_normalized_ip(&normalized) {
        return is_blocked_ip(ip);
    }
    is_internal_hostname(&normalized)
}

/// True only for an `http`/`https` URL whose host is publicly routable.
///
/// A clicked link reaches an unrestricted URL opener on the viewer's machine and a
/// server-side fetch reaches the machine's own network, so anything that is not plainly
/// safe fails closed. Whitespace anywhere in the target is rejected outright rather than
/// percent-encoded away, since it is only ever there to smuggle a second URL past a
/// parser.
pub fn is_safe_http_url(url: &str) -> bool {
    let trimmed = url.trim_matches(is_js_whitespace);
    if trimmed.chars().any(is_js_whitespace) {
        return false;
    }
    let Ok(parsed) = url::Url::parse(trimmed) else {
        return false;
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return false;
    }
    match parsed.host_str() {
        Some(host) => !is_private_or_internal_host(host),
        None => false,
    }
}

/// Resolves `url`'s host and returns the address to pin the connection to, or `None` if
/// the host is not publicly routable.
///
/// Pinning is what closes the DNS-rebinding window: the guard's verdict and the socket
/// the request actually opens come from the same lookup, so a name that answers
/// differently a millisecond later cannot be dialled. Every answer must be public -- one
/// blocked address in the set rejects the host.
pub async fn resolve_and_pin(url: &reqwest::Url) -> Option<SocketAddr> {
    let host = url.host_str()?;
    let port = url.port_or_known_default().unwrap_or(80);
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    if let Ok(ip) = bare.parse::<IpAddr>() {
        return (!is_blocked_ip(ip)).then_some(SocketAddr::new(ip, port));
    }
    if is_private_or_internal_host(host) {
        return None;
    }

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((bare, port)).await.ok()?.collect();
    if addrs.is_empty() || addrs.iter().any(|a| is_blocked_ip(a.ip())) {
        return None;
    }
    Some(addrs[0])
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || v4.octets()[0] == 0
                || {
                    let o = v4.octets();
                    o[0] == 100 && (o[1] & 0xc0) == 0x40
                }
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn spec_normalized_ip(host: &str) -> Option<IpAddr> {
    let parsed = url::Url::parse(&format!("http://{host}/")).ok()?;
    let normalized = parsed.host_str()?;
    if normalized == host {
        return None;
    }
    normalized
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(normalized)
        .parse::<IpAddr>()
        .ok()
}

fn is_internal_hostname(host: &str) -> bool {
    if let Some([a, b, _, _]) = parse_dotted_quad(host) {
        return a == 0
            || a == 127
            || a == 10
            || (a == 169 && b == 254)
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 100 && (64..=127).contains(&b));
    }

    if host.contains(':') {
        return host == "::1"
            || host == "::"
            || ["fe8", "fe9", "fea", "feb"]
                .iter()
                .any(|p| host.starts_with(p))
            || host.starts_with("fc")
            || host.starts_with("fd")
            || host.starts_with("::ffff:");
    }

    if !host.contains('.') {
        return true;
    }
    INTERNAL_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix))
}

fn parse_dotted_quad(host: &str) -> Option<[u32; 4]> {
    let mut octets = [0u32; 4];
    let mut parts = host.split('.');
    for octet in &mut octets {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *octet = p.parse().ok()?;
    }
    parts.next().is_none().then_some(octets)
}

fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_ranges_and_internal_names_are_blocked() {
        for host in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.2.1",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "fc00::1",
            "[::1]",
            "[::ffff:127.0.0.1]",
            "localhost",
            "router",
            "svc.internal",
            "office.intranet",
            "box.home",
            "vpn.corp",
            "gw.home.arpa",
            "app.localhost",
            "printer.lan",
            "nas.local",
            "localhost.",
            "nas.local.",
            "router.",
            "127.0.0.1.",
            "LOCALHOST",
            "NAS.Local",
        ] {
            assert!(is_private_or_internal_host(host), "{host} must be blocked");
        }
    }

    #[test]
    fn public_hosts_are_allowed() {
        for host in [
            "8.8.8.8",
            "example.com",
            "example.com.",
            "decentraland.org..",
            "172.15.0.1",
            "172.32.0.1",
            "100.63.0.1",
            "100.128.0.1",
            "events.decentraland.org",
            "2600::1",
            "[2600::1]",
        ] {
            assert!(
                !is_private_or_internal_host(host),
                "{host} must be reachable"
            );
        }
    }

    #[test]
    fn obfuscated_ip_literals_are_blocked() {
        for host in [
            "2130706433",
            "0x7f000001",
            "0177.0.0.1",
            "127.1",
            "0x7f.1",
            "017700000001",
        ] {
            assert!(
                is_private_or_internal_host(host),
                "{host} must resolve to loopback and be blocked"
            );
        }
    }

    #[test]
    fn safe_urls_pass() {
        for url in [
            "https://decentraland.org/",
            "http://example.com/path?q=1",
            "https://example.com./",
            "https://8.8.8.8/",
            "https://[2600::1]/",
        ] {
            assert!(is_safe_http_url(url), "{url} must be safe");
        }
    }

    #[test]
    fn unsafe_schemes_and_hosts_are_rejected() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "smb://attacker/share",
            "ftp://example.com/",
            "//example.com/",
            "example.com",
            "",
            "http://169.254.169.254/latest/meta-data/",
            "http://2130706433/",
            "http://0x7f000001/",
            "http://0177.0.0.1/",
            "http://192.168.1.1/",
            "http://localhost:8080/",
            "http://router/",
            "http://printer.lan/",
            "http://nas.local/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://localhost./",
            "http://nas.local./",
            "http://router./",
            "http://10.0.0.1/",
            "http://100.100.0.1/",
        ] {
            assert!(!is_safe_http_url(url), "{url} must be rejected");
        }
    }

    #[test]
    fn whitespace_smuggling_is_rejected() {
        for url in [
            "https://example.com/ javascript:alert(1)",
            "https://exa\tmple.com/",
            "https://example.com/\nhttp://127.0.0.1/",
            "https://example.com/\u{2028}x",
            "java\nscript:alert(1)",
        ] {
            assert!(!is_safe_http_url(url), "{url} must be rejected");
        }
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_not_rejected() {
        assert!(is_safe_http_url("  https://decentraland.org/  "));
        assert!(is_safe_http_url("\n\thttps://decentraland.org/\r\n"));
    }

    #[tokio::test]
    async fn pinning_rejects_literal_private_addresses() {
        for url in [
            "http://127.0.0.1/x",
            "http://169.254.169.254/latest/",
            "http://[::1]/x",
            "http://[::ffff:10.0.0.1]/x",
            "http://router/x",
            "http://svc.internal/x",
        ] {
            let parsed = reqwest::Url::parse(url).unwrap();
            assert!(
                resolve_and_pin(&parsed).await.is_none(),
                "{url} must not be pinned"
            );
        }
    }

    #[tokio::test]
    async fn pinning_accepts_a_public_literal_and_keeps_the_port() {
        let parsed = reqwest::Url::parse("http://8.8.8.8:8080/x").unwrap();
        let pinned = resolve_and_pin(&parsed).await.expect("public literal pins");
        assert_eq!(pinned, "8.8.8.8:8080".parse::<SocketAddr>().unwrap());

        let parsed = reqwest::Url::parse("https://8.8.4.4/x").unwrap();
        let pinned = resolve_and_pin(&parsed).await.expect("public literal pins");
        assert_eq!(pinned.port(), 443, "https must pin the default port");
    }

    #[tokio::test]
    async fn pinning_accepts_a_public_ipv6_literal() {
        let parsed = reqwest::Url::parse("http://[2600::1]:81/x").unwrap();
        let pinned = resolve_and_pin(&parsed).await.expect("public literal pins");
        assert_eq!(pinned, "[2600::1]:81".parse::<SocketAddr>().unwrap());
    }
}
