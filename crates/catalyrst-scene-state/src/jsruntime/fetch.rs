use std::rc::Rc;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::delegation::{StorageDelegation, RENEWAL_TIMEOUT};
use crate::runtime::RuntimeLimits;

pub const MAX_URL_LENGTH: usize = 2048;

const MAX_RESPONSE_HEADER_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct StorageCtx {
    pub origin: url::Url,
    pub allow_http_loopback: bool,
    pub delegation: Arc<Mutex<Option<StorageDelegation>>>,
    pub renew_tx: Option<mpsc::UnboundedSender<oneshot::Sender<()>>>,
}

pub(super) struct FetchWiring {
    pub ctx: StorageCtx,
    pub tx: mpsc::UnboundedSender<FetchJob>,
    pub results: std::sync::mpsc::Receiver<FetchResult>,
}

pub(super) struct FetchJob {
    pub id: u64,
    pub method: String,
    pub url: url::Url,
    pub body: Option<Vec<u8>>,
    pub headers: Vec<(String, String)>,
}

pub(super) struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub(super) struct FetchResult {
    pub id: u64,
    pub outcome: Result<FetchResponse, String>,
}

fn is_loopback_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(a)) => a.is_loopback(),
        Some(url::Host::Ipv6(a)) => a.is_loopback(),
        None => false,
    }
}

fn check_scheme(url: &url::Url, allow_http_loopback: bool) -> Result<(), &'static str> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_http_loopback && is_loopback_host(url) => Ok(()),
        "http" => Err("http is only allowed for loopback origins"),
        _ => Err("unsupported URL scheme"),
    }
}

pub fn parse_origin(raw: &str, allow_http_loopback: bool) -> Result<url::Url, &'static str> {
    let url = url::Url::parse(raw.trim()).map_err(|_| "not an absolute URL")?;
    if url.host().is_none() {
        return Err("origin has no host");
    }
    check_scheme(&url, allow_http_loopback)?;
    Ok(url)
}

pub(super) fn validate_scene_url(raw: &str, ctx: &StorageCtx) -> Result<url::Url, &'static str> {
    if raw.len() > MAX_URL_LENGTH {
        return Err("URL too long");
    }
    let url = url::Url::parse(raw).map_err(|_| "invalid URL")?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL userinfo is not allowed");
    }
    check_scheme(&url, ctx.allow_http_loopback)?;
    if url.scheme() != ctx.origin.scheme()
        || url.host() != ctx.origin.host()
        || url.port_or_known_default() != ctx.origin.port_or_known_default()
    {
        return Err("URL is outside the configured storage origin");
    }
    Ok(url)
}

pub(super) fn sanitize_scene_headers(headers: Vec<(String, String)>) -> Vec<(String, String)> {
    headers
        .into_iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            !(name.starts_with("x-identity-")
                || matches!(
                    name.as_str(),
                    "x-authoritative-scope"
                        | "x-original-path"
                        | "host"
                        | "content-length"
                        | "cookie"
                        | "authorization"
                ))
        })
        .collect()
}

async fn current_delegation(ctx: &StorageCtx) -> Result<StorageDelegation, String> {
    let now = Utc::now();
    if let Some(d) = ctx.delegation.lock().clone() {
        if !d.near_expiry(now) {
            return Ok(d);
        }
    }
    if let Some(renew) = &ctx.renew_tx {
        let (done_tx, done_rx) = oneshot::channel();
        if renew.send(done_tx).is_ok() {
            let _ = tokio::time::timeout(RENEWAL_TIMEOUT, done_rx).await;
        }
    }
    match ctx.delegation.lock().clone() {
        Some(d) if !d.is_expired(Utc::now()) => Ok(d),
        Some(_) => Err("storage delegation expired".to_string()),
        None => Err("no storage delegation".to_string()),
    }
}

async fn run_job(
    client: &reqwest::Client,
    ctx: &StorageCtx,
    limits: &RuntimeLimits,
    job: FetchJob,
) -> Result<FetchResponse, String> {
    let url = validate_scene_url(job.url.as_str(), ctx).map_err(str::to_string)?;
    let delegation = current_delegation(ctx).await?;

    let method =
        reqwest::Method::from_bytes(job.method.as_bytes()).map_err(|_| "invalid method")?;
    let mut req = client.request(method, url.clone());
    for (name, value) in sanitize_scene_headers(job.headers) {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(&value),
        ) {
            req = req.header(name, value);
        }
    }

    let timestamp = Utc::now().timestamp_millis().to_string();
    let path = match url.query() {
        Some(q) if !q.is_empty() => format!("{}?{}", url.path(), q),
        _ => url.path().to_string(),
    };
    let metadata = serde_json::json!({
        "origin": "catalyrst-scene-state://",
        "signer": "dcl:authoritative-server",
        "isGuest": false,
        "realmName": delegation.world,
        "realm": { "serverName": delegation.world },
        "sceneId": delegation.scene_id,
        "parcel": delegation.parcel,
    })
    .to_string();
    let payload = format!("{}:{}:{}:{}", job.method, path, timestamp, metadata).to_lowercase();
    let chain = catalyrst_crypto::create_simple_auth_chain(&delegation.ephemeral, &payload)
        .map_err(|_| "signing failed")?;
    let links = chain.as_array().cloned().unwrap_or_default();
    for (i, link) in links.iter().enumerate() {
        req = req.header(format!("x-identity-auth-chain-{i}"), link.to_string());
    }
    req = req
        .header("x-identity-timestamp", &timestamp)
        .header("x-identity-metadata", &metadata)
        .header("x-authoritative-scope", &delegation.scope_header);
    if let Some(body) = job.body {
        req = req.body(body);
    }

    let resp = req.send().await.map_err(|_| "request failed")?;

    let status = resp.status();
    let drop_location = status.is_redirection();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut header_bytes = 0usize;
    for (name, value) in resp.headers() {
        let name = name.as_str();
        if name == "set-cookie" || (drop_location && name == "location") {
            continue;
        }
        let Ok(value) = value.to_str() else { continue };
        header_bytes += name.len() + value.len();
        if header_bytes > MAX_RESPONSE_HEADER_BYTES {
            break;
        }
        headers.push((name.to_string(), value.to_string()));
    }
    let body = crate::scene_fetcher::read_body_capped(resp, limits.fetch_max_response_bytes)
        .await
        .map_err(|_| "response too large or unreadable")?;
    Ok(FetchResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

pub(super) fn spawn_fetch_worker(
    ctx: StorageCtx,
    limits: RuntimeLimits,
    mut jobs: mpsc::UnboundedReceiver<FetchJob>,
    results: std::sync::mpsc::Sender<FetchResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("scene-fetch".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build fetch worker runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(std::time::Duration::from_millis(limits.fetch_timeout_ms))
                    .connect_timeout(std::time::Duration::from_millis(
                        limits.fetch_timeout_ms.min(5_000),
                    ))
                    .build()
                    .expect("build fetch worker client");
                let ctx = Rc::new(ctx);
                while let Some(job) = jobs.recv().await {
                    let ctx = Rc::clone(&ctx);
                    let client = client.clone();
                    let results = results.clone();
                    tokio::task::spawn_local(async move {
                        let id = job.id;
                        let outcome = run_job(&client, &ctx, &limits, job).await;
                        let _ = results.send(FetchResult { id, outcome });
                    });
                }
            });
        })
        .expect("spawn fetch worker thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(origin: &str, allow_http_loopback: bool) -> StorageCtx {
        StorageCtx {
            origin: parse_origin(origin, allow_http_loopback).unwrap(),
            allow_http_loopback,
            delegation: Arc::new(Mutex::new(None)),
            renew_tx: None,
        }
    }

    #[test]
    fn rejects_everything_outside_an_https_storage_origin() {
        let c = ctx("https://storage.decentraland.org", false);
        for raw in [
            "https://storage.decentraland.org.evil.com/values/k",
            "https://evilstorage.decentraland.org/values/k",
            "https://storage.decentraland.org@evil.com/values/k",
            "https://user:pw@storage.decentraland.org/values/k",
            "http://storage.decentraland.org/values/k",
            "http://169.254.169.254/latest/meta-data/",
            "http://127.0.0.1:5140/",
            "http://[::1]/",
            "http://localhost/",
            "http://192.168.1.1/",
            "https://storage.decentraland.org:8443/values/k",
            "ftp://storage.decentraland.org/values/k",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(
                validate_scene_url(raw, &c).is_err(),
                "{raw} must be rejected"
            );
        }
        assert!(validate_scene_url("https://storage.decentraland.org/values/k?x=1", &c).is_ok());
        assert!(validate_scene_url("https://STORAGE.decentraland.org/values/k", &c).is_ok());
    }

    #[test]
    fn loopback_http_is_allowed_only_with_the_dev_flag_and_matching_port() {
        let c = ctx("http://127.0.0.1:5151", true);
        assert!(validate_scene_url("http://127.0.0.1:5151/values/k", &c).is_ok());
        for raw in [
            "http://127.0.0.1:5152/values/k",
            "http://internal.corp/values/k",
            "http://192.168.1.1:5151/values/k",
            "http://localhost:5151/values/k",
            "https://127.0.0.1:5151/values/k",
        ] {
            assert!(
                validate_scene_url(raw, &c).is_err(),
                "{raw} must be rejected"
            );
        }

        assert!(parse_origin("http://127.0.0.1:5151", false).is_err());
        assert!(parse_origin("http://internal.corp:5151", true).is_err());
    }

    #[test]
    fn caps_url_length() {
        let c = ctx("https://storage.decentraland.org", false);
        let long = format!(
            "https://storage.decentraland.org/values/{}",
            "k".repeat(MAX_URL_LENGTH)
        );
        assert_eq!(validate_scene_url(&long, &c), Err("URL too long"));
    }

    #[test]
    fn scene_header_filter_strips_identity_and_transport_headers() {
        let kept = sanitize_scene_headers(vec![
            ("X-Identity-Timestamp".into(), "0".into()),
            ("x-identity-auth-chain-0".into(), "{}".into()),
            ("x-identity-metadata".into(), "evil".into()),
            ("X-Authoritative-Scope".into(), "evil".into()),
            ("X-Original-Path".into(), "/values/other".into()),
            ("Host".into(), "evil.com".into()),
            ("content-length".into(), "999".into()),
            ("Cookie".into(), "a=b".into()),
            ("authorization".into(), "Bearer x".into()),
            ("X-Custom".into(), "yes".into()),
            ("content-type".into(), "application/json".into()),
        ]);
        assert_eq!(
            kept,
            vec![
                ("X-Custom".to_string(), "yes".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ]
        );
    }
}
