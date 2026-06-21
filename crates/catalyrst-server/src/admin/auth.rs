//! Admin sign-in: a SIWE-style wallet handshake plus the `AdminSession` request
//! extractor that gates every mutation endpoint.
//!
//! Flow:
//!  1. `GET  /admin/auth/nonce?address=0x…` → the server returns the exact
//!     message the wallet must sign. The message is bound to the **host** the
//!     console is served on, the **address** signing in, and an **expiry**; its
//!     `Nonce:` is a stateless HMAC over `(host, address, exp)` (no server-side
//!     nonce store — restart/instance-safe and not replayable to another host or
//!     a different address).
//!  2. browser `personal_sign`s that message.
//!  3. `POST /admin/auth/verify` with `{message, signature}` → the server checks
//!     the message's host matches this request's host, the expiry is in the
//!     future, the nonce HMAC is valid, recovers the signer via
//!     `catalyrst_crypto::recover::recover_address` (EIP-191 prefix applied
//!     internally, matching `personal_sign`), requires the recovered address to
//!     equal the message's address AND be allowlisted, then sets the HMAC session
//!     cookie.
//!  4. `POST /admin/auth/logout` clears the cookie.
//!  5. `GET  /admin/auth/me` reports the current auth state for the UI.
//!
//! The `AdminSession` extractor is the single gate: a handler that takes it as an
//! argument cannot run without a valid, unexpired, allowlisted session cookie,
//! and (defense-in-depth) rejects cross-origin requests whose `Origin`/`Referer`
//! host doesn't match the request host.

use std::sync::Arc;

use axum::extract::{FromRequestParts, Json, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::admin::session;
use crate::state::AppState;

/// How long a sign-in challenge stays valid. The signed message carries this as
/// an absolute `Expiration Time`; replay is bounded to this window (and to one
/// host + one address) — the session cookie is the real long-lived credential.
const NONCE_TTL_SECS: i64 = 300; // 5 min

/// Authenticated admin, produced only by a valid session cookie on a same-origin
/// request.
pub struct AdminSession {
    pub address: String,
}

impl<S> FromRequestParts<S> for AdminSession
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let denied = (StatusCode::FORBIDDEN, "admin auth required");
        // Defense-in-depth CSRF: a present Origin/Referer must match our host.
        // (Cookies are already SameSite=Strict + HttpOnly with no CORS
        // credentials, so this is belt-and-suspenders for the high-impact
        // mutation surface.)
        if !origin_ok(&parts.headers) {
            return Err((StatusCode::FORBIDDEN, "cross-origin request rejected"));
        }
        let Some(val) = cookie_value(&parts.headers, session::COOKIE_NAME) else {
            return Err(denied);
        };
        match session::verify(&val) {
            Some(address) => Ok(AdminSession { address }),
            None => Err(denied),
        }
    }
}

/// Extract a single cookie's value from the `Cookie` header.
pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// The host the console is served on (prefer the proxy-forwarded host), lowercased.
fn request_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

/// True iff the forwarded request arrived over HTTPS (used only to build the
/// `URI:` line in the message; the cookie defaults to `Secure` regardless).
fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

/// The session cookie is `Secure` by default; an operator running on a plain-HTTP
/// private network (no TLS terminator) can opt out with `ADMIN_COOKIE_INSECURE=1`.
/// (Browsers treat localhost as a secure context, so loopback dev works as-is.)
fn cookie_secure() -> bool {
    !matches!(
        std::env::var("ADMIN_COOKIE_INSECURE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// Host component of an `Origin`/`Referer` URL (`scheme://host[:port]/…`).
fn url_host(u: &str) -> Option<String> {
    let after = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let host = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// CSRF guard: if an `Origin` (else `Referer`) header is present, its host must
/// equal the request host. Absent ⇒ allowed (same-origin navigations may omit it
/// and `SameSite=Strict` already protects the cookie).
fn origin_ok(headers: &HeaderMap) -> bool {
    let stated = headers
        .get(header::ORIGIN)
        .or_else(|| headers.get(header::REFERER))
        .and_then(|v| v.to_str().ok())
        .and_then(url_host);
    match (stated, request_host(headers)) {
        (Some(o), Some(h)) => o == h,
        (Some(_), None) => false, // header present but we can't determine our host
        (None, _) => true,
    }
}

fn valid_eth_address(s: &str) -> bool {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    matches!(s, Some(rest) if rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn chain_id(network: &str) -> u64 {
    match network {
        "mainnet" => 1,
        "sepolia" => 11_155_111,
        "goerli" => 5,
        _ => 1,
    }
}

fn rfc3339(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

/// Build the human-readable, host/address/expiry-bound sign-in message. The
/// `Nonce:` is `mac(host|address|exp)` so the message is self-verifying.
fn build_message(
    host: &str,
    scheme: &str,
    address: &str,
    chain: u64,
    nonce: &str,
    issued: i64,
    exp: i64,
) -> String {
    format!(
        "{host} wants you to sign in with your Ethereum account:\n{address}\n\n\
         Sign in to the catalyrst admin console.\n\n\
         URI: {scheme}://{host}/admin\nVersion: 1\nChain ID: {chain}\n\
         Nonce: {nonce}\nIssued At: {issued_at}\nExpiration Time: {exp_at}",
        issued_at = rfc3339(issued),
        exp_at = rfc3339(exp),
    )
}

#[derive(Deserialize)]
pub struct NonceQuery {
    pub address: String,
}

/// `GET /admin/auth/nonce?address=0x…` — return the exact message to sign. The
/// nonce binds the message to this host + address + expiry; no server state.
pub async fn nonce(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<NonceQuery>,
) -> Response {
    if !session::admin_enabled() {
        return (StatusCode::FORBIDDEN, "admin disabled").into_response();
    }
    let address = q.address.trim().to_lowercase();
    if !valid_eth_address(&address) {
        return (StatusCode::BAD_REQUEST, "invalid address").into_response();
    }
    let Some(host) = request_host(&headers) else {
        return (StatusCode::BAD_REQUEST, "missing host").into_response();
    };
    let issued = session::now_unix();
    let exp = issued + NONCE_TTL_SECS;
    let Some(nonce) = session::mac_b64(&challenge_payload(&host, &address, exp)) else {
        return (StatusCode::FORBIDDEN, "admin disabled").into_response();
    };
    let scheme = if is_https(&headers) { "https" } else { "http" };
    let message = build_message(
        &host,
        scheme,
        &address,
        chain_id(&state.eth_network),
        &nonce,
        issued,
        exp,
    );
    Json(json!({ "message": message })).into_response()
}

/// The value HMAC'd into the nonce — binds host, address, and expiry together.
fn challenge_payload(host: &str, address: &str, exp: i64) -> String {
    format!("{host}|{address}|{exp}")
}

#[derive(Deserialize)]
pub struct VerifyReq {
    pub message: String,
    pub signature: String,
}

struct ParsedMessage {
    host: String,
    address: String,
    nonce: String,
    exp: i64,
}

fn line_after<'a>(message: &'a str, label: &str) -> Option<&'a str> {
    message
        .lines()
        .find_map(|l| l.trim().strip_prefix(label).map(|r| r.trim()))
        .filter(|s| !s.is_empty())
}

fn parse_message(message: &str) -> Option<ParsedMessage> {
    let host = message
        .lines()
        .next()?
        .split(" wants you to sign in")
        .next()?
        .trim()
        .to_lowercase();
    if host.is_empty() {
        return None;
    }
    let address = message
        .lines()
        .map(|l| l.trim())
        .find(|l| valid_eth_address(l))?
        .to_lowercase();
    let nonce = line_after(message, "Nonce:")?.to_string();
    let exp_raw = line_after(message, "Expiration Time:")?;
    let exp = chrono::DateTime::parse_from_rfc3339(exp_raw)
        .ok()?
        .timestamp();
    Some(ParsedMessage {
        host,
        address,
        nonce,
        exp,
    })
}

/// `POST /admin/auth/verify` — validate the host/expiry/nonce binding, recover the
/// signer, gate on the allowlist, and set the session cookie. Any failure ⇒ 403.
pub async fn verify(headers: HeaderMap, Json(req): Json<VerifyReq>) -> Response {
    let denied = || (StatusCode::FORBIDDEN, "sign-in failed").into_response();

    // Fail closed (and skip the ecrecover) when admin is not configured.
    if !session::admin_enabled() {
        return denied();
    }
    let Some(host) = request_host(&headers) else {
        return denied();
    };
    let Some(parsed) = parse_message(&req.message) else {
        return denied();
    };
    // The message must have been minted for THIS host, must not be expired, and
    // its nonce must be our HMAC over (host, address, exp).
    if parsed.host != host {
        return denied();
    }
    if parsed.exp <= session::now_unix() {
        return denied();
    }
    if !session::mac_verify(
        &challenge_payload(&parsed.host, &parsed.address, parsed.exp),
        &parsed.nonce,
    ) {
        return denied();
    }

    // Recover the signer. `recover_address` applies the EIP-191 prefix itself, so
    // we pass the raw message bytes exactly as the wallet signed them.
    let recovered =
        match catalyrst_crypto::recover::recover_address(req.message.as_bytes(), &req.signature) {
            Ok(addr) => addr.to_lowercase(),
            Err(_) => return denied(),
        };
    // The recovered signer must be the address the challenge was issued for.
    if recovered != parsed.address {
        return denied();
    }

    // Mint the cookie. `mint` returns None when no secret is configured; round-
    // trip through `verify` to enforce allowlist membership and fail closed for
    // non-allowlisted signers.
    let Some(cookie) = session::mint(&recovered) else {
        return denied();
    };
    if session::verify(&cookie).is_none() {
        return denied();
    }
    let set_cookie = session::set_cookie_header(&cookie, cookie_secure());

    let mut resp = Json(json!({ "authenticated": true, "address": recovered })).into_response();
    match set_cookie.parse() {
        Ok(h) => {
            resp.headers_mut().insert(header::SET_COOKIE, h);
            resp
        }
        Err(_) => denied(),
    }
}

/// `POST /admin/auth/logout` — clear the session cookie.
pub async fn logout() -> Response {
    let mut resp = Json(json!({ "authenticated": false })).into_response();
    if let Ok(h) = session::clear_cookie_header().parse() {
        resp.headers_mut().insert(header::SET_COOKIE, h);
    }
    resp
}

/// `GET /admin/auth/me` — report the current auth state. 200 when authed (with
/// the address), 401 otherwise.
pub async fn me(headers: HeaderMap) -> Response {
    match cookie_value(&headers, session::COOKIE_NAME).and_then(|v| session::verify(&v)) {
        Some(address) => (
            StatusCode::OK,
            Json(json!({ "authenticated": true, "address": address })),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "authenticated": false })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_eth_address_checks_shape() {
        assert!(valid_eth_address(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(valid_eth_address(
            "0xAbC0000000000000000000000000000000000000"
        ));
        assert!(!valid_eth_address("0x123")); // too short
        assert!(!valid_eth_address(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )); // no 0x
        assert!(!valid_eth_address("0x../../etc/passwd")); // path injection attempt
        assert!(!valid_eth_address(
            "0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        )); // non-hex
    }

    #[test]
    fn url_host_extracts_host() {
        assert_eq!(
            url_host("https://realm.example.com/admin").as_deref(),
            Some("realm.example.com")
        );
        assert_eq!(
            url_host("http://127.0.0.1:5141").as_deref(),
            Some("127.0.0.1:5141")
        );
        assert_eq!(
            url_host("https://h.test/a/b?c=d").as_deref(),
            Some("h.test")
        );
    }

    #[test]
    fn origin_ok_matches_host() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "realm.test".parse().unwrap());
        // no origin → allowed
        assert!(origin_ok(&h));
        // matching origin → allowed
        h.insert(header::ORIGIN, "https://realm.test".parse().unwrap());
        assert!(origin_ok(&h));
        // mismatched origin → rejected
        h.insert(header::ORIGIN, "https://evil.test".parse().unwrap());
        assert!(!origin_ok(&h));
    }

    #[test]
    fn parse_message_roundtrips_build() {
        let msg = build_message(
            "realm.test",
            "https",
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
            "the-nonce-value",
            1_900_000_000,
            1_900_000_300,
        );
        let p = parse_message(&msg).expect("parses");
        assert_eq!(p.host, "realm.test");
        assert_eq!(p.address, "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(p.nonce, "the-nonce-value");
        assert_eq!(p.exp, 1_900_000_300);
    }

    #[test]
    fn parse_message_rejects_garbage() {
        assert!(parse_message("not a real message").is_none());
    }

    #[test]
    fn cookie_value_finds_named_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            "other=1; cat_admin=the-value; x=2".parse().unwrap(),
        );
        assert_eq!(cookie_value(&h, "cat_admin").as_deref(), Some("the-value"));
        assert!(cookie_value(&h, "missing").is_none());
    }

    #[tokio::test]
    async fn me_without_cookie_is_401() {
        let resp = me(HeaderMap::new()).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
