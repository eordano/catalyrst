use catalyrst_types::sanitize::{is_internal_link_host, sanitize_markup_description};
use reqwest::Url;

pub(crate) const MAX_SANITIZE_PASSES: usize = 5;

/// The deployment's own content origin, exempt from the internal-host filter.
///
/// The filter exists to stop scene-supplied metadata from pointing a viewer at *someone
/// else's* internal network; a self-hosted realm serving its thumbnails from
/// `http://localhost:5141` is not that, and blanking those is what turns a bundled
/// deployment's place list into empty images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentOrigin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

impl ContentOrigin {
    pub fn parse(base_url: &str) -> Option<Self> {
        let url = Url::parse(base_url.trim()).ok()?;
        match url.scheme() {
            "http" | "https" => {}
            _ => return None,
        }
        Some(Self {
            scheme: url.scheme().to_string(),
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default(),
        })
    }

    fn allows(&self, url: &Url) -> bool {
        let host = url.host_str().map(|h| h.to_ascii_lowercase());
        url.scheme() == self.scheme
            && host.as_deref() == Some(self.host.as_str())
            && url.port_or_known_default() == self.port
    }
}

pub fn sanitize_image_url(
    value: Option<&str>,
    content_origin: Option<&ContentOrigin>,
) -> Option<String> {
    let url = Url::parse(value?).ok()?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return None,
    }
    let host = url.host_str()?;
    if content_origin.is_some_and(|origin| origin.allows(&url)) {
        return Some(url.to_string());
    }
    if is_internal_link_host(&host.to_ascii_lowercase()) {
        return None;
    }
    Some(url.to_string())
}

pub fn sanitize_place_description(description: Option<&str>) -> Option<String> {
    description
        .filter(|d| !d.is_empty())
        .map(sanitize_markup_description)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(value: Option<&str>) -> Option<String> {
        sanitize_image_url(value, None)
    }

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
        ] {
            assert_eq!(
                img(Some(input)).as_deref(),
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
            let out = img(Some(input)).unwrap_or_default();
            assert!(
                !out.contains(['"', '<', '>']),
                "{input:?} leaked breakout characters as {out:?}"
            );
        }
    }

    #[test]
    fn sanitize_image_url_drops_non_http_and_unparseable_values() {
        assert_eq!(img(None), None);
        assert_eq!(img(Some("")), None);
        assert_eq!(img(Some("javascript:alert(1)")), None);
        assert_eq!(img(Some("file:///etc/passwd")), None);
        assert_eq!(img(Some("/images/places/banner.jpg")), None);
    }

    #[test]
    fn sanitize_image_url_drops_the_cloud_metadata_ip() {
        assert_eq!(img(Some("http://169.254.169.254/latest/meta-data/")), None);
    }

    #[test]
    fn sanitize_image_url_drops_a_private_network_host() {
        assert_eq!(img(Some("http://10.0.0.1/x")), None);
    }

    #[test]
    fn sanitize_image_url_drops_an_obfuscated_decimal_loopback_ip() {
        assert_eq!(img(Some("http://2130706433/")), None);
    }

    #[test]
    fn sanitize_image_url_drops_single_label_and_reserved_suffix_hosts() {
        for input in [
            "http://localhost:5141/world/contents/bafkreiabc",
            "http://router/",
            "http://printer.lan/",
            "http://nas.local/",
            "http://svc.internal/x",
        ] {
            assert_eq!(img(Some(input)), None, "{input:?}");
        }
    }

    #[test]
    fn sanitize_image_url_keeps_a_public_ip_host() {
        let input = "https://8.8.8.8/thumb.png";
        assert_eq!(img(Some(input)).as_deref(), Some(input));
    }

    #[test]
    fn sanitize_image_url_keeps_the_configured_content_origin() {
        let origin = ContentOrigin::parse("http://localhost:5141").unwrap();
        let input = "http://localhost:5141/world/contents/bafkreiabc";
        assert_eq!(
            sanitize_image_url(Some(input), Some(&origin)).as_deref(),
            Some(input),
            "a self-hosted realm's own thumbnails must survive"
        );
    }

    #[test]
    fn a_configured_content_origin_exempts_nothing_else() {
        let origin = ContentOrigin::parse("http://localhost:5141/").unwrap();
        for input in [
            "http://localhost:5140/x",
            "https://localhost:5141/x",
            "http://router/",
            "http://nas.local/",
            "http://10.0.0.1/x",
        ] {
            assert_eq!(
                sanitize_image_url(Some(input), Some(&origin)),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn content_origin_normalizes_the_default_port_and_rejects_non_http_bases() {
        assert_eq!(
            ContentOrigin::parse("https://peer.example.com/content"),
            ContentOrigin::parse("https://PEER.example.com:443/other")
        );
        assert!(ContentOrigin::parse("").is_none());
        assert!(ContentOrigin::parse("ftp://host/").is_none());
        assert!(ContentOrigin::parse("/relative/path").is_none());
    }

    fn contains_link_opener(s: &str) -> bool {
        s.to_ascii_lowercase().contains("<link")
    }
}
