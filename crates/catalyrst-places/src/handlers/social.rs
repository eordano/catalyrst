use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;

use crate::ports::places::PlaceRow;
use crate::sanitize::sanitize_image_url;
use crate::AppState;

const SITE_URL: &str = "https://places.decentraland.org";

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render(title: &str, description: &str, image: &str, url: &str) -> String {
    let t = escape_html(title);
    let d = description.trim();
    let d = match d.find("\n\n") {
        Some(i) if i > 0 => d[..i].trim(),
        _ => d,
    };
    let d = escape_html(d).replace('\n', " ");
    let image = escape_html(image);
    let url = escape_html(url);
    format!(
        "<!DOCTYPE html><html><head>\
<title data-react-helmet=\"true\">{t}</title>\
<meta data-react-helmet=\"true\" name=\"description\" content=\"{d}\" />\
<meta data-react-helmet=\"true\" property=\"og:title\" content=\"{t}\" />\
<meta data-react-helmet=\"true\" property=\"og:description\" content=\"{d}\" />\
<meta data-react-helmet=\"true\" property=\"og:image\" content=\"{image}\" />\
<meta data-react-helmet=\"true\" property=\"og:url\" content=\"{url}\" />\
<meta data-react-helmet=\"true\" property=\"og:type\" content=\"website\" />\
<meta data-react-helmet=\"true\" name=\"twitter:title\" content=\"{t}\" />\
<meta data-react-helmet=\"true\" name=\"twitter:description\" content=\"{d}\" />\
<meta data-react-helmet=\"true\" name=\"twitter:image\" content=\"{image}\" />\
<meta data-react-helmet=\"true\" name=\"twitter:url\" content=\"{url}\" />\
<meta data-react-helmet=\"true\" name=\"twitter:card\" content=\"summary_large_image\" />\
<meta data-react-helmet=\"true\" name=\"twitter:site\" content=\"@decentraland\" />\
<link data-react-helmet=\"true\" rel=\"canonical\" href=\"{url}\" />\
</head><body></body></html>"
    )
}

fn social_url(path: &str, key: &str, value: &str) -> String {
    let mut u = reqwest::Url::parse(SITE_URL).expect("SITE_URL is a valid URL");
    u.set_path(path);
    u.query_pairs_mut().append_pair(key, value);
    u.to_string()
}

fn place_url(place: &PlaceRow) -> String {
    social_url("/place/", "position", &place.base_position)
}

fn with_canonical(url: &str, html: String) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&format!("<{}>; rel=canonical", url)) {
        headers.insert(header::LINK, v);
    }
    (headers, Html(html)).into_response()
}

#[utoipa::path(
    get,
    path = "/place",
    tag = "social",
    params(("id" = Option<String>, Query), ("position" = Option<String>, Query)),
    responses(
        (status = 200, content_type = "text/html", body = String)
    )
)]
pub async fn inject_place_metadata(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let id = q.get("id").cloned().unwrap_or_default();
    let position = q.get("position").cloned().unwrap_or_default();

    let place = if !id.is_empty() {
        state.places.find_by_id(&id).await.ok().flatten()
    } else if !position.is_empty() {
        state.places.find_by_id(&position).await.ok().flatten()
    } else {
        None
    };

    if let Some(place) = place {
        let url = place_url(&place);
        let title = format!(
            "{} | Decentraland Place",
            place.title.clone().unwrap_or_default()
        );
        let image = sanitize_image_url(place.image.as_deref()).unwrap_or_default();
        let html = render(
            &title,
            place.description.as_deref().unwrap_or("").trim(),
            &image,
            &url,
        );
        return with_canonical(&url, html);
    }

    let url = format!("{}/place/", SITE_URL);
    Html(render("Decentraland Place", "", "", &url)).into_response()
}

#[utoipa::path(
    get,
    path = "/world",
    tag = "social",
    params(("id" = Option<String>, Query), ("name" = Option<String>, Query)),
    responses(
        (status = 200, content_type = "text/html", body = String)
    )
)]
pub async fn inject_world_metadata(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let world_id = q
        .get("id")
        .or_else(|| q.get("name"))
        .cloned()
        .unwrap_or_default()
        .to_lowercase();

    let world = if !world_id.is_empty() {
        state
            .places
            .find_world_by_id(&world_id)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    if let Some(world) = world {
        let name = world.world_name.clone().unwrap_or_default();
        let url = social_url("/world/", "name", &name);
        let title = format!(
            "{} | Decentraland Place",
            world.title.clone().unwrap_or_default()
        );
        let image = sanitize_image_url(world.image.as_deref()).unwrap_or_default();
        let html = render(
            &title,
            world.description.as_deref().unwrap_or("").trim(),
            &image,
            &url,
        );
        return with_canonical(&url, html);
    }

    let url = format!("{}/world/", SITE_URL);
    Html(render("Decentraland Place", "", "", &url)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BREAKOUT: &str = "https://a\"><script>alert(1)</script><meta name=\"x";

    #[test]
    fn sanitize_image_url_keeps_valid_http_and_https_urls_normalized() {
        assert_eq!(
            sanitize_image_url(Some("https://cdn.decentraland.org/thumb.png")),
            Some("https://cdn.decentraland.org/thumb.png".to_string())
        );
        assert_eq!(
            sanitize_image_url(Some("http://cdn.decentraland.org/thumb.png")),
            Some("http://cdn.decentraland.org/thumb.png".to_string())
        );
        assert_eq!(
            sanitize_image_url(Some("https://cdn.decentraland.org/t.png?w=100&h=50")),
            Some("https://cdn.decentraland.org/t.png?w=100&h=50".to_string())
        );
    }

    #[test]
    fn sanitize_image_url_rejects_html_breakout_values() {
        assert_eq!(sanitize_image_url(Some(BREAKOUT)), None);
    }

    #[test]
    fn sanitize_image_url_rejects_non_http_schemes() {
        assert_eq!(
            sanitize_image_url(Some("javascript:alert(document.domain)")),
            None
        );
        assert_eq!(
            sanitize_image_url(Some("data:text/html,<script>alert(1)</script>")),
            None
        );
        assert_eq!(sanitize_image_url(Some("file:///etc/passwd")), None);
    }

    #[test]
    fn sanitize_image_url_rejects_missing_or_unparseable_values() {
        assert_eq!(sanitize_image_url(None), None);
        assert_eq!(sanitize_image_url(Some("")), None);
        assert_eq!(sanitize_image_url(Some("not a url")), None);
        assert_eq!(sanitize_image_url(Some("/relative/path.png")), None);
    }

    #[test]
    fn sanitize_image_url_percent_encodes_breakout_chars_outside_the_host() {
        assert_eq!(
            sanitize_image_url(Some("https://example.com/a\"b<c>d")),
            Some("https://example.com/a%22b%3Cc%3Ed".to_string())
        );
    }

    #[test]
    fn render_never_emits_a_sanitized_breakout_image_as_live_markup() {
        let image = sanitize_image_url(Some(BREAKOUT)).unwrap_or_default();
        let html = render(
            "Genesis Plaza | Decentraland Place",
            "description",
            &image,
            "https://places.decentraland.org/place/?position=0,0",
        );
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains(BREAKOUT));
        assert!(html.contains("property=\"og:image\" content=\"\""));
    }

    #[test]
    fn render_keeps_a_valid_image_url_as_the_og_image_content() {
        let image =
            sanitize_image_url(Some("https://cdn.decentraland.org/thumb.png")).unwrap_or_default();
        let html = render(
            "Genesis Plaza | Decentraland Place",
            "description",
            &image,
            "https://places.decentraland.org/place/?position=0,0",
        );
        assert!(html.contains("content=\"https://cdn.decentraland.org/thumb.png\""));
    }

    #[test]
    fn render_emits_twitter_variants_alongside_og_for_all_metadata() {
        let html = render(
            "Genesis Plaza | Decentraland Place",
            "A plaza",
            "https://cdn.decentraland.org/thumb.png",
            "https://places.decentraland.org/place/?position=0%2C0",
        );
        for tag in [
            "property=\"og:title\" content=\"Genesis Plaza | Decentraland Place\"",
            "name=\"twitter:title\" content=\"Genesis Plaza | Decentraland Place\"",
            "property=\"og:description\" content=\"A plaza\"",
            "name=\"twitter:description\" content=\"A plaza\"",
            "property=\"og:image\" content=\"https://cdn.decentraland.org/thumb.png\"",
            "name=\"twitter:image\" content=\"https://cdn.decentraland.org/thumb.png\"",
            "property=\"og:url\" content=\"https://places.decentraland.org/place/?position=0%2C0\"",
            "name=\"twitter:url\" content=\"https://places.decentraland.org/place/?position=0%2C0\"",
        ] {
            assert!(html.contains(tag), "missing tag: {tag}");
        }
    }

    #[test]
    fn render_truncates_description_at_first_paragraph_and_collapses_newlines() {
        let html = render(
            "t",
            "  line one\nline two\n\nsecond paragraph  ",
            "",
            "https://places.decentraland.org/place/",
        );
        assert!(html.contains("property=\"og:description\" content=\"line one line two\""));
        assert!(html.contains("name=\"twitter:description\" content=\"line one line two\""));
        assert!(html.contains("name=\"description\" content=\"line one line two\""));
        assert!(!html.contains("second paragraph"));
    }

    #[test]
    fn social_url_form_encodes_query_values_like_upstream() {
        assert_eq!(
            social_url("/place/", "position", "0,0"),
            "https://places.decentraland.org/place/?position=0%2C0"
        );
        assert_eq!(
            social_url("/world/", "name", "my world\".dcl.eth"),
            "https://places.decentraland.org/world/?name=my+world%22.dcl.eth"
        );
    }

    #[test]
    fn render_escapes_image_and_url_attribute_values() {
        let html = render(
            "t",
            "d",
            "https://cdn.decentraland.org/t.png?w=100&h=50",
            "https://x\"><script>alert(1)</script>",
        );
        assert!(html.contains("content=\"https://cdn.decentraland.org/t.png?w=100&amp;h=50\""));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(
            html.contains("content=\"https://x&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;\"")
        );
    }
}
