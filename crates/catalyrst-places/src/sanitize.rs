use reqwest::Url;

pub(crate) const MAX_SANITIZE_PASSES: usize = 5;

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

fn is_js_whitespace(c: char) -> bool {
    matches!(c, '\u{2000}'..='\u{200a}')
        || matches!(
            c,
            '\t' | '\n'
                | '\u{b}'
                | '\u{c}'
                | '\r'
                | ' '
                | '\u{a0}'
                | '\u{1680}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
}

pub fn sanitize_image_url(value: Option<&str>) -> Option<String> {
    let url = Url::parse(value?).ok()?;
    match url.scheme() {
        "http" | "https" => Some(url.to_string()),
        _ => None,
    }
}

pub fn sanitize_place_description(description: Option<&str>) -> Option<String> {
    let description = description.filter(|d| !d.is_empty())?;

    let mut current = description.to_string();
    for _ in 0..MAX_SANITIZE_PASSES {
        let next = strip_pass(&current);
        if next == current {
            return Some(current);
        }
        current = next;
    }
    Some(current.replace(['<', '>'], ""))
}

fn strip_pass(text: &str) -> String {
    drop_unclosed_link_openers(&strip_markup_once(text))
}

fn strip_markup_once(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut open_link_kept: Vec<bool> = Vec::new();
    let mut copied = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(end) = markup_tag_end(bytes, i) else {
            i += 1;
            continue;
        };

        out.push_str(&text[copied..i]);
        let tag = &text[i..end];
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
        copied = end;
        i = end;
    }

    out.push_str(&text[copied..]);
    out
}

fn markup_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'/') {
        i += 1;
    }
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    bytes[i..]
        .iter()
        .position(|b| *b == b'>')
        .map(|offset| i + offset + 1)
}

fn drop_unclosed_link_openers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0usize;

    for i in 0..bytes.len() {
        if bytes[i] != b'<'
            || !starts_link_name(bytes, i + 1)
            || closes_before_next_tag(bytes, i + 1)
        {
            continue;
        }
        out.push_str(&text[copied..i]);
        copied = i + 1;
    }

    out.push_str(&text[copied..]);
    out
}

fn starts_link_name(bytes: &[u8], at: usize) -> bool {
    let start = if bytes.get(at) == Some(&b'/') {
        at + 1
    } else {
        at
    };
    let end = start + 4;
    if bytes.len() < end || !bytes[start..end].eq_ignore_ascii_case(b"link") {
        return false;
    }
    !matches!(bytes.get(end), Some(b) if b.is_ascii_alphanumeric() || *b == b'_')
}

fn closes_before_next_tag(bytes: &[u8], at: usize) -> bool {
    bytes[at..]
        .iter()
        .take_while(|b| **b != b'<')
        .any(|b| *b == b'>')
}

fn is_link_close_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix("</") else {
        return false;
    };
    let Some(rest) = strip_prefix_ci(rest, "link") else {
        return false;
    };
    let Some(rest) = rest.strip_suffix('>') else {
        return false;
    };
    rest.chars().all(is_js_whitespace)
}

fn link_open_target(tag: &str) -> Option<&str> {
    let rest = tag.strip_prefix('<')?;
    let rest = strip_prefix_ci(rest, "link")?;
    let rest = rest
        .trim_start_matches(is_js_whitespace)
        .strip_prefix('=')?
        .trim_start_matches(is_js_whitespace);
    let inner = rest.strip_suffix('>')?;

    let target = match inner.strip_prefix('"') {
        Some(quoted) => quoted
            .trim_end_matches(is_js_whitespace)
            .strip_suffix('"')?,
        None => inner,
    };
    if target.contains(['"', '<', '>']) {
        return None;
    }
    Some(target)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, tail) = s.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then_some(tail)
}

fn is_safe_link_target(target: &str) -> bool {
    let trimmed = target.trim_matches(is_js_whitespace);
    if trimmed.chars().any(is_js_whitespace) {
        return false;
    }

    let Ok(url) = Url::parse(trimmed) else {
        return false;
    };
    if url.scheme() != "http" && url.scheme() != "https" {
        return false;
    }

    url.host_str()
        .is_some_and(|host| !is_internal_link_host(&host.to_ascii_lowercase()))
}

fn is_internal_link_host(hostname: &str) -> bool {
    let unbracketed = hostname
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(hostname);
    let host = unbracketed.trim_end_matches('.');

    if let Some([a, b, _, _]) = ipv4_octets(host) {
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
            || host.starts_with("fe8")
            || host.starts_with("fe9")
            || host.starts_with("fea")
            || host.starts_with("feb")
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

fn ipv4_octets(host: &str) -> Option<[u32; 4]> {
    let mut octets = [0u32; 4];
    let mut parts = host.split('.');
    for octet in octets.iter_mut() {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *octet = part.parse().ok()?;
    }
    parts.next().is_none().then_some(octets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitize(input: &str) -> String {
        sanitize_place_description(Some(input)).unwrap_or_default()
    }

    #[test]
    fn strips_both_sides_of_a_custom_protocol_link() {
        assert_eq!(
            sanitize(r#"Join <link="decentraland://?position=0,0">click here</link>"#),
            "Join click here"
        );
    }

    #[test]
    fn strips_file_and_smb_links_without_leaving_orphan_tags() {
        assert_eq!(
            sanitize(r#"a <link="file:///etc/passwd">x</link> b <link="smb://h/s">y</link> c"#),
            "a x b y c"
        );
    }

    #[test]
    fn preserves_safe_https_and_http_links() {
        let https = r#"Visit <link="https://decentraland.org">our site</link>"#;
        assert_eq!(sanitize(https), https);
        let http = r#"<link="http://example.com">x</link>"#;
        assert_eq!(sanitize(http), http);
    }

    #[test]
    fn keeps_the_safe_link_and_strips_the_unsafe_one() {
        assert_eq!(
            sanitize(r#"<link="https://a.com">A</link><link="javascript:alert(1)">B</link>"#),
            r#"<link="https://a.com">A</link>B"#
        );
    }

    #[test]
    fn strips_a_tag_carrying_extra_content_after_the_target() {
        assert_eq!(sanitize("<link=https://a.com onclick=x>t</link>"), "t");
    }

    #[test]
    fn fails_closed_on_a_malformed_opener_embedding_a_nested_tag() {
        assert!(!contains_link_opener(&sanitize(
            r#"<link="javascript:alert(1)"<b>>click</link>"#
        )));
    }

    #[test]
    fn fails_closed_when_a_stripped_tag_reassembles_a_new_opener() {
        assert!(!contains_link_opener(&sanitize(
            r#"<<b>link="javascript:alert(1)">click</link>"#
        )));
    }

    #[test]
    fn drops_an_unclosed_link_opener() {
        assert!(!contains_link_opener(&sanitize(
            "see <color=red><link=javascript:alert(1)"
        )));
    }

    #[test]
    fn strips_links_to_the_cloud_metadata_ip() {
        assert_eq!(
            sanitize(r#"<link="http://169.254.169.254/latest/meta-data/">x</link>"#),
            "x"
        );
    }

    #[test]
    fn strips_links_to_an_obfuscated_loopback_ip() {
        assert_eq!(sanitize(r#"<link="http://2130706433/">x</link>"#), "x");
    }

    #[test]
    fn strips_links_to_private_and_localhost_hosts() {
        assert_eq!(
            sanitize(
                r#"a <link="http://192.168.1.1/">x</link> b <link="http://localhost:8080/">y</link> c"#
            ),
            "a x b y c"
        );
    }

    #[test]
    fn keeps_links_to_a_public_ip() {
        let input = r#"<link="https://8.8.8.8/">x</link>"#;
        assert_eq!(sanitize(input), input);
    }

    #[test]
    fn strips_links_to_single_label_and_reserved_suffix_hosts() {
        assert_eq!(
            sanitize(
                r#"a <link="http://router/">x</link> b <link="http://printer.lan/">y</link> c <link="http://nas.local/">z</link> d"#
            ),
            "a x b y c z d"
        );
    }

    #[test]
    fn strips_internal_hosts_written_in_fully_qualified_form() {
        assert_eq!(
            sanitize(
                r#"a <link="http://localhost./">x</link> b <link="http://nas.local./">y</link> c <link="http://router./">z</link> d"#
            ),
            "a x b y c z d"
        );
    }

    #[test]
    fn keeps_a_public_host_written_in_fully_qualified_form() {
        let input = r#"<link="https://example.com./">ok</link>"#;
        assert_eq!(sanitize(input), input);
    }

    #[test]
    fn strips_link_tags_with_mismatched_quotes() {
        assert!(!contains_link_opener(&sanitize(
            r#"<link="https://example.com>click"#
        )));
        assert!(!contains_link_opener(&sanitize(
            r#"<link=https://example.com">click"#
        )));
    }

    #[test]
    fn leaves_non_tag_angle_brackets_and_ampersands_untouched() {
        let input = "Open 5 < 10 hours & counting";
        assert_eq!(sanitize(input), input);
    }

    #[test]
    fn normalizes_missing_and_empty_descriptions_to_none() {
        assert_eq!(sanitize_place_description(None), None);
        assert_eq!(sanitize_place_description(Some("")), None);
    }

    #[test]
    fn strips_links_to_ipv6_loopback_and_link_local_hosts() {
        assert_eq!(sanitize(r#"<link="http://[::1]/">x</link>"#), "x");
        assert_eq!(sanitize(r#"<link="http://[fe80::1]/">x</link>"#), "x");
        assert_eq!(sanitize(r#"<link="http://[fd00::1]/">x</link>"#), "x");
    }

    #[test]
    fn is_idempotent_and_never_leaves_a_live_unsafe_link() {
        for input in [
            r#"<link="javascript:alert(1)"<b>>click</link>"#,
            r#"<<b>link="javascript:alert(1)">click</link>"#,
            "see <color=red><link=javascript:alert(1)",
            r#"<<<b>b>link="javascript:alert(1)">click"#,
            r#"<LINK="JavaScript:alert(1)">x</LINK>"#,
            r#"Visit <link="https://decentraland.org">our site</link>"#,
        ] {
            let once = sanitize(input);
            assert_eq!(sanitize(&once), once, "sanitizing {input:?} is not stable");
            assert!(
                !contains_link_opener(&once) || once.contains(r#"<link="https://"#),
                "unsafe link survived in {once:?}"
            );
        }
    }

    #[test]
    fn nested_openers_that_converge_leave_nothing_and_deeper_ones_fail_closed() {
        for input in [
            "<<<<b>b>b>zeppelinword>",
            "<<<<<b>b>b>b>zeppelinword>",
            r#"<<<<b>b>b>link="javascript:alert(1)">"#,
        ] {
            assert_eq!(sanitize(input), "", "{input:?} must not survive a pass");
        }
        assert_eq!(sanitize("<<<<<<b>b>b>b>b>kryptonword>"), "kryptonword");
        assert_eq!(sanitize("<<<<<<<b>b>b>b>b>b>kryptonword>"), "bkryptonword");
    }

    #[test]
    fn strips_link_tags_padded_with_a_next_line_control() {
        for input in [
            "<link=\u{85}\"https://a.com\">x</link>",
            "<link\u{85}=\"https://a.com\">x</link>",
            "<link=\"https://a.com\"\u{85}>x</link>",
            "<link=\"\u{85}https://a.com\">x</link>",
            "<link=\"https://a.com\u{85}\">x</link>",
            "<link=\u{85}https://a.com>x</link>",
        ] {
            assert_eq!(sanitize(input), "x", "{input:?} must not survive");
        }
        assert_eq!(
            sanitize("<link=\"https://a.com\">x</link\u{85}>"),
            "<link=\"https://a.com\">x"
        );
    }

    #[test]
    fn keeps_link_tags_padded_with_a_byte_order_mark() {
        for input in [
            "<link=\u{feff}\"https://a.com\">x</link>",
            "<link\u{feff}=\"https://a.com\">x</link>",
            "<link=\"https://a.com\"\u{feff}>x</link>",
            "<link=\"\u{feff}https://a.com\">x</link>",
            "<link=\"https://a.com\u{feff}\">x</link>",
            "<link=\"https://a.com\">x</link\u{feff}>",
            "<link=\u{feff}https://a.com>x</link>",
        ] {
            assert_eq!(sanitize(input), input, "{input:?} must be preserved");
        }
        assert_eq!(
            sanitize("<link=\u{feff}\"javascript:alert(1)\">x</link>"),
            "x"
        );
    }

    #[test]
    fn treats_no_break_space_as_padding_and_zero_width_space_as_junk() {
        let padded = "<link=\u{a0}\"https://a.com\">x</link>";
        assert_eq!(sanitize(padded), padded);
        let padded_close = "<link=\"https://a.com\">x</link\u{a0}>";
        assert_eq!(sanitize(padded_close), padded_close);

        assert_eq!(sanitize("<link=\u{200b}\"https://a.com\">x</link>"), "x");
        assert_eq!(
            sanitize("<link=\"https://a.com\">x</link\u{200b}>"),
            "<link=\"https://a.com\">x"
        );
    }

    #[test]
    fn sanitize_image_url_round_trips_content_server_thumbnails_unchanged() {
        for input in [
            "https://peer.decentraland.org/content/contents/bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y",
            "https://api.decentraland.org/v1/map.png?center=-9,-9&selected=-9,-9&width=1024&height=1024&size=10",
            "http://localhost:5141/world/contents/bafkreiabc",
        ] {
            assert_eq!(
                sanitize_image_url(Some(input)).as_deref(),
                Some(input),
                "{input:?} must not be perturbed"
            );
        }
    }

    #[test]
    fn sanitize_image_url_never_returns_attribute_breakout_characters() {
        for input in [
            "https://cdn.example/contents/x\"><script>alert(1)</script>",
            "https://a\"><meta http-equiv=\"refresh\" content=\"0\">",
            "https://cdn.example/a<b>c",
        ] {
            let out = sanitize_image_url(Some(input)).unwrap_or_default();
            assert!(
                !out.contains(['"', '<', '>']),
                "{input:?} leaked breakout characters as {out:?}"
            );
        }
    }

    #[test]
    fn sanitize_image_url_drops_non_http_and_unparseable_values() {
        assert_eq!(sanitize_image_url(None), None);
        assert_eq!(sanitize_image_url(Some("")), None);
        assert_eq!(sanitize_image_url(Some("javascript:alert(1)")), None);
        assert_eq!(sanitize_image_url(Some("/images/places/banner.jpg")), None);
    }

    fn contains_link_opener(s: &str) -> bool {
        s.to_ascii_lowercase().contains("<link")
    }
}
