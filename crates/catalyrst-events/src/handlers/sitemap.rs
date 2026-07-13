use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::http::response::ApiError;
use crate::ports::events::SITEMAP_ITEMS_PER_PAGE;
use crate::AppState;

const DEFAULT_EVENTS_BASE_URL: &str = "https://events.decentraland.org";

fn base_url() -> String {
    std::env::var("EVENTS_BASE_URL").unwrap_or_else(|_| DEFAULT_EVENTS_BASE_URL.to_string())
}

fn site_url(base: &str, pathname: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if pathname.is_empty() {
        return format!("{}/", trimmed);
    }
    format!("{}{}", trimmed, pathname)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_response(body: String) -> Response {
    (StatusCode::OK, [(CONTENT_TYPE, "application/xml")], body).into_response()
}

pub async fn sitemap_index(State(state): State<AppState>) -> Result<Response, ApiError> {
    let base = base_url();
    let count = state.events.count_approved().await?;
    let pages = if count <= 0 {
        0
    } else {
        ((count + SITEMAP_ITEMS_PER_PAGE - 1) / SITEMAP_ITEMS_PER_PAGE) as usize
    };

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    out.push_str("<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">");
    out.push_str(&format!(
        "<sitemap><loc>{}</loc></sitemap>",
        xml_escape(&site_url(&base, "/sitemap.static.xml"))
    ));
    out.push_str(&format!(
        "<sitemap><loc>{}</loc></sitemap>",
        xml_escape(&site_url(&base, "/sitemap.schedules.xml"))
    ));
    for i in 0..pages {
        let loc = format!("{}?page={}", site_url(&base, "/sitemap.events.xml"), i);
        out.push_str(&format!(
            "<sitemap><loc>{}</loc></sitemap>",
            xml_escape(&loc)
        ));
    }
    out.push_str("</sitemapindex>");
    Ok(xml_response(out))
}

pub async fn sitemap_static() -> Result<Response, ApiError> {
    let base = base_url();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">");
    out.push_str(&format!(
        "<url><loc>{}</loc></url>",
        xml_escape(&site_url(&base, ""))
    ));
    out.push_str(&format!(
        "<url><loc>{}</loc></url>",
        xml_escape(&site_url(&base, "/submit/"))
    ));
    out.push_str("</urlset>");
    Ok(xml_response(out))
}

pub async fn sitemap_events(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let base = base_url();
    let raw_page = params.get("page");
    let page = match raw_page.and_then(|s| s.parse::<i64>().ok()) {
        Some(p) if p >= 0 && raw_page.map(String::as_str) == Some(p.to_string().as_str()) => {
            Some(p)
        }
        _ => None,
    };

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">");
    if let Some(page) = page {
        let ids = state.events.sitemap_event_ids(page).await?;
        for id in ids {
            let loc = format!("{}?id={}", site_url(&base, "/event/"), id);
            out.push_str(&format!("<url><loc>{}</loc></url>", xml_escape(&loc)));
        }
    }
    out.push_str("</urlset>");
    Ok(xml_response(out))
}

pub async fn sitemap_schedules(State(state): State<AppState>) -> Result<Response, ApiError> {
    let base = base_url();
    let ids = state.schedules.sitemap_schedule_ids().await?;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">");
    for id in ids {
        let loc = format!("{}?id={}", site_url(&base, "/schedule/"), id);
        out.push_str(&format!("<url><loc>{}</loc></url>", xml_escape(&loc)));
    }
    out.push_str("</urlset>");
    Ok(xml_response(out))
}
