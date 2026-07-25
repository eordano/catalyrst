use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Html;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::admin::{auth, session};
use crate::state::AppState;

mod catalog;
mod controls;
mod pages;

pub use pages::*;

use catalog::CATALOG;

const TEMPLATE: &str = include_str!("../../console.html");

struct Svc {
    name: &'static str,
    member: &'static str,
    reference: &'static str,
    desc: &'static str,
    path: &'static str,
}
struct Group {
    key: &'static str,
    title: &'static str,
    bundle: &'static str,
    multi: bool,
    detail: &'static [(&'static str, &'static str)],
    services: &'static [Svc],
}

fn group_by_key(key: &str) -> Option<&'static Group> {
    CATALOG.iter().find(|g| g.key == key)
}

pub(crate) fn service_urls() -> &'static BTreeMap<String, String> {
    static M: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = BTreeMap::new();
        if let Ok(raw) = std::env::var("CATALYRST_SERVICE_URLS") {
            for pair in raw.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    let (k, v) = (k.trim(), v.trim().trim_end_matches('/'));
                    if !k.is_empty() && !v.is_empty() {
                        m.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        m
    })
}

pub(crate) fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default()
    })
}

async fn probe_up(url: &str) -> bool {
    matches!(client().get(url).send().await, Ok(r) if r.status().is_success())
}

async fn fetch_json(url: &str) -> Option<Value> {
    let r = client().get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json::<Value>().await.ok()
}

fn num(v: &Value, path: &str) -> Option<u64> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    cur.as_u64().or_else(|| cur.as_f64().map(|f| f as u64))
}

#[derive(Clone, Default)]
struct GroupHealth {
    up: bool,
    members: BTreeMap<String, bool>,
}

#[derive(Clone, Default)]
struct Activity {
    users: Option<u64>,
    peers: Option<u64>,
    islands: Option<u64>,
    uptime_secs: Option<u64>,
    hot_scenes: Option<u64>,
    ss_connections: Option<u64>,
    ss_scenes: Option<u64>,
    ab_queue: Option<u64>,
}

#[derive(Clone, Default)]
struct Probed {
    groups: BTreeMap<String, GroupHealth>,
    activity: Activity,
}

const PROBE_TTL: Duration = Duration::from_secs(5);

fn health_path(g: &Group) -> &'static str {
    match g.key {
        "scene-state" => "/status",
        _ => "/health",
    }
}

async fn probe() -> Probed {
    static CACHE: OnceLock<AsyncMutex<Option<(Instant, Probed)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| AsyncMutex::new(None));
    {
        let g = cache.lock().await;
        if let Some((at, ref p)) = *g {
            if at.elapsed() < PROBE_TTL {
                return p.clone();
            }
        }
    }
    let urls = service_urls();

    let health_futs = CATALOG
        .iter()
        .filter(|g| g.key != "content")
        .filter_map(|g| {
            urls.get(g.key).map(|base| async move {
                let gh = if g.multi {
                    match fetch_json(&format!("{base}/health")).await {
                        Some(v) => {
                            let members = v
                                .get("members")
                                .and_then(|m| m.as_object())
                                .map(|m| {
                                    m.iter()
                                        .map(|(k, val)| (k.clone(), val.as_str() == Some("up")))
                                        .collect()
                                })
                                .unwrap_or_default();
                            let up = v.get("status").and_then(|s| s.as_str()) == Some("ok");
                            GroupHealth { up, members }
                        }
                        None => GroupHealth::default(),
                    }
                } else {
                    GroupHealth {
                        up: probe_up(&format!("{base}{}", health_path(g))).await,
                        members: BTreeMap::new(),
                    }
                };
                (g.key.to_string(), gh)
            })
        });
    let groups: BTreeMap<String, GroupHealth> = futures::future::join_all(health_futs)
        .await
        .into_iter()
        .collect();

    let explore = urls.get("explore");
    let scene = urls.get("scene-state");
    let create = urls.get("create");
    let (core, stats, hot, ss, queues) = tokio::join!(
        async {
            match explore {
                Some(b) => fetch_json(&format!("{b}/core-status")).await,
                None => None,
            }
        },
        async {
            match explore {
                Some(b) => fetch_json(&format!("{b}/stats/health")).await,
                None => None,
            }
        },
        async {
            match explore {
                Some(b) => fetch_json(&format!("{b}/hot-scenes")).await,
                None => None,
            }
        },
        async {
            match scene {
                Some(b) => fetch_json(&format!("{b}/status")).await,
                None => None,
            }
        },
        async {
            match create {
                Some(b) => fetch_json(&format!("{b}/queues/status")).await,
                None => None,
            }
        },
    );
    let arr_len = |v: &Option<Value>, key: &str| -> Option<u64> {
        v.as_ref()?.get(key)?.as_array().map(|a| a.len() as u64)
    };
    let activity = Activity {
        users: core.as_ref().and_then(|v| num(v, "userCount")),
        peers: stats.as_ref().and_then(|v| num(v, "peersTotal")),
        islands: stats.as_ref().and_then(|v| num(v, "islandsTotal")),
        uptime_secs: stats.as_ref().and_then(|v| num(v, "uptimeSecs")),
        hot_scenes: hot
            .as_ref()
            .and_then(|v| v.as_array().map(|a| a.len() as u64)),
        ss_connections: ss.as_ref().and_then(|v| num(v, "connections")),
        ss_scenes: arr_len(&ss, "loadedScenes"),
        ab_queue: queues.as_ref().map(|v| {
            ["windowsPendingJobs", "macPendingJobs", "webglPendingJobs"]
                .iter()
                .filter_map(|k| v.get(k).and_then(|a| a.as_array()).map(|a| a.len() as u64))
                .sum()
        }),
    };

    let p = Probed { groups, activity };
    *cache.lock().await = Some((Instant::now(), p.clone()));
    p
}

fn group_health(g: &Group, content_healthy: bool, p: &Probed) -> Option<bool> {
    if g.key == "content" {
        Some(content_healthy)
    } else if service_urls().contains_key(g.key) {
        Some(p.groups.get(g.key).map(|gh| gh.up).unwrap_or(false))
    } else {
        None
    }
}

fn svc_health(g: &Group, s: &Svc, content_healthy: bool, p: &Probed) -> Option<bool> {
    if g.key == "content" {
        Some(content_healthy)
    } else if !service_urls().contains_key(g.key) {
        None
    } else if g.multi {
        p.groups
            .get(g.key)
            .map(|gh| gh.members.get(s.member).copied().unwrap_or(false))
    } else {
        Some(p.groups.get(g.key).map(|gh| gh.up).unwrap_or(false))
    }
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

fn dot(h: Option<bool>) -> (&'static str, &'static str) {
    match h {
        Some(true) => ("ok", "Online"),
        Some(false) => ("bad", "Unreachable"),
        None => ("unk", "Not configured"),
    }
}

fn mode_str(read_only: bool) -> &'static str {
    if read_only {
        "read-only"
    } else {
        "write"
    }
}

fn network_name(net: &str) -> &str {
    match net {
        "mainnet" => "Ethereum mainnet",
        "sepolia" => "Sepolia testnet",
        other => other,
    }
}

fn short_commit(h: &str) -> &str {
    if h.len() > 10 {
        &h[..10]
    } else {
        h
    }
}

fn human(n: u64) -> String {
    let s = n.to_string();
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

fn fmt_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn opt_big(v: Option<u64>) -> String {
    v.map(human).unwrap_or_else(|| "—".into())
}

fn realm_base_url(state: &AppState) -> Option<String> {
    let url = state.content_public_url.trim();
    if url.is_empty() {
        return None;
    }
    let scheme_end = url.find("://")? + 3;
    let host_end = url[scheme_end..]
        .find('/')
        .map(|i| scheme_end + i)
        .unwrap_or(url.len());
    let base = &url[..host_end];
    if base.contains("127.0.0.1") || base.contains("localhost") {
        None
    } else {
        Some(base.to_string())
    }
}

fn page(state: &AppState, title: &str, active: &str, body: &str) -> Html<String> {
    let nav = [("/", "Overview", "overview"), ("/admin", "Admin", "admin")]
        .iter()
        .map(|(href, label, key)| {
            let on = if *key == active { " on" } else { "" };
            format!("<a href=\"{href}\" class=\"nav-l{on}\">{label}</a>")
        })
        .collect::<String>();
    let realm = state.realm_name.clone().unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    let html = TEMPLATE
        .replace("<!--SSR:title-->", &esc(title))
        .replace("<!--SSR:realmname-->", &esc(&realm))
        .replace("<!--SSR:nav-->", &nav)
        .replace("<!--SSR:version-->", &esc(&state.content_version))
        .replace("<!--SSR:commit-->", &esc(short_commit(&state.commit_hash)))
        .replace("<!--SSR:network-->", &esc(&state.eth_network))
        .replace("<!--SSR:mode-->", mode_str(state.is_read_only()))
        .replace("<!--SSR:now-->", &now)
        .replace("<!--SSR:main-->", body);
    Html(html)
}

fn stat(label: &str, value: &str, small: bool, hl: bool) -> String {
    let cls = if hl { "stat hl" } else { "stat" };
    let big = if small { "big sm" } else { "big" };
    format!(
        "<div class=\"{cls}\"><div class=\"lab\">{}</div><div class=\"{big}\">{}</div></div>",
        esc(label),
        value
    )
}

struct ViewerAdmin {
    enabled: bool,
    is_admin: bool,
    addr: Option<String>,
}

fn viewer_admin(headers: &HeaderMap) -> ViewerAdmin {
    let enabled = session::admin_enabled();
    let addr = if enabled {
        auth::cookie_value(headers, session::COOKIE_NAME).and_then(|v| session::verify(&v))
    } else {
        None
    };
    ViewerAdmin {
        enabled,
        is_admin: addr.is_some(),
        addr,
    }
}

fn has_service(key: &str) -> bool {
    service_urls().contains_key(key)
}

fn env_set_any(names: &[&str]) -> bool {
    names
        .iter()
        .any(|n| std::env::var(n).is_ok_and(|v| !v.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_placeholders_are_all_filled() {
        const FILLED: &[&str] = &[
            "title",
            "realmname",
            "nav",
            "version",
            "commit",
            "network",
            "mode",
            "now",
            "main",
        ];
        let mut rest = TEMPLATE;
        while let Some(i) = rest.find("<!--SSR:") {
            let after = &rest[i + "<!--SSR:".len()..];
            let end = after.find("-->").expect("unterminated SSR placeholder");
            let key = &after[..end];
            assert!(
                FILLED.contains(&key),
                "template has unfilled placeholder: {key}"
            );
            rest = &after[end + 3..];
        }
    }

    #[test]
    fn catalog_is_well_formed() {
        assert_eq!(CATALOG[0].key, "content", "content group must be first");
        for g in CATALOG {
            assert!(!g.title.is_empty() && !g.bundle.is_empty());
            assert!(!g.services.is_empty(), "{} has no services", g.key);

            if g.multi {
                for s in g.services {
                    assert!(
                        !s.member.is_empty(),
                        "{}/{} missing member key",
                        g.key,
                        s.name
                    );
                }
            }
        }

        let mut keys: Vec<&str> = CATALOG.iter().map(|g| g.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), CATALOG.len(), "duplicate group key");
    }

    #[test]
    fn escaping_neutralizes_markup() {
        assert_eq!(esc("<a href=\"x\">&"), "&lt;a href=&quot;x&quot;&gt;&amp;");
    }

    #[test]
    fn human_groups_thousands() {
        assert_eq!(human(0), "0");
        assert_eq!(human(42), "42");
        assert_eq!(human(1234), "1,234");
        assert_eq!(human(1234567), "1,234,567");
    }

    #[test]
    fn num_reads_nested_paths() {
        assert_eq!(num(&serde_json::json!({"a": {"b": 5}}), "a.b"), Some(5));
        assert_eq!(
            num(&serde_json::json!({"userCount": 12}), "userCount"),
            Some(12)
        );
        assert_eq!(num(&serde_json::json!({"a": "x"}), "a"), None);
    }
}
