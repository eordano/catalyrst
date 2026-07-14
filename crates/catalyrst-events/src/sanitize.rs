//! Read-side sanitizer for event descriptions: a clicked `<link>` target reaches an unrestricted `Application.OpenURL` on the viewer's machine, so only http(s) links to public hosts are kept; all other markup is stripped.

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

/// Neutralizes unsafe markup in a user-authored description, keeping only `<link>` tags that target http(s) URLs on public hosts.
pub fn sanitize_event_description(description: &str) -> String {
    let mut open_link_kept: Vec<bool> = Vec::new();
    let mut out = String::with_capacity(description.len());
    let mut i = 0;
    while i < description.len() {
        let Some(rel) = description[i..].find('<') else {
            out.push_str(&description[i..]);
            break;
        };
        let lt = i + rel;
        out.push_str(&description[i..lt]);
        match markup_tag_end(description.as_bytes(), lt) {
            Some(end) => {
                let tag = &description[lt..end];
                if is_link_close_tag(tag) {
                    if open_link_kept.pop().unwrap_or(false) {
                        out.push_str(tag);
                    }
                } else if let Some(target) = link_open_target(tag) {
                    let keep = is_safe_link_target(target);
                    open_link_kept.push(keep);
                    if keep {
                        out.push_str(tag);
                    }
                }
                i = end;
            }
            None => {
                out.push('<');
                i = lt + 1;
            }
        }
    }
    out
}

fn markup_tag_end(bytes: &[u8], lt: usize) -> Option<usize> {
    let mut j = lt + 1;
    if bytes.get(j) == Some(&b'/') {
        j += 1;
    }
    if !bytes.get(j)?.is_ascii_alphabetic() {
        return None;
    }
    j += 1;
    loop {
        match bytes.get(j)? {
            b'>' => return Some(j + 1),
            b'<' => return None,
            _ => j += 1,
        }
    }
}

fn is_link_close_tag(tag: &str) -> bool {
    let Some(rest) = strip_prefix_ignore_ascii_case(tag, "</link") else {
        return false;
    };
    let Some(inner) = rest.strip_suffix('>') else {
        return false;
    };
    inner.chars().all(is_js_whitespace)
}

fn link_open_target(tag: &str) -> Option<&str> {
    let rest = strip_prefix_ignore_ascii_case(tag, "<link")?;
    let rest = rest.strip_suffix('>')?;
    let rest = rest.trim_start_matches(is_js_whitespace);
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start_matches(is_js_whitespace);
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    match rest.find('"') {
        None => Some(rest),
        Some(q) => {
            let tail = &rest[q + 1..];
            tail.chars().all(is_js_whitespace).then_some(&rest[..q])
        }
    }
}

fn is_safe_link_target(target: &str) -> bool {
    let trimmed = target.trim_matches(is_js_whitespace);
    if trimmed.chars().any(is_js_whitespace) {
        return false;
    }

    let Ok(url) = url::Url::parse(trimmed) else {
        return false;
    };

    if url.scheme() != "https" && url.scheme() != "http" {
        return false;
    }

    match url.host_str() {
        Some(host) => !is_internal_link_host(&host.to_ascii_lowercase()),
        None => false,
    }
}

fn is_internal_link_host(hostname: &str) -> bool {
    let host = hostname
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(hostname);

    if let Some([a, b, _, _]) = parse_dotted_quad(host) {
        return a == 0
            || a == 127
            || a == 10
            || (a == 169 && b == 254) // link-local incl. cloud metadata
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 100 && (64..=127).contains(&b)); // carrier-grade NAT
    }

    if host.contains(':') {
        return host == "::1"
            || host == "::"
            // link-local fe80::/10
            || ["fe8", "fe9", "fea", "feb"].iter().any(|p| host.starts_with(p))
            // unique-local fc00::/7
            || host.starts_with("fc")
            || host.starts_with("fd")
            // IPv4-mapped
            || host.starts_with("::ffff:");
    }

    if !host.contains('.') {
        return true;
    }
    INTERNAL_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix))
}

fn strip_prefix_ignore_ascii_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
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
    fn strips_link_to_custom_protocol_keeping_inner_text() {
        assert_eq!(
            sanitize_event_description(
                "Join <link=\"decentraland://?position=0,0\">click here</link>"
            ),
            "Join click here"
        );
    }

    #[test]
    fn strips_file_and_smb_links_without_orphan_tags() {
        assert_eq!(
            sanitize_event_description(
                "a <link=\"file:///etc/passwd\">x</link> b <link=\"smb://h/s\">y</link> c"
            ),
            "a x b y c"
        );
    }

    #[test]
    fn preserves_safe_https_link_untouched() {
        assert_eq!(
            sanitize_event_description("Join <link=\"https://decentraland.org\">our site</link>"),
            "Join <link=\"https://decentraland.org\">our site</link>"
        );
    }

    #[test]
    fn preserves_safe_http_link_untouched() {
        assert_eq!(
            sanitize_event_description("<link=\"http://example.com\">x</link>"),
            "<link=\"http://example.com\">x</link>"
        );
    }

    #[test]
    fn keeps_safe_link_and_strips_unsafe_one() {
        assert_eq!(
            sanitize_event_description(
                "<link=\"https://a.com\">A</link><link=\"javascript:alert(1)\">B</link>"
            ),
            "<link=\"https://a.com\">A</link>B"
        );
    }

    #[test]
    fn strips_ambiguous_tag_with_extra_content_after_target() {
        assert_eq!(
            sanitize_event_description("<link=https://a.com onclick=x>t</link>"),
            "t"
        );
    }

    #[test]
    fn strips_link_to_cloud_metadata_ip() {
        assert_eq!(
            sanitize_event_description(
                "<link=\"http://169.254.169.254/latest/meta-data/\">x</link>"
            ),
            "x"
        );
    }

    #[test]
    fn strips_link_to_obfuscated_loopback_ip() {
        assert_eq!(
            sanitize_event_description("<link=\"http://2130706433/\">x</link>"),
            "x"
        );
    }

    #[test]
    fn strips_links_to_private_and_localhost_hosts() {
        assert_eq!(
            sanitize_event_description(
                "a <link=\"http://192.168.1.1/\">x</link> b <link=\"http://localhost:8080/\">y</link> c"
            ),
            "a x b y c"
        );
    }

    #[test]
    fn keeps_link_to_public_ip() {
        assert_eq!(
            sanitize_event_description("<link=\"https://8.8.8.8/\">x</link>"),
            "<link=\"https://8.8.8.8/\">x</link>"
        );
    }

    #[test]
    fn strips_single_label_and_reserved_suffix_hosts() {
        assert_eq!(
            sanitize_event_description(
                "a <link=\"http://router/\">x</link> b <link=\"http://printer.lan/\">y</link> c <link=\"http://nas.local/\">z</link> d"
            ),
            "a x b y c z d"
        );
    }

    #[test]
    fn removes_html_anchor_and_image_tags() {
        assert_eq!(
            sanitize_event_description(
                "<a href=\"smb://attacker/share\">x</a><img src=\"file:///etc/passwd\">"
            ),
            "x"
        );
    }

    #[test]
    fn preserves_markdown_and_comparison_operators() {
        let text = "See [our site](https://decentraland.org) for **details** — 5 < 10 and 10 > 5 and I <3 events";
        assert_eq!(sanitize_event_description(text), text);
    }

    #[test]
    fn empty_description_returned_unchanged() {
        assert_eq!(sanitize_event_description(""), "");
    }

    #[test]
    fn strips_obfuscated_hex_and_octal_loopback_ips() {
        assert_eq!(
            sanitize_event_description("<link=\"http://0x7f000001/\">x</link>"),
            "x"
        );
        assert_eq!(
            sanitize_event_description("<link=\"http://0177.0.0.1/\">y</link>"),
            "y"
        );
    }

    #[test]
    fn strips_ipv6_loopback_and_ipv4_mapped_links() {
        assert_eq!(
            sanitize_event_description("<link=\"http://[::1]/\">x</link>"),
            "x"
        );
        assert_eq!(
            sanitize_event_description("<link=\"http://[::ffff:127.0.0.1]/\">y</link>"),
            "y"
        );
    }

    #[test]
    fn drops_orphan_close_tag() {
        assert_eq!(sanitize_event_description("a </link> b"), "a  b");
    }

    #[test]
    fn strips_link_tag_with_stray_quote() {
        assert_eq!(
            sanitize_event_description("<link=\"https://a.com\"x\">t</link>"),
            "t"
        );
    }

    #[test]
    fn unquoted_link_target_is_kept_when_safe() {
        assert_eq!(
            sanitize_event_description("<link=https://a.com>t</link>"),
            "<link=https://a.com>t</link>"
        );
    }

    #[test]
    fn internal_host_check_covers_reserved_ranges() {
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
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "fc00::1",
            "[::1]",
            "localhost",
            "router",
            "svc.internal",
            "office.intranet",
            "box.home",
            "vpn.corp",
            "gw.home.arpa",
            "app.localhost",
        ] {
            assert!(is_internal_link_host(host), "{host} must be internal");
        }
        for host in [
            "8.8.8.8",
            "172.15.0.1",
            "172.32.0.1",
            "100.63.0.1",
            "100.128.0.1",
            "decentraland.org",
            "events.decentraland.org",
            "2600::1",
        ] {
            assert!(!is_internal_link_host(host), "{host} must be public");
        }
    }
}
