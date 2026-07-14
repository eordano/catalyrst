//! Read-side sanitizer for event descriptions: delegates to
//! `catalyrst_types::sanitize`, which keeps only `<link>` tags targeting
//! http(s) URLs on public hosts (a clicked target reaches an unrestricted
//! `Application.OpenURL` on the viewer's machine).

use catalyrst_types::is_safe_link_target;

pub fn sanitize_event_description(description: &str) -> String {
    catalyrst_types::sanitize_markup_description(description)
}

pub const MAX_EVENT_URL_LEN: usize = 2048;

pub fn validate_event_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.chars().count() > MAX_EVENT_URL_LEN {
        return Err("url must be at most 2048 characters".into());
    }
    if !is_safe_link_target(trimmed) {
        return Err("url must be a safe http(s) link".into());
    }
    Ok(trimmed.to_owned())
}

pub fn sanitize_asset_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.chars().count() > MAX_EVENT_URL_LEN {
        return Err("image must be at most 2048 characters".into());
    }
    if let Some(hash) = trimmed.strip_prefix("/poster/") {
        if !hash.is_empty()
            && hash
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        {
            return Ok(trimmed.to_owned());
        }
        return Err("image must be a /poster/<hash> path or a safe http(s) URL".into());
    }
    if is_safe_link_target(trimmed) {
        return Ok(trimmed.to_owned());
    }
    Err("image must be a /poster/<hash> path or a safe http(s) URL".into())
}

pub const MAX_FEATURED_ITEM_LEN: usize = 160;

const FEATURED_ITEM_CHAINS: [&str; 4] = ["matic", "ethereum", "amoy", "sepolia"];

pub fn validate_featured_item(urn: &str) -> Result<(), String> {
    if urn.chars().count() > MAX_FEATURED_ITEM_LEN {
        return Err("featured_item must be at most 160 characters".into());
    }
    let invalid = || "featured_item must be a collections-v2 item or collection urn".to_string();
    let rest = urn.strip_prefix("urn:decentraland:").ok_or_else(invalid)?;
    let (chain, rest) = rest.split_once(':').ok_or_else(invalid)?;
    if !FEATURED_ITEM_CHAINS.contains(&chain) {
        return Err(invalid());
    }
    let rest = rest.strip_prefix("collections-v2:0x").ok_or_else(invalid)?;
    let (address, item) = match rest.split_once(':') {
        Some((address, item)) => (address, Some(item)),
        None => (rest, None),
    };
    if address.len() != 40 || !address.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    if let Some(item) = item {
        if item.is_empty() || !item.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
    }
    Ok(())
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
    fn keeps_safe_link_and_strips_unsafe_one() {
        assert_eq!(
            sanitize_event_description(
                "<link=\"https://a.com\">A</link><link=\"javascript:alert(1)\">B</link>"
            ),
            "<link=\"https://a.com\">A</link>B"
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
        let text = "See [our site](https://decentraland.org) for **details** \u{2014} 5 < 10 and 10 > 5 and I <3 events";
        assert_eq!(sanitize_event_description(text), text);
    }

    #[test]
    fn empty_description_returned_unchanged() {
        assert_eq!(sanitize_event_description(""), "");
    }

    #[test]
    fn drops_orphan_close_tag() {
        assert_eq!(sanitize_event_description("a </link> b"), "a  b");
    }

    #[test]
    fn fails_closed_when_input_does_not_stabilize_within_the_pass_cap() {
        let out = sanitize_event_description("<<<<<<b>b>b>b>b>b>");
        assert!(!out.contains('<') && !out.contains('>'), "{out}");
    }

    #[test]
    fn output_is_idempotent() {
        for input in [
            "<link=\"javascript:alert(1)\"<b>>click</link>",
            "<<b>link=\"javascript:alert(1)\">click</link>",
            "see <color=red><link=javascript:alert(1)",
            "<link=\"https://a.com\">A</link><link=\"javascript:alert(1)\">B</link>",
            "See [our site](https://decentraland.org) \u{2014} 5 < 10 and 10 > 5",
        ] {
            let once = sanitize_event_description(input);
            assert_eq!(sanitize_event_description(&once), once, "{input}");
        }
    }

    #[test]
    fn accepts_safe_event_urls_and_rejects_unsafe_ones() {
        assert_eq!(
            validate_event_url("  https://decentraland.org/events  "),
            Ok("https://decentraland.org/events".to_owned())
        );
        for url in [
            "javascript:alert(1)",
            "http://localhost:8080/",
            "http://169.254.169.254/",
            "",
            &"h".repeat(MAX_EVENT_URL_LEN + 1),
        ] {
            assert!(validate_event_url(url).is_err(), "{url:?}");
        }
    }

    #[test]
    fn accepts_poster_paths_and_safe_absolute_asset_urls() {
        for url in [
            "/poster/bafkreiabc123",
            "/poster/hash-with.dots-1",
            "https://decentraland.org/poster.png",
        ] {
            assert_eq!(sanitize_asset_url(url), Ok(url.to_owned()), "{url}");
        }
        for url in [
            "/poster/",
            "/poster/../etc/passwd",
            "/poster/a/b",
            "/other/hash",
            "javascript:alert(1)",
            "http://127.0.0.1/x.png",
            "",
        ] {
            assert!(sanitize_asset_url(url).is_err(), "{url:?}");
        }
    }

    #[test]
    fn accepts_collections_v2_item_and_collection_urns() {
        let address = "0x1234567890abcdef1234567890abcdef12345678";
        for urn in [
            format!("urn:decentraland:matic:collections-v2:{address}:1"),
            format!("urn:decentraland:matic:collections-v2:{address}"),
            format!("urn:decentraland:ethereum:collections-v2:{address}:42"),
            format!("urn:decentraland:amoy:collections-v2:{address}:0"),
            format!("urn:decentraland:sepolia:collections-v2:{address}"),
            "urn:decentraland:matic:collections-v2:0xAbCdEf1234567890ABCDEF1234567890abcdef12:7"
                .into(),
        ] {
            assert_eq!(validate_featured_item(&urn), Ok(()), "{urn}");
        }
    }

    #[test]
    fn rejects_featured_items_outside_the_collections_v2_urn_shape() {
        let address = "0x1234567890abcdef1234567890abcdef12345678";
        for urn in [
            format!("urn:decentraland:mainnet:collections-v2:{address}:1"),
            format!("urn:decentraland:matic:collections-v1:{address}:1"),
            "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef1234567:1"
                .into(),
            "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef123456789:1"
                .into(),
            "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef1234567g:1"
                .into(),
            format!("urn:decentraland:matic:collections-v2:{address}:1:2"),
            format!("urn:decentraland:matic:collections-v2:{address}:abc"),
            format!("urn:decentraland:matic:collections-v2:{address}:"),
            format!("decentraland:matic:collections-v2:{address}:1"),
            format!(" urn:decentraland:matic:collections-v2:{address}:1"),
            format!("urn:decentraland:matic:collections-v2:{address}:1\n"),
            "my favourite wearable".into(),
            String::new(),
            format!(
                "urn:decentraland:matic:collections-v2:{address}:{}",
                "1".repeat(130)
            ),
        ] {
            assert!(validate_featured_item(&urn).is_err(), "{urn:?}");
        }
    }
}
