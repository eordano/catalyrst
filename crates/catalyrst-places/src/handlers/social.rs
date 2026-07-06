use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;

use crate::ports::places::PlaceRow;
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
    let d = escape_html(description);
    format!(
        "<!DOCTYPE html><html><head>\
<title data-react-helmet=\"true\">{t}</title>\
<meta data-react-helmet=\"true\" name=\"description\" content=\"{d}\" />\
<meta data-react-helmet=\"true\" property=\"og:title\" content=\"{t}\" />\
<meta data-react-helmet=\"true\" property=\"og:description\" content=\"{d}\" />\
<meta data-react-helmet=\"true\" property=\"og:image\" content=\"{image}\" />\
<meta data-react-helmet=\"true\" property=\"og:url\" content=\"{url}\" />\
<meta data-react-helmet=\"true\" property=\"og:type\" content=\"website\" />\
<meta data-react-helmet=\"true\" name=\"twitter:card\" content=\"summary_large_image\" />\
<meta data-react-helmet=\"true\" name=\"twitter:site\" content=\"@decentraland\" />\
<link data-react-helmet=\"true\" rel=\"canonical\" href=\"{url}\" />\
</head><body></body></html>"
    )
}

fn place_url(place: &PlaceRow) -> String {
    format!("{}/place/?position={}", SITE_URL, place.base_position)
}

fn with_canonical(url: &str, html: String) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&format!("<{}>; rel=canonical", url)) {
        headers.insert(header::LINK, v);
    }
    (headers, Html(html)).into_response()
}

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
        let html = render(
            &title,
            place.description.as_deref().unwrap_or("").trim(),
            place.image.as_deref().unwrap_or(""),
            &url,
        );
        return with_canonical(&url, html);
    }

    let url = format!("{}/place/", SITE_URL);
    Html(render("Decentraland Place", "", "", &url)).into_response()
}

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
        let url = format!("{}/world/?name={}", SITE_URL, name);
        let title = format!(
            "{} | Decentraland Place",
            world.title.clone().unwrap_or_default()
        );
        let html = render(
            &title,
            world.description.as_deref().unwrap_or("").trim(),
            world.image.as_deref().unwrap_or(""),
            &url,
        );
        return with_canonical(&url, html);
    }

    let url = format!("{}/world/", SITE_URL);
    Html(render("Decentraland Place", "", "", &url)).into_response()
}
