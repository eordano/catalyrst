use axum::extract::{Form, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::AppState;

pub(crate) const COOKIE_NAME: &str = "telemetry_admin";
const COOKIE_MAX_AGE_SECS: u32 = 30 * 24 * 3600;

fn base_path() -> String {
    std::env::var("TELEMETRY_BASE_PATH").unwrap_or_default()
}

fn cookie(value: &str, max_age: u32) -> String {
    let base = base_path();
    let path = if base.is_empty() { "/" } else { &base };
    format!(
        "{COOKIE_NAME}={value}; Path={path}; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    )
}

#[derive(Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    pub err: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    Query(q): Query<LoginQuery>,
    headers: HeaderMap,
) -> Response {
    if super::admin::authorize_read(&state, &headers).is_ok() {
        return Redirect::to(&format!("{}/", base_path())).into_response();
    }
    let notice = if q.err.is_some() {
        r#"<p class="err">That token was not accepted.</p>"#
    } else {
        ""
    };
    Html(LOGIN_TEMPLATE.replace("<!--NOTICE-->", notice)).into_response()
}

#[derive(Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    pub token: String,
}

pub async fn submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let base = base_path();
    let token = form.token.trim();
    if token.is_empty() {
        return (
            StatusCode::SEE_OTHER,
            [
                (header::SET_COOKIE, cookie("", 0)),
                (header::LOCATION, format!("{base}/login")),
            ],
        )
            .into_response();
    }
    if super::admin::token_matches(&state, token) {
        (
            StatusCode::SEE_OTHER,
            [
                (header::SET_COOKIE, cookie(token, COOKIE_MAX_AGE_SECS)),
                (header::LOCATION, format!("{base}/")),
            ],
        )
            .into_response()
    } else {
        Redirect::to(&format!("{base}/login?err=1")).into_response()
    }
}

const LOGIN_TEMPLATE: &str = r#"<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Telemetry sign-in</title>
<style>
  body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;
    background:#101014;color:#e8e8ee;font:15px/1.5 system-ui,sans-serif}
  form{background:#17171d;border:1px solid #2a2a33;border-radius:10px;padding:28px 32px;
    display:flex;flex-direction:column;gap:12px;min-width:320px}
  h1{font-size:1.05rem;margin:0}
  p{margin:0;opacity:.7;font-size:.85rem}
  p.err{color:#ff7a7a;opacity:1}
  input{background:#0d0d11;border:1px solid #2a2a33;border-radius:6px;color:#e8e8ee;
    padding:9px 11px;font-size:.95rem}
  input:focus{outline:none;border-color:#6a5acd}
  button{background:#6a5acd;border:0;border-radius:6px;color:#fff;padding:9px 0;
    font-size:.95rem;font-weight:600;cursor:pointer}
  button:hover{filter:brightness(1.1)}
</style>
<form method="post" action="login">
  <h1>Telemetry admin</h1>
  <p>Paste the admin token once &#x2014; a browser session cookie keeps you signed in for 30 days.</p>
  <!--NOTICE-->
  <input type="password" name="token" placeholder="admin token" autofocus autocomplete="current-password">
  <button type="submit">Sign in</button>
</form>
"#;
