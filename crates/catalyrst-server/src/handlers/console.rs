//! Server-side-rendered operator surface for the realm:
//!
//! - `GET /`                — service-plane landing page (public)
//! - `GET /admin`           — cross-service ops console (live activity + status)
//! - `GET /admin/{service}` — per-service detail (raw status of one bundle)
//!
//! SSR-first by design — every value is rendered into the HTML by the server
//! from `AppState` and a short-TTL probe of the sibling bundles. The embedded
//! JavaScript only progressively enhances: relative timestamps and an opt-in
//! 15s auto-refresh. Every view is a real URL with no hidden client state, so
//! any page (including a per-service deep link) can be shared and reproduces
//! exactly what the sharer saw.
//!
//! Live cross-service data comes from each bundle's unauthenticated
//! introspection endpoints — the aggregated `/health` (which lists every member
//! `up`/`down`), archipelago's `/core-status` + `/stats/health` + `/hot-scenes`,
//! scene-state's `/status`, and the AB build `/queues/status` — probed
//! server-side over `CATALYRST_SERVICE_URLS`. Bundles with no configured URL
//! render as "not configured", never "down".
//!
//! `/admin*` exposes operational detail and is intentionally NOT proxied on the
//! public edge (`docs/deploy/nginx-catalyrst-bundles.conf` 404s `/admin`); reach
//! it on the loopback port or over the tailnet, and front it with auth before
//! any public exposure.

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

const TEMPLATE: &str = include_str!("../console.html");

// ───────────────────────── service catalog ─────────────────────────
// One card per deployment bundle. `key` is the lookup in CATALYRST_SERVICE_URLS
// and the `/admin/{key}` detail slug. `multi` bundles expose an aggregated
// `/health` whose `members` map gives per-service up/down; single-service
// bundles are up iff `/health` returns 2xx. `detail` lists the introspection
// endpoints surfaced (raw) on the per-service page. Each `Svc.member` matches a
// key in the bundle `/health.members` map (empty for single-service / content).
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

const CATALOG: &[Group] = &[
    Group {
        key: "content",
        title: "Content core",
        bundle: "catalyrst-live · :5141",
        multi: false,
        detail: &[("About", "/about"), ("Content status", "/content/status"), ("Lambdas status", "/lambdas/status")],
        services: &[
            Svc { name: "Content store", member: "", reference: "catalyst content-server", desc: "Content-addressed scenes, wearables, emotes & profiles (ADR-45 / IPFS CIDs)", path: "/content/status" },
            Svc { name: "Lambdas", member: "", reference: "catalyst lambdas", desc: "Profiles, wearable/emote catalogs, LAND & third-party items", path: "/lambdas/status" },
            Svc { name: "About", member: "", reference: "realm descriptor", desc: "Realm identity, comms config & health for explorer clients", path: "/about" },
        ],
    },
    Group {
        key: "explore",
        title: "Explore",
        bundle: "catalyrst-explore · :5143",
        multi: true,
        detail: &[("Bundle health", "/health"), ("Archipelago core", "/core-status"), ("Archipelago stats", "/stats/health"), ("Hot scenes", "/hot-scenes"), ("Comms islands", "/comms/islands"), ("Comms parcels", "/comms/parcels")],
        services: &[
            Svc { name: "Places", member: "places", reference: "places.decentraland.org", desc: "Place discovery, favorites & content-moderation reports", path: "/places" },
            Svc { name: "Events", member: "events", reference: "events.decentraland.org", desc: "Event schedules, posters & attendance", path: "/events/" },
            Svc { name: "Map", member: "map", reference: "atlas-server", desc: "Genesis-city tiles, minimap & map.png renderer", path: "/v1/map.png" },
            Svc { name: "Worlds", member: "worlds", reference: "worlds-content-server", desc: "World realms — about, permissions & comms", path: "/worlds" },
            Svc { name: "Archipelago", member: "archipelago", reference: "archipelago-workers", desc: "Peer clustering, ws-connector & hot scenes", path: "/hot-scenes" },
            Svc { name: "Lists", member: "lists", reference: "dcl-lists", desc: "Curated POIs & banned-name denylist", path: "/pois" },
        ],
    },
    Group {
        key: "create",
        title: "Create",
        bundle: "catalyrst-create · :5144",
        multi: true,
        detail: &[("Bundle health", "/health"), ("AB build queues", "/queues/status"), ("AB registry status", "/status")],
        services: &[
            Svc { name: "Builder", member: "builder", reference: "builder-server", desc: "Collection items, storage & newsletter", path: "" },
            Svc { name: "Camera Reel", member: "camera-reel", reference: "camera-reel-service", desc: "Content-addressed in-world photo store", path: "/images" },
            Svc { name: "AB Registry", member: "ab-registry", reference: "asset-bundle-registry", desc: "Asset-bundle build status, versions & bundles", path: "/registry" },
        ],
    },
    Group {
        key: "social",
        title: "Social",
        bundle: "catalyrst-social · :5145",
        multi: true,
        detail: &[("Bundle health", "/health"), ("Comms status", "/status")],
        services: &[
            Svc { name: "Communities", member: "communities", reference: "social-service-ea", desc: "Community routes with authority-chain federation", path: "" },
            Svc { name: "Comms Gatekeeper", member: "comms", reference: "comms-gatekeeper", desc: "LiveKit tokens, scene bans, voice & Cast 2.0", path: "" },
            Svc { name: "Notifications", member: "notifications", reference: "notifications", desc: "Signed-fetch notification reader & marker", path: "" },
            Svc { name: "Badges", member: "badges", reference: "badges", desc: "Profile badge state", path: "/badges/" },
            Svc { name: "Autotranslate", member: "media", reference: "autotranslate-server", desc: "LibreTranslate-compatible /translate", path: "" },
        ],
    },
    Group {
        key: "data",
        title: "Data",
        bundle: "catalyrst-data · :5146",
        multi: true,
        detail: &[("Bundle health", "/health")],
        services: &[
            Svc { name: "Marketplace", member: "market", reference: "marketplace-server", desc: "Catalog, items, orders, bids, sales & trades", path: "/v1/catalog" },
            Svc { name: "Transactions", member: "economy", reference: "transactions-server", desc: "Meta-transaction relay", path: "" },
            Svc { name: "Price feed", member: "price", reference: "coingecko proxy", desc: "MANA / token spot prices over the mana_price archive", path: "/api/v3/simple/price" },
            Svc { name: "Credits", member: "credits", reference: "credits.decentraland.org", desc: "Marketplace Credits program API", path: "/seasons" },
            Svc { name: "EVM RPC", member: "rpc", reference: "rpc.decentraland.org", desc: "Method-filtered read-only JSON-RPC relay", path: "" },
        ],
    },
    Group {
        key: "ab-cdn",
        title: "Asset bundle CDN",
        bundle: "catalyrst-ab-cdn · :5147",
        multi: false,
        detail: &[("Health", "/health")],
        services: &[
            Svc { name: "AB CDN", member: "", reference: "ab-cdn", desc: "Content-addressed LODs, manifests & asset-bundle binaries", path: "/manifest/" },
        ],
    },
    Group {
        key: "social-rpc",
        title: "Social RPC",
        bundle: "catalyrst-social-rpc · :5148",
        multi: false,
        detail: &[("Health", "/health"), ("Info", "/info")],
        services: &[
            Svc { name: "Friends & voice", member: "", reference: "social-service-ea", desc: "dcl-rpc WebSocket — friends, presence, blocks, mutes & voice", path: "" },
        ],
    },
    Group {
        key: "scene-state",
        title: "Scene state",
        bundle: "catalyrst-scene-state · :5153",
        multi: false,
        detail: &[("Status", "/status")],
        services: &[
            Svc { name: "SDK7 multiplayer", member: "", reference: "scene-state-server", desc: "Authoritative SDK7 scene state (HTTP + WebSocket CRDT)", path: "" },
        ],
    },
    Group {
        key: "profile-images",
        title: "Profile images",
        bundle: "catalyrst-profile-images · :5154",
        multi: false,
        detail: &[("Health", "/health")],
        services: &[
            Svc { name: "Avatar thumbnails", member: "", reference: "profile-images", desc: "Headless-godot avatar render with disk cache", path: "" },
        ],
    },
    Group {
        key: "explorer-api",
        title: "Explorer API",
        bundle: "catalyrst-explorer-api · :5137",
        multi: false,
        detail: &[("Health", "/health"), ("Auth health", "/auth/health/live")],
        services: &[
            Svc { name: "Realm provider", member: "", reference: "realm-provider", desc: "Realm selection for explorer clients", path: "" },
            Svc { name: "Auth & flags", member: "", reference: "auth-api · feature-flags", desc: "Auth API, blocklist, builder-api & feature flags", path: "" },
        ],
    },
    Group {
        key: "telemetry",
        title: "Observability",
        bundle: "catalyrst-telemetry",
        multi: false,
        detail: &[("Health", "/health")],
        services: &[
            Svc { name: "Telemetry sink", member: "", reference: "sentry + segment", desc: "Client error/analytics ingest stored in Postgres", path: "" },
            Svc { name: "Dashboard", member: "", reference: "metrics.dcl.one", desc: "Errors, metrics, release health, flags & ad-hoc SQL", path: "" },
        ],
    },
];

fn group_by_key(key: &str) -> Option<&'static Group> {
    CATALOG.iter().find(|g| g.key == key)
}

// ───────────────────────── http probing ─────────────────────────
/// `CATALYRST_SERVICE_URLS` = comma-separated `key=baseurl` pairs, e.g.
/// `explore=http://127.0.0.1:5143,data=http://127.0.0.1:5146`. Keys match the
/// catalog `key`s above.
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

/// True iff `url` responds 2xx (any content type — some health endpoints return
/// bare `"ok"`/`"alive"` text rather than JSON).
async fn probe_up(url: &str) -> bool {
    matches!(client().get(url).send().await, Ok(r) if r.status().is_success())
}

/// Fetch + parse a 2xx JSON body, or `None` on any error / non-2xx / non-JSON.
async fn fetch_json(url: &str) -> Option<Value> {
    let r = client().get(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json::<Value>().await.ok()
}

/// Pull a number out of a JSON value at one of several candidate paths
/// (`a.b.c`), returning the first that resolves to a number.
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

/// Probe every configured sibling bundle (health + live activity) concurrently,
/// cached for a few seconds so a burst of page loads / auto-refresh doesn't fan
/// out a fresh round each time. Content is local (computed by the caller), so it
/// is intentionally excluded from this network snapshot.
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

    // per-bundle health (skip content — it is resolved locally)
    let health_futs = CATALOG.iter().filter(|g| g.key != "content").filter_map(|g| {
        urls.get(g.key).map(|base| async move {
            let gh = if g.multi {
                match fetch_json(&format!("{base}/health")).await {
                    Some(v) => {
                        let members = v
                            .get("members")
                            .and_then(|m| m.as_object())
                            .map(|m| m.iter().map(|(k, val)| (k.clone(), val.as_str() == Some("up"))).collect())
                            .unwrap_or_default();
                        let up = v.get("status").and_then(|s| s.as_str()) == Some("ok");
                        GroupHealth { up, members }
                    }
                    None => GroupHealth::default(),
                }
            } else {
                GroupHealth { up: probe_up(&format!("{base}/health")).await, members: BTreeMap::new() }
            };
            (g.key.to_string(), gh)
        })
    });
    let groups: BTreeMap<String, GroupHealth> = futures::future::join_all(health_futs).await.into_iter().collect();

    // live activity (each guarded by whether its bundle is configured)
    let explore = urls.get("explore");
    let scene = urls.get("scene-state");
    let create = urls.get("create");
    let (core, stats, hot, ss, queues) = tokio::join!(
        async { match explore { Some(b) => fetch_json(&format!("{b}/core-status")).await, None => None } },
        async { match explore { Some(b) => fetch_json(&format!("{b}/stats/health")).await, None => None } },
        async { match explore { Some(b) => fetch_json(&format!("{b}/hot-scenes")).await, None => None } },
        async { match scene { Some(b) => fetch_json(&format!("{b}/status")).await, None => None } },
        async { match create { Some(b) => fetch_json(&format!("{b}/queues/status")).await, None => None } },
    );
    let arr_len = |v: &Option<Value>, key: &str| -> Option<u64> {
        v.as_ref()?.get(key)?.as_array().map(|a| a.len() as u64)
    };
    let activity = Activity {
        users: core.as_ref().and_then(|v| num(v, "userCount")),
        peers: stats.as_ref().and_then(|v| num(v, "peersTotal")),
        islands: stats.as_ref().and_then(|v| num(v, "islandsTotal")),
        uptime_secs: stats.as_ref().and_then(|v| num(v, "uptimeSecs")),
        hot_scenes: hot.as_ref().and_then(|v| v.as_array().map(|a| a.len() as u64)),
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

/// Overall health of a bundle: `Some(true/false)` when known, `None` when the
/// bundle has no configured URL (rendered "not configured", never "down").
fn group_health(g: &Group, content_healthy: bool, p: &Probed) -> Option<bool> {
    if g.key == "content" {
        Some(content_healthy)
    } else if service_urls().contains_key(g.key) {
        Some(p.groups.get(g.key).map(|gh| gh.up).unwrap_or(false))
    } else {
        None
    }
}

/// Health of a single member service within its bundle.
fn svc_health(g: &Group, s: &Svc, content_healthy: bool, p: &Probed) -> Option<bool> {
    if g.key == "content" {
        Some(content_healthy)
    } else if !service_urls().contains_key(g.key) {
        None
    } else if g.multi {
        p.groups.get(g.key).map(|gh| gh.members.get(s.member).copied().unwrap_or(false))
    } else {
        Some(p.groups.get(g.key).map(|gh| gh.up).unwrap_or(false))
    }
}

// ───────────────────────── html helpers ─────────────────────────
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
    if read_only { "read-only" } else { "write" }
}

fn network_name(net: &str) -> &str {
    match net {
        "mainnet" => "Ethereum mainnet",
        "sepolia" => "Sepolia testnet",
        other => other,
    }
}

fn short_commit(h: &str) -> &str {
    if h.len() > 10 { &h[..10] } else { h }
}

/// Thousands-separated integer (no locale dep).
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
    if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") }
}

fn opt_big(v: Option<u64>) -> String {
    v.map(human).unwrap_or_else(|| "—".into())
}

/// The realm's public base URL (scheme + host), derived from the configured
/// content URL, or `None` for loopback/unset hosts where an "open in explorer"
/// deep link would be meaningless to share.
fn realm_base_url(state: &AppState) -> Option<String> {
    let url = state.content_public_url.trim();
    if url.is_empty() {
        return None;
    }
    let scheme_end = url.find("://")? + 3;
    let host_end = url[scheme_end..].find('/').map(|i| scheme_end + i).unwrap_or(url.len());
    let base = &url[..host_end];
    if base.contains("127.0.0.1") || base.contains("localhost") {
        None
    } else {
        Some(base.to_string())
    }
}

/// Wrap a rendered page body in the shared shell (header/nav/footer + CSS/JS).
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

// ───────────────────────── admin controls ─────────────────────────
// SSR-first write controls. The server is the source of truth for *which*
// controls exist: a card is emitted only when (a) admin is configured
// (`session::admin_enabled()`), (b) the current viewer holds a valid session
// cookie (`is_admin`), and (c) the control's prerequisites hold (the target
// bundle is configured and any required downstream secret is present). The
// embedded JS never injects a control — it only wires the ones the server
// already rendered, so an unauthenticated share of any /admin URL stays
// read-only.

/// The viewer's admin state for the current request: whether write controls
/// should render at all, whether *this* viewer is an authenticated admin, and
/// (if so) the address to display.
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

/// True iff a bundle key is reachable (has a configured URL).
fn has_service(key: &str) -> bool {
    service_urls().contains_key(key)
}

/// True iff any of `names` is set to a non-empty value (used to gate token-gated
/// controls so we never render a card whose downstream call would 403). Accepts
/// the sibling service's own env name as a fallback, matching the proxy handlers.
fn env_set_any(names: &[&str]) -> bool {
    names
        .iter()
        .any(|n| std::env::var(n).is_ok_and(|v| !v.trim().is_empty()))
}

/// Sign-in / sign-out affordance plus the per-card controls. `which` selects the
/// subset of cards to render so each per-service page surfaces only its own:
/// `"all"` (the /admin overview) emits every applicable card; a group key emits
/// just that bundle's card. Returns an empty string when admin is unconfigured
/// (a small read-only note is rendered separately by the caller).
fn controls(va: &ViewerAdmin, which: &str) -> String {
    if !va.enabled {
        return String::new();
    }
    let mut b = String::new();
    b.push_str("<section><div class=\"shead\"><h3>Operator controls</h3><span class=\"c\">privileged actions — confirm before each</span></div>");

    if !va.is_admin {
        // Unauthenticated viewer: offer the wallet sign-in handshake. The JS
        // (console.html) wires #admin-signin.
        b.push_str("<div class=\"ctlcard\"><div class=\"ctlh\">Sign in</div>");
        b.push_str("<div class=\"ctlbody\"><p class=\"ctldesc\">Write controls are configured for this realm. Authenticate with an allowlisted wallet to enable them.</p>");
        b.push_str("<button class=\"btn\" id=\"admin-signin\">Sign in with wallet</button>");
        b.push_str("<span id=\"admin-who\" class=\"who\"></span>");
        b.push_str("<div class=\"ctl-result\" id=\"signin-result\"></div></div></div>");
        b.push_str("</section>");
        return b;
    }

    // Authenticated admin: show the session pill + sign-out, then the cards.
    let addr = va.addr.clone().unwrap_or_default();
    b.push_str(&format!(
        "<div class=\"ctlcard\"><div class=\"ctlh\">Session</div><div class=\"ctlbody\"><p class=\"ctldesc\">Signed in as <span class=\"mono who\" id=\"admin-who\">{}</span></p><button class=\"btn ghost\" id=\"admin-signout\">Sign out</button></div></div>",
        esc(&addr)
    ));

    let want = |key: &str| which == "all" || which == key;

    // Content — always available (operates on the local process).
    if want("content") {
        b.push_str(&ctl_card(
            "Content",
            "Drop the in-process deployments cache; the next read repopulates from the database.",
            &[ctl_button(
                "Flush deployments cache",
                "/admin/api/content/flush-cache",
                "Flush the deployments cache?",
            )],
        ));
    }

    // Telemetry — issue-state form + ad-hoc read-only SQL.
    if want("telemetry") && has_service("telemetry") {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/issue-state\" data-confirm=\"Update this issue's state?\">");
        inner.push_str("<label>Fingerprint<input name=\"fingerprint\" placeholder=\"issue fingerprint\" required></label>");
        inner.push_str("<label>Status<select name=\"status\"><option value=\"resolve\">resolve</option><option value=\"ignore\">ignore</option><option value=\"unresolve\">unresolve</option></select></label>");
        inner.push_str("<label>Assignee<input name=\"assignee\" placeholder=\"address / handle (optional)\"></label>");
        inner.push_str("<label>Note<input name=\"note\" placeholder=\"optional note\"></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set issue state</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/sql\" data-result=\"#sql-out\">");
        inner.push_str("<label>Ad-hoc SQL <span class=\"hint\">read-only, enforced downstream</span><textarea name=\"sql\" rows=\"3\" placeholder=\"select ...\" required></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Run query</button>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"sql-out\"></pre></form>");
        b.push_str(&ctl_card_raw("Telemetry", "Triage issues and run read-only queries against the telemetry store.", &inner));
    }

    // Create / AB registry — gated on the registry admin token being present.
    if want("create") && has_service("create") && env_set_any(&["AB_REGISTRY_ADMIN_TOKEN", "API_ADMIN_TOKEN"]) {
        b.push_str(&ctl_card(
            "Create",
            "Re-ingest the asset-bundle registry or flush its build cache.",
            &[
                ctl_button("Re-ingest registry", "/admin/api/create/registry-reingest", "Re-ingest the AB registry?"),
                ctl_button("Flush AB cache", "/admin/api/create/flush-ab-cache", "Flush the asset-bundle cache?"),
            ],
        ));
    }

    // Social / comms moderation — gated on the moderator token being present.
    if want("social") && has_service("social") && env_set_any(&["COMMS_MODERATOR_TOKEN", "MODERATOR_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social/user-ban\" data-confirm=\"Ban this user?\">");
        inner.push_str("<label>Address<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str("<label>Reason<input name=\"reason\" placeholder=\"reason (optional)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Ban</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social/user-unban\" data-fields=\"address\" data-confirm=\"Unban this user?\">Unban</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social/user-warning\" data-fields=\"address,reason\" data-confirm=\"Warn this user?\">Warn</button>");
        inner.push_str("</div>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Social moderation", "Ban, unban, or warn a user via the comms gatekeeper.", &inner));
    }

    // Scene-state — gated on the debugging secret being present.
    if want("scene-state") && has_service("scene-state") && env_set_any(&["DEBUGGING_SECRET"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/scene/reload\" data-confirm=\"Reload this scene?\">");
        inner.push_str("<label>Scene<input name=\"name\" placeholder=\"scene id or name\" required></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Reload scene</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Scene state", "Force a reload of an authoritative SDK7 scene.", &inner));
    }

    // ── Content mutations (content-core agent's gated proxies) ──
    if want("content") {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        // NOTE: "Retry failed deployments" and "Regenerate snapshots" are omitted
        // here — the live read-only deployer/snapshot-generator return 501 for
        // those, so surfacing them would be dead buttons. Their endpoints remain
        // for a write-capable build that implements the trait methods.
        inner.push_str(&btn_field("Clear failed deployments", "/admin/api/content/failed-deployments/clear", "Clear the failed-deployments queue?", "", false));
        inner.push_str(&btn_field("Refresh challenge", "/admin/api/content/challenge/refresh", "Refresh the content challenge?", "", false));
        inner.push_str("</div>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field("Pause sync", "/admin/api/content/sync/pause", "Pause content synchronization?", "", false));
        inner.push_str(&btn_field("Resume sync", "/admin/api/content/sync/resume", "Resume content synchronization?", "", false));
        inner.push_str(&btn_field("Force sync", "/admin/api/content/sync/force", "Force a sync pass now?", "", false));
        inner.push_str("</div>");
        inner.push_str("<div class=\"ctl-result\"></div>");
        // Denylist add/remove (entity ids)
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/denylist/add\" data-confirm=\"Add this entity to the content denylist?\">");
        inner.push_str("<label>Entity ID<input name=\"entity_id\" placeholder=\"bafy… / Qm…\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Denylist add</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/content/denylist/remove\" data-fields=\"entity_id\" data-confirm=\"Remove this entity from the denylist?\">Denylist remove</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/content/denylist/list\" data-fields=\"\">List denylist</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // Mode toggles (enabled boolean)
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/read-only\" data-confirm=\"Change the read-only flag?\">");
        inner.push_str("<label>Read-only<select name=\"enabled\"><option value=\"true\">enable (read-only)</option><option value=\"false\">disable (writable)</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set read-only</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/content/accepting-users\" data-confirm=\"Change whether the realm accepts users?\">");
        inner.push_str("<label>Accepting users<select name=\"enabled\"><option value=\"true\">accepting</option><option value=\"false\">closed</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set accepting-users</button>");
        inner.push_str("<div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Content operations", "Synchronization, snapshots, denylist and realm mode for the local content core.", &inner));
    }

    // ── Places (explore bundle) ──
    if want("explore") && has_service("explore") && env_set_any(&["PLACES_ADMIN_AUTH_TOKEN"]) {
        let mut inner = String::new();
        // Reports listing + resolution
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/reports\" data-result=\"#places-reports-out\">");
        inner.push_str("<label>Reports<span class=\"hint\">filter & list moderation reports</span></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<input name=\"status\" placeholder=\"status (optional)\"><input name=\"entity_id\" placeholder=\"entity_id (optional)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List reports</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"places-reports-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/report-resolve\" data-confirm=\"Resolve this report?\">");
        inner.push_str("<label>Resolve report<input name=\"id\" placeholder=\"report id\" required></label>");
        inner.push_str("<label>Status<input name=\"status\" placeholder=\"resolved / dismissed\"></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Resolve report</button><div class=\"ctl-result\"></div></form>");
        // Place disable
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/place-disable\" data-confirm=\"Disable this place?\">");
        inner.push_str("<label>Disable place<input name=\"place_id\" placeholder=\"place id\" required></label>");
        inner.push_str("<label>Disabled<select name=\"disabled\"><option value=\"true\">disable</option><option value=\"false\">re-enable</option></select></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set place disabled</button><div class=\"ctl-result\"></div></form>");
        // Highlight / rating (places + worlds)
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/place-highlight\" data-confirm=\"Update this place's highlight?\">");
        inner.push_str("<label>Place highlight / rating<input name=\"place_id\" placeholder=\"place id\" required></label>");
        inner.push_str("<input name=\"highlighted\" placeholder=\"highlighted true/false\"><input name=\"rating\" placeholder=\"rating (for rating action)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set highlight</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/place-rating\" data-fields=\"place_id,rating\" data-confirm=\"Set this place's rating?\">Set rating</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/world-highlight\" data-confirm=\"Update this world's highlight?\">");
        inner.push_str("<label>World highlight / rating<input name=\"world_id\" placeholder=\"world id\" required></label>");
        inner.push_str("<input name=\"highlighted\" placeholder=\"highlighted true/false\"><input name=\"rating\" placeholder=\"rating (for rating action)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set highlight</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/world-rating\" data-fields=\"world_id,rating\" data-confirm=\"Set this world's rating?\">Set rating</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // POIs CRUD
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/places/poi-create\" data-confirm=\"Create / update this POI?\">");
        inner.push_str("<label>POIs<span class=\"hint\">curated points of interest</span></label>");
        inner.push_str("<input name=\"position\" placeholder=\"position e.g. 0,0\"><input name=\"entity_id\" placeholder=\"entity_id (optional)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/pois-list\" data-fields=\"\">List POIs</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create POI</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/poi-update\" data-fields=\"position,entity_id\" data-confirm=\"Update this POI?\">Update POI</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/places/poi-delete\" data-fields=\"position\" data-confirm=\"Delete this POI?\">Delete POI</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Places", "Moderation reports, place/world highlights & ratings, and curated POIs.", &inner));
    }

    // ── Events (explore bundle) ──
    if want("explore") && has_service("explore") && env_set_any(&["CATALYRST_EVENTS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/events/create\" data-confirm=\"Create this event?\">");
        inner.push_str("<label>Create event<input name=\"name\" placeholder=\"event name\" required></label>");
        inner.push_str("<input name=\"x\" placeholder=\"x\"><input name=\"y\" placeholder=\"y\"><input name=\"start_at\" placeholder=\"start_at ISO\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create event</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/events/moderate\" data-confirm=\"Moderate this event?\">");
        inner.push_str("<label>Moderate event<input name=\"event_id\" placeholder=\"event id\" required></label>");
        inner.push_str("<label>Action<input name=\"approved\" placeholder=\"approved true/false\"></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Moderate event</button><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Events", "Create and moderate realm events.", &inner));
    }

    // ── Worlds (explore bundle) ──
    if want("explore") && has_service("explore") && env_set_any(&["CATALYRST_WORLDS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/list\" data-result=\"#worlds-list-out\">");
        inner.push_str("<label>Worlds<span class=\"hint\">list & inspect world realms</span></label>");
        inner.push_str("<div class=\"btnrow\"><input name=\"limit\" placeholder=\"limit\"><input name=\"offset\" placeholder=\"offset\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List worlds</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-list-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/detail\" data-result=\"#worlds-detail-out\">");
        inner.push_str("<label>World name<input name=\"world_name\" placeholder=\"name.dcl.eth\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Detail</button>");
        inner.push_str("<button class=\"btn\" type=\"button\" data-admin-action=\"/admin/api/worlds/enable\" data-fields=\"world_name\" data-confirm=\"Enable this world?\">Enable</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/disable\" data-fields=\"world_name\" data-confirm=\"Disable this world?\">Disable</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"worlds-detail-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/ban-status\" data-result=\"#worlds-ban-out\">");
        inner.push_str("<label>Ban status<input name=\"world_name\" placeholder=\"name.dcl.eth\" required></label>");
        inner.push_str("<input name=\"address\" placeholder=\"0x… (optional)\"><input name=\"parcel\" placeholder=\"parcel (optional)\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Check ban status</button>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-ban-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/blocked-add\" data-confirm=\"Block this wallet from worlds?\">");
        inner.push_str("<label>Blocked wallets<input name=\"wallet\" placeholder=\"0x…\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Block wallet</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/blocked-remove\" data-fields=\"wallet\" data-confirm=\"Unblock this wallet?\">Unblock wallet</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/worlds/blocked-list\" data-fields=\"\">List blocked</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/worlds/access-log\" data-result=\"#worlds-log-out\">");
        inner.push_str("<label>Access log<span class=\"hint\">recent world access events</span></label>");
        inner.push_str("<div class=\"btnrow\"><input name=\"world\" placeholder=\"world (optional)\"><input name=\"address\" placeholder=\"address (optional)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">View access log</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"worlds-log-out\"></pre></form>");
        b.push_str(&ctl_card_raw("Worlds", "Enable/disable worlds, manage the blocklist, and inspect ban status & access logs.", &inner));
    }

    // ── AB registry (create bundle) ──
    if want("create") && has_service("create") && env_set_any(&["API_ADMIN_TOKEN", "AB_REGISTRY_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field("Retry queues", "/admin/api/create/queues-retry", "Retry failed AB build jobs?", "", false));
        inner.push_str(&btn_field("Pause queues", "/admin/api/create/queues-pause", "Pause AB build queues?", "", false));
        inner.push_str(&btn_field("Resume queues", "/admin/api/create/queues-resume", "Resume AB build queues?", "", false));
        inner.push_str(&btn_field("Queue status", "/admin/api/create/queues-status", "", "", false));
        inner.push_str("</div><div class=\"ctl-result\"></div>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/create/denylist-add\" data-confirm=\"Add this entity to the AB denylist?\">");
        inner.push_str("<label>AB denylist<input name=\"entity_id\" placeholder=\"entity id\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Denylist add</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/create/denylist-remove\" data-fields=\"entity_id\" data-confirm=\"Remove this entity from the AB denylist?\">Denylist remove</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("AB registry", "Control asset-bundle build queues and the registry denylist.", &inner));
    }

    // ── Camera reel (create bundle) ──
    if want("create") && has_service("create") && env_set_any(&["CATALYRST_CAMERA_REEL_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/camera-reel/image-delete\" data-confirm=\"Delete this image?\">");
        inner.push_str("<label>Image<input name=\"image_id\" placeholder=\"image id\" required></label>");
        inner.push_str("<label>Review action<input name=\"action\" placeholder=\"approve / reject (for review)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/camera-reel/image-review\" data-fields=\"image_id,action\" data-confirm=\"Review this image?\">Review image</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Delete image</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Camera reel", "Moderate in-world photos: review or delete an image.", &inner));
    }

    // ── Builder (create bundle) ──
    if want("create") && has_service("create") && env_set_any(&["CATALYRST_BUILDER_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/builder/item-status\" data-confirm=\"Set this item's status?\">");
        inner.push_str("<label>Item status<input name=\"collection_id\" placeholder=\"collection id\" required></label>");
        inner.push_str("<input name=\"item_id\" placeholder=\"item id\"><input name=\"status\" placeholder=\"status\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set item status</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/builder/items-status\" data-fields=\"collection_id,status\" data-confirm=\"Bulk-set every item's status in this collection?\">Bulk set status</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Builder", "Approve or reject collection items (single or bulk).", &inner));
    }

    // ── Communities (social bundle) ──
    if want("social") && has_service("social") && env_set_any(&["API_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/communities/list\" data-result=\"#communities-out\">");
        inner.push_str("<label>Communities<span class=\"hint\">list & filter</span></label>");
        inner.push_str("<div class=\"btnrow\"><input name=\"status\" placeholder=\"status\"><input name=\"owner\" placeholder=\"owner\"><input name=\"search\" placeholder=\"search\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">List communities</button></div>");
        inner.push_str("<pre class=\"ctl-result sqlout\" id=\"communities-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/communities/suspend\" data-confirm=\"Suspend this community?\">");
        inner.push_str("<label>Suspend / unsuspend<input name=\"id\" placeholder=\"community id\" required></label>");
        inner.push_str("<label>Reason<input name=\"reason\" placeholder=\"reason (optional)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Suspend</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/communities/unsuspend\" data-fields=\"id\" data-confirm=\"Unsuspend this community?\">Unsuspend</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Communities", "List, suspend, and reinstate communities.", &inner));
    }

    // ── Notifications (social bundle) ──
    if want("social") && has_service("social") && env_set_any(&["CATALYRST_NOTIFICATIONS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/notifications/broadcast\" data-confirm=\"Broadcast this notification to all users?\">");
        inner.push_str("<label>Title<input name=\"title\" placeholder=\"notification title\" required></label>");
        inner.push_str("<label>Body<textarea name=\"body\" rows=\"2\" placeholder=\"message body\"></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Broadcast</button><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Notifications", "Broadcast a notification to every user.", &inner));
    }

    // ── Badges (social bundle) ──
    if want("social") && has_service("social") && env_set_any(&["CATALYRST_BADGES_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/badges/grant\" data-confirm=\"Grant this badge?\">");
        inner.push_str("<label>Grant / revoke badge<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str("<label>Badge ID<input name=\"badge_id\" placeholder=\"badge id\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Grant badge</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/badges/revoke\" data-fields=\"address,badge_id\" data-confirm=\"Revoke this badge?\">Revoke badge</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Badges", "Grant or revoke a profile badge for a user.", &inner));
    }

    // ── Social RPC ──
    if want("social-rpc") && has_service("social-rpc") && env_set_any(&["CATALYRST_SOCIAL_RPC_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social-rpc/presence\" data-result=\"#srpc-out\">");
        inner.push_str("<label>Inspect<span class=\"hint\">presence / voice / friendships</span></label>");
        inner.push_str("<input name=\"address\" placeholder=\"address (for friendships)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Presence</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/voice-calls\" data-fields=\"limit\">Voice calls</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/friendships\" data-fields=\"address,limit\">Friendships</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"srpc-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/social-rpc/disconnect\" data-confirm=\"Force-disconnect this address?\">");
        inner.push_str("<label>Operate<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str("<input name=\"presence\" placeholder=\"presence (for force-presence)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Disconnect</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/force-presence\" data-fields=\"address,presence\" data-confirm=\"Force this address's presence?\">Force presence</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/social-rpc/reset-settings\" data-fields=\"address\" data-confirm=\"Reset this address's social settings?\">Reset settings</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Social RPC", "Inspect presence/voice/friendships and force-disconnect or reset a user's social session.", &inner));
    }

    // ── Scene state mutations (scene-state bundle, new gated proxies) ──
    if want("scene-state") && has_service("scene-state") && env_set_any(&["CATALYRST_SCENE_STATE_ADMIN_TOKEN", "DEBUGGING_SECRET"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/scene-state/crdt\" data-result=\"#scenestate-out\">");
        inner.push_str("<label>Scene<input name=\"scene\" placeholder=\"scene id\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Inspect CRDT</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/scene-state/kick-all\" data-fields=\"scene\" data-confirm=\"Kick everyone from this scene?\">Kick all</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/scene-state/reset\" data-fields=\"scene\" data-confirm=\"Reset this scene's authoritative state? This discards all current CRDT data.\">Reset scene</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"scenestate-out\"></pre></form>");
        b.push_str(&ctl_card_raw("Scene state (authoritative)", "Inspect the CRDT, kick all peers, or reset a scene's authoritative state.", &inner));
    }

    // ── Credits (data bundle) — HIGH-RISK financial ──
    if want("data") && has_service("data") && env_set_any(&["CATALYRST_CREDITS_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        // Seasons CRUD
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/credits/season-create\" data-confirm=\"Create / update this credits season?\">");
        inner.push_str("<label>Seasons<span class=\"hint\">credits program seasons</span></label>");
        inner.push_str("<input name=\"id\" placeholder=\"id (for update/delete)\"><input name=\"name\" placeholder=\"name\"><input name=\"start_at\" placeholder=\"start_at\"><input name=\"end_at\" placeholder=\"end_at\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/seasons-list\" data-fields=\"\">List seasons</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create season</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/season-update\" data-fields=\"id,name,start_at,end_at\" data-confirm=\"Update this season?\">Update season</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/season-delete\" data-fields=\"id\" data-confirm=\"Delete this season? This is irreversible.\">Delete season</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // Goals CRUD
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/credits/goal-create\" data-confirm=\"Create / update this goal?\">");
        inner.push_str("<label>Goals<span class=\"hint\">weekly credit goals</span></label>");
        inner.push_str("<input name=\"id\" placeholder=\"id (for update/delete)\"><input name=\"weekId\" placeholder=\"weekId (for list)\"><input name=\"description\" placeholder=\"description\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/goals-list\" data-fields=\"weekId\">List goals</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Create goal</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/credits/goal-update\" data-fields=\"id,description\" data-confirm=\"Update this goal?\">Update goal</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/goal-delete\" data-fields=\"id\" data-confirm=\"Delete this goal? This is irreversible.\">Delete goal</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // Grant / revoke / block — financial, distinct styling + strong confirm
        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/credits/grant\" data-confirm=\"GRANT credits to this address? This mints real spendable Marketplace Credits. Confirm the address and amount are correct.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Credits grant / revoke (financial)<input name=\"address\" placeholder=\"0x…\" required></label>");
        inner.push_str("<input name=\"amount\" placeholder=\"amount\"><input name=\"reason\" placeholder=\"reason (audited)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Grant credits</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/revoke\" data-fields=\"address,amount,reason\" data-confirm=\"REVOKE credits from this address? This removes spendable Marketplace Credits from a real user.\">Revoke credits</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/credits/user-block\" data-fields=\"address,reason\" data-confirm=\"BLOCK this address from the credits program?\">Block user</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Credits", "Manage credit seasons & goals, and grant/revoke/block credits for users.", &inner));
    }

    // ── Price (data bundle) — HIGH-RISK financial override ──
    if want("data") && has_service("data") && env_set_any(&["CATALYRST_PRICE_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/price/override-set\" data-confirm=\"SET a manual price override? This replaces the live market price feed for this pair and affects every price-quoting surface.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Price override (financial)<input name=\"token\" placeholder=\"token e.g. mana\" required></label>");
        inner.push_str("<input name=\"vs\" placeholder=\"vs e.g. usd\" required><input name=\"price\" placeholder=\"price\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Set override</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/price/override-delete\" data-fields=\"token,vs\" data-confirm=\"Delete this price override and restore the live feed?\">Delete override</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Price overrides", "Manually override a token's spot price (replaces the live feed).", &inner));
    }

    // ── EVM RPC (data bundle) ──
    if want("data") && has_service("data") && env_set_any(&["CATALYRST_RPC_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field("Config", "/admin/api/rpc/config", "", "#rpc-out", false));
        inner.push_str(&btn_field("List methods", "/admin/api/rpc/methods-list", "", "#rpc-out", false));
        inner.push_str(&btn_field("List networks", "/admin/api/rpc/networks-list", "", "#rpc-out", false));
        inner.push_str(&btn_field("Reset methods", "/admin/api/rpc/methods-reset", "Reset the RPC method allowlist to defaults?", "#rpc-out", false));
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"rpc-out\"></pre>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/rpc/methods-add\" data-confirm=\"Add this method to the allowlist?\">");
        inner.push_str("<label>Method allowlist<input name=\"method\" placeholder=\"eth_…\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Add method</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/rpc/methods-remove\" data-fields=\"method\" data-confirm=\"Remove this method from the allowlist?\">Remove method</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/rpc/networks-set\" data-confirm=\"Set this network's RPC upstream?\">");
        inner.push_str("<label>Networks<input name=\"network\" placeholder=\"mainnet / sepolia / …\" required></label>");
        inner.push_str("<input name=\"url\" placeholder=\"upstream url (for set)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set network</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/rpc/networks-delete\" data-fields=\"network\" data-confirm=\"Delete this network's RPC config?\">Delete network</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("EVM RPC", "Inspect config and manage the method allowlist & network upstreams of the JSON-RPC relay.", &inner));
    }

    // ── Explorer API ──
    if want("explorer-api") && has_service("explorer-api") && env_set_any(&["CATALYRST_EXPLORER_API_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        // Feature flags
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/flags-toggle\" data-confirm=\"Toggle this feature flag?\">");
        inner.push_str("<label>Feature flags<input name=\"flag\" placeholder=\"flag name\" required></label>");
        inner.push_str("<input name=\"enabled\" placeholder=\"enabled true/false\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Toggle flag</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/flags-reload\" data-fields=\"\" data-confirm=\"Reload feature flags?\">Reload flags</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // Blocklist
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/blocklist-add\" data-confirm=\"Add to the blocklist?\">");
        inner.push_str("<label>Blocklist<input name=\"value\" placeholder=\"address / id\" required></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn\" type=\"submit\">Block</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/blocklist-remove\" data-fields=\"value\" data-confirm=\"Remove from the blocklist?\">Unblock</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/blocklist-reload\" data-fields=\"\" data-confirm=\"Reload the blocklist?\">Reload</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        // Config KV
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/config-set\" data-confirm=\"Set this config value?\">");
        inner.push_str("<label>Config<input name=\"key\" placeholder=\"key\" required></label>");
        inner.push_str("<input name=\"value\" placeholder=\"value (for set)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-list\" data-fields=\"\" data-result=\"#exapi-cfg-out\">List config</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-get\" data-fields=\"key\" data-result=\"#exapi-cfg-out\">Get config</button>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set config</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/config-delete\" data-fields=\"key\" data-confirm=\"Delete this config key?\">Delete config</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"exapi-cfg-out\"></pre></form>");
        // Auth challenges & identities
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/explorer-api/challenges-list\" data-result=\"#exapi-auth-out\">");
        inner.push_str("<label>Auth challenges / identities<input name=\"id\" placeholder=\"id (for get/revoke)\"></label>");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"submit\">List challenges</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/challenge-get\" data-fields=\"id\" data-result=\"#exapi-auth-out\">Get challenge</button>");
        inner.push_str("<button class=\"btn\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/challenge-revoke\" data-fields=\"id\" data-confirm=\"Revoke this challenge?\">Revoke challenge</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/identities-list\" data-fields=\"\" data-result=\"#exapi-auth-out\">List identities</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/explorer-api/identity-revoke\" data-fields=\"id\" data-confirm=\"Revoke this identity?\">Revoke identity</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"exapi-auth-out\"></pre></form>");
        b.push_str(&ctl_card_raw("Explorer API", "Feature flags, blocklist, runtime config and auth challenges/identities.", &inner));
    }

    // ── Telemetry (new gated admin proxies, distinct from issue-state/sql above) ──
    if want("telemetry") && has_service("telemetry") && env_set_any(&["CATALYRST_TELEMETRY_ADMIN_TOKEN"]) {
        let mut inner = String::new();
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str(&btn_field("Regroup", "/admin/api/telemetry/regroup", "Recompute issue grouping?", "", false));
        inner.push_str(&btn_field("Release", "/admin/api/telemetry/release", "Mark a release event?", "", false));
        inner.push_str("</div><div class=\"ctl-result\"></div>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/quota\" data-confirm=\"Set the telemetry ingest quota?\">");
        inner.push_str("<label>Quota<input name=\"limit\" placeholder=\"events/min\" required></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Set quota</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/ingest\" data-confirm=\"Inject a synthetic ingest event?\">");
        inner.push_str("<label>Manual ingest<textarea name=\"payload\" rows=\"2\" placeholder=\"event JSON\"></textarea></label>");
        inner.push_str("<button class=\"btn\" type=\"submit\">Ingest</button><div class=\"ctl-result\"></div></form>");
        inner.push_str("<form class=\"ctlform\" data-admin-action=\"/admin/api/telemetry/export\" data-result=\"#tel-export-out\">");
        inner.push_str("<label>Export / audit<input name=\"fingerprint\" placeholder=\"fingerprint (for audit)\"></label>");
        inner.push_str("<input name=\"action\" placeholder=\"action (for audit)\"><input name=\"limit\" placeholder=\"limit\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn ghost\" type=\"submit\">Export</button>");
        inner.push_str("<button class=\"btn ghost\" type=\"button\" data-admin-action=\"/admin/api/telemetry/audit\" data-fields=\"fingerprint,action,limit\" data-result=\"#tel-export-out\">Audit log</button>");
        inner.push_str("</div><pre class=\"ctl-result sqlout\" id=\"tel-export-out\"></pre></form>");
        inner.push_str("<form class=\"ctlform danger-form\" data-admin-action=\"/admin/api/telemetry/purge\" data-confirm=\"PURGE telemetry data? This permanently deletes stored events.\">");
        inner.push_str("<label class=\"danger-lab\">⚠ Destructive<input name=\"before\" placeholder=\"before ISO date (purge)\"></label>");
        inner.push_str("<input name=\"fingerprint\" placeholder=\"fingerprint (bulk-delete)\">");
        inner.push_str("<div class=\"btnrow\">");
        inner.push_str("<button class=\"btn danger\" type=\"submit\">Purge</button>");
        inner.push_str("<button class=\"btn danger\" type=\"button\" data-admin-action=\"/admin/api/telemetry/bulk-delete\" data-fields=\"fingerprint\" data-confirm=\"BULK-DELETE every event for this fingerprint? This is irreversible.\">Bulk delete</button>");
        inner.push_str("</div><div class=\"ctl-result\"></div></form>");
        b.push_str(&ctl_card_raw("Telemetry operations", "Quota, manual ingest, regroup/release, export/audit, and destructive purge/bulk-delete.", &inner));
    }

    b.push_str("</section>");
    b
}

/// A confirm-guarded button that POSTs an empty body to `action` and (optionally)
/// renders into `result` (a selector). Used for the multi-button rows where each
/// button hits a different no-body endpoint. `danger` applies the destructive
/// styling. Empty `confirm` skips the confirm() prompt (for read-only fetches).
fn btn_field(label: &str, action: &str, confirm: &str, result: &str, danger: bool) -> String {
    let cls = if danger { "btn danger" } else { "btn ghost" };
    let conf = if confirm.is_empty() { String::new() } else { format!(" data-confirm=\"{}\"", esc(confirm)) };
    let res = if result.is_empty() { String::new() } else { format!(" data-result=\"{}\"", esc(result)) };
    format!(
        "<button class=\"{}\" type=\"button\" data-admin-action=\"{}\" data-fields=\"\"{}{}>{}</button>",
        cls,
        esc(action),
        conf,
        res,
        esc(label)
    )
}

/// A control card whose body is a row of simple action buttons.
fn ctl_card(title: &str, desc: &str, buttons: &[String]) -> String {
    let mut inner = String::from("<div class=\"btnrow\">");
    for btn in buttons {
        inner.push_str(btn);
    }
    inner.push_str("</div><div class=\"ctl-result\"></div>");
    ctl_card_raw(title, desc, &inner)
}

/// A control card with a pre-rendered inner body (forms, etc.).
fn ctl_card_raw(title: &str, desc: &str, inner: &str) -> String {
    format!(
        "<div class=\"ctlcard\"><div class=\"ctlh\">{}</div><div class=\"ctlbody\"><p class=\"ctldesc\">{}</p>{}</div></div>",
        esc(title),
        esc(desc),
        inner
    )
}

/// A single confirm-guarded action button that POSTs an empty body.
fn ctl_button(label: &str, action: &str, confirm: &str) -> String {
    format!(
        "<button class=\"btn\" data-admin-action=\"{}\" data-confirm=\"{}\">{}</button>",
        esc(action),
        esc(confirm),
        esc(label)
    )
}

/// The "admin not configured" read-only note (shown in place of controls when
/// `ADMIN_ADDRESSES`/`SESSION_SECRET` are unset). Default-safe.
fn admin_disabled_note() -> &'static str {
    "<div class=\"note\">Admin write controls are disabled — set ADMIN_ADDRESSES + SESSION_SECRET to enable them. This page is read-only.</div>"
}

// ───────────────────────── landing (GET /) ─────────────────────────
pub async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let p = probe().await;

    // per-service health roll-up across every configured bundle
    let (mut configured, mut up) = (0usize, 0usize);
    for g in CATALOG {
        for s in g.services {
            if let Some(ok) = svc_health(g, s, content_healthy, &p) {
                configured += 1;
                if ok {
                    up += 1;
                }
            }
        }
    }

    let realm = state.realm_name.clone().unwrap_or_else(|| "catalyrst realm".to_string());
    let (status_cls, status_txt) = if content_healthy { ("ok", "Online") } else { ("bad", "Degraded") };

    let mut b = String::new();
    b.push_str("<div class=\"hero\"><div class=\"wrap\">");
    b.push_str("<h2>A self-hosted <em>Decentraland</em> realm.</h2>");
    b.push_str("<p>catalyrst is a from-scratch Rust implementation of the Decentraland service plane — content &amp; lambdas, the explorer APIs, the social stack, the creator and marketplace planes, scene-state multiplayer and a federation layer. Everything an explorer talks to, from one workspace.</p>");
    if let Some(base) = realm_base_url(&state) {
        let play = format!("https://decentraland.org/play/?realm={}", urlencoding::encode(&base));
        b.push_str(&format!(
            "<div><a class=\"cta\" href=\"{}\" rel=\"noopener\">Open in Decentraland →</a><a class=\"cta ghost\" href=\"/about\">View realm API</a></div>",
            esc(&play)
        ));
    }
    b.push_str("<div class=\"statusbar\">");
    b.push_str(&stat("realm status", &format!("<span class=\"pill\"><span class=\"dot {status_cls}\"></span>{status_txt}</span>"), true, true));
    b.push_str(&stat("realm", &esc(&realm), true, false));
    b.push_str(&stat("network", &esc(network_name(&state.eth_network)), true, false));
    if p.activity.users.is_some() {
        b.push_str(&stat("users online", &opt_big(p.activity.users), false, false));
    }
    b.push_str(&stat("content sync", &esc(&sync_state), true, false));
    b.push_str(&stat("services healthy", &format!("{up}<span style=\"color:var(--mut2);font-size:18px\"> / {configured}</span>"), false, false));
    b.push_str("</div></div></div>");

    b.push_str("<main><div class=\"wrap\">");

    // live activity strip (only when at least one signal is present)
    let a = &p.activity;
    if a.users.is_some() || a.peers.is_some() || a.hot_scenes.is_some() || a.ss_connections.is_some() {
        b.push_str("<section><div class=\"shead\"><h3>Live activity</h3><span class=\"c\">across the realm right now</span></div><div class=\"statusbar\">");
        b.push_str(&stat("users online", &opt_big(a.users), false, false));
        b.push_str(&stat("peers", &opt_big(a.peers), false, false));
        b.push_str(&stat("islands", &opt_big(a.islands), false, false));
        b.push_str(&stat("hot scenes", &opt_big(a.hot_scenes), false, false));
        b.push_str(&stat("scene connections", &opt_big(a.ss_connections), false, false));
        b.push_str("</div></section>");
    }

    // service plane
    b.push_str(&format!(
        "<section><div class=\"shead\"><h3>Service plane</h3><span class=\"c\">{} bundles · {} services</span></div><div class=\"groups\">",
        CATALOG.len(),
        CATALOG.iter().map(|g| g.services.len()).sum::<usize>()
    ));
    for g in CATALOG {
        let (dc, dt) = dot(group_health(g, content_healthy, &p));
        b.push_str("<div class=\"group\"><div class=\"gh\">");
        b.push_str(&format!("<span class=\"dot {dc}\" title=\"{dt}\"></span>"));
        b.push_str(&format!("<a class=\"gt\" href=\"/admin/{}\">{}</a>", esc(g.key), esc(g.title)));
        b.push_str(&format!("<span class=\"gp\">{}</span></div>", esc(g.bundle)));
        for s in g.services {
            let (sc, stt) = dot(svc_health(g, s, content_healthy, &p));
            b.push_str(&format!("<div class=\"svc\"><span class=\"dot {sc}\" title=\"{stt}\"></span><div class=\"sd\">"));
            b.push_str(&format!("<div class=\"sn\">{}<span class=\"ref\">{}</span></div>", esc(s.name), esc(s.reference)));
            b.push_str(&format!("<div class=\"sdesc\">{}</div>", esc(s.desc)));
            if s.path.is_empty() {
                b.push_str("<span class=\"spath no\">internal</span>");
            } else {
                b.push_str(&format!("<a class=\"spath\" href=\"{0}\">{0}</a>", esc(s.path)));
            }
            b.push_str("</div></div>");
        }
        b.push_str("</div>");
    }
    b.push_str("</div></section>");

    // quick links — all same-origin so they resolve for anyone the URL is shared with
    b.push_str("<section><div class=\"shead\"><h3>Quick links</h3></div><div class=\"links\">");
    let links = [
        ("/about", "Realm descriptor", "What explorer clients fetch to join this realm", "/about"),
        ("/places", "Places", "Discover places & scenes in this realm", "/places"),
        ("/v1/map.png", "Map", "Genesis-city map render", "/v1/map.png"),
        ("/admin", "Admin console", "Live cross-service status (loopback / tailnet only)", "/admin"),
    ];
    for (href, t, d, u) in links {
        b.push_str(&format!(
            "<a class=\"link\" href=\"{}\"><div class=\"lt\">{}</div><div class=\"ld\">{}</div><div class=\"lu\">{}</div></a>",
            esc(href), esc(t), esc(d), esc(u)
        ));
    }
    b.push_str("</div></section>");
    b.push_str("</div></main>");

    page(&state, &format!("catalyrst — {realm}"), "overview", &b)
}

// ───────────────────────── admin overview (GET /admin) ─────────────────────────
pub async fn admin(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Html<String> {
    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let cluster = state.content_cluster.get_status();
    let failed = state.database.get_failed_deployments().await;
    let p = probe().await;
    let a = &p.activity;
    let va = viewer_admin(&headers);

    let mut b = String::new();
    b.push_str("<main><div class=\"wrap\">");

    // operator controls (or sign-in / disabled note) — rendered first so the
    // primary admin actions are above the fold.
    if va.enabled {
        b.push_str(&controls(&va, "all"));
    } else {
        b.push_str("<section><div class=\"shead\"><h3>Operator controls</h3></div>");
        b.push_str(admin_disabled_note());
        b.push_str("</section>");
    }

    // live activity
    b.push_str("<section><div class=\"shead\"><h3>Live activity</h3><span class=\"c\">real-time across services</span></div><div class=\"statusbar\">");
    b.push_str(&stat("users online", &opt_big(a.users), false, true));
    b.push_str(&stat("peers", &opt_big(a.peers), false, false));
    b.push_str(&stat("islands", &opt_big(a.islands), false, false));
    b.push_str(&stat("hot scenes", &opt_big(a.hot_scenes), false, false));
    b.push_str(&stat("scene conns", &opt_big(a.ss_connections), false, false));
    b.push_str(&stat("loaded scenes", &opt_big(a.ss_scenes), false, false));
    b.push_str(&stat("AB build queue", &opt_big(a.ab_queue), false, false));
    b.push_str(&stat("comms uptime", &a.uptime_secs.map(fmt_uptime).unwrap_or_else(|| "—".into()), true, false));
    b.push_str("</div></section>");

    // realm config
    b.push_str("<section><div class=\"shead\"><h3>Realm</h3></div><div class=\"grid\">");
    let kvs: [(&str, String); 8] = [
        ("realm name", state.realm_name.clone().unwrap_or_else(|| "—".into())),
        ("eth network", state.eth_network.clone()),
        ("mode", mode_str(state.is_read_only()).to_string()),
        ("content version", state.content_version.clone()),
        ("lambdas version", state.lambdas_version.clone()),
        ("commit", state.commit_hash.clone()),
        ("content url", state.content_public_url.clone()),
        ("lambdas url", state.lambdas_public_url.clone()),
    ];
    for (k, v) in kvs {
        b.push_str(&format!(
            "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
            esc(k),
            esc(if v.is_empty() { "—" } else { &v })
        ));
    }
    b.push_str("</div></section>");

    // synchronization
    let (sc, st) = if content_healthy { ("ok-t", "healthy") } else { ("bad-t", "degraded") };
    b.push_str(&format!(
        "<section><div class=\"shead\"><h3>Synchronization</h3><span class=\"c {sc}\">{st}</span></div><div class=\"grid\">"
    ));
    b.push_str(&format!(
        "<div class=\"kv\"><div class=\"k\">state</div><div class=\"v\">{}</div></div>",
        esc(&sync_state)
    ));
    if let Value::Object(map) = &cluster {
        for (k, v) in map {
            let vs = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            b.push_str(&format!(
                "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
                esc(k),
                esc(&vs)
            ));
        }
    }
    b.push_str("</div></section>");

    // deployments
    b.push_str("<section><div class=\"shead\"><h3>Deployments</h3></div><div class=\"grid\">");
    let (failed_txt, failed_cls) = match &failed {
        Ok(v) if v.is_empty() => ("0".to_string(), "ok-t"),
        Ok(v) => (human(v.len() as u64), "warn-t"),
        Err(_) => ("unavailable".to_string(), "bad-t"),
    };
    b.push_str(&format!(
        "<div class=\"kv\"><div class=\"k\">failed deployments</div><div class=\"v {failed_cls}\">{failed_txt}</div></div>"
    ));
    b.push_str("</div></section>");

    // cross-service status table — each row deep-links to its detail page
    b.push_str("<section><div class=\"shead\"><h3>Service health</h3><span class=\"c\">click a bundle for live detail</span></div>");
    b.push_str("<table><thead><tr><th>Bundle</th><th>Service group</th><th>Members</th><th>Health</th><th>Probe</th></tr></thead><tbody>");
    let urls = service_urls();
    for g in CATALOG {
        let h = group_health(g, content_healthy, &p);
        let (dc, dt) = dot(h);
        let members = if g.multi {
            p.groups.get(g.key).map(|gh| {
                let total = gh.members.len();
                let up = gh.members.values().filter(|v| **v).count();
                format!("{up}/{total} up")
            }).unwrap_or_else(|| "—".into())
        } else {
            "—".into()
        };
        let probe = if g.key == "content" {
            "local".to_string()
        } else {
            urls.get(g.key).map(|u| format!("{u}/health")).unwrap_or_else(|| "—".into())
        };
        b.push_str(&format!(
            "<tr><td class=\"mono\"><a href=\"/admin/{}\">{}</a></td><td>{}</td><td class=\"mono\">{}</td><td><span class=\"pill\"><span class=\"dot {dc}\"></span>{dt}</span></td><td class=\"mono\">{}</td></tr>",
            esc(g.key),
            esc(g.bundle),
            esc(g.title),
            esc(&members),
            esc(&probe)
        ));
    }
    b.push_str("</tbody></table>");
    b.push_str("<div class=\"note\">/admin is not exposed on the public edge. Reach it on the loopback port or over the tailnet, and front it with auth before any public exposure.</div>");
    b.push_str("</section>");

    b.push_str("</div></main>");
    page(&state, "catalyrst — admin", "admin", &b)
}

// ───────────────────────── admin detail (GET /admin/{service}) ─────────────────────────
pub async fn admin_service(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let va = viewer_admin(&headers);
    let Some(g) = group_by_key(&key) else {
        let body = "<main><div class=\"wrap\"><a class=\"back\" href=\"/admin\">← back to admin</a><div class=\"empty\" style=\"padding:60px;color:var(--mut2)\">no such service</div></div></main>";
        return (
            axum::http::StatusCode::NOT_FOUND,
            page(&state, "catalyrst — not found", "admin", body),
        )
            .into_response();
    };

    let sync_state = state.synchronization_state.get_state();
    let content_healthy = sync_state == "Syncing";
    let p = probe().await;
    let (dc, dt) = dot(group_health(g, content_healthy, &p));

    let mut b = String::new();
    b.push_str("<main><div class=\"wrap\">");
    b.push_str("<a class=\"back\" href=\"/admin\">← back to admin</a>");
    b.push_str(&format!(
        "<div class=\"shead\"><h3>{}</h3><span class=\"c\">{}</span><span class=\"spacer\" style=\"flex:1\"></span><span class=\"pill\"><span class=\"dot {dc}\"></span>{dt}</span></div>",
        esc(g.title),
        esc(g.bundle)
    ));

    // operator controls scoped to this bundle (sign-in card when unauthed, the
    // bundle's own control card when authed + prerequisites hold). Each control
    // is thus reachable from its bundle's shareable deep link.
    if va.enabled {
        b.push_str(&controls(&va, g.key));
    }

    // member roll-up for multi bundles
    if g.multi {
        b.push_str("<div class=\"grid\">");
        for s in g.services {
            let (mc, mt) = dot(svc_health(g, s, content_healthy, &p));
            b.push_str(&format!(
                "<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\"><span class=\"pill\"><span class=\"dot {mc}\"></span>{mt}</span></div></div>",
                esc(s.name)
            ));
        }
        b.push_str("</div>");
    }

    // content is the local process — render its rich state directly
    if g.key == "content" {
        b.push_str("<div class=\"grid\">");
        let cluster = state.content_cluster.get_status();
        let kvs = [
            ("sync state", sync_state.clone()),
            ("content version", state.content_version.clone()),
            ("lambdas version", state.lambdas_version.clone()),
            ("commit", state.commit_hash.clone()),
            ("network", state.eth_network.clone()),
            ("mode", mode_str(state.is_read_only()).to_string()),
        ];
        for (k, v) in kvs {
            b.push_str(&format!("<div class=\"kv\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>", esc(k), esc(&v)));
        }
        b.push_str("</div>");
        b.push_str(&format!("<div class=\"raw\"><div class=\"rt\">cluster status</div><pre>{}</pre></div>", esc(&serde_json::to_string_pretty(&cluster).unwrap_or_default())));
    } else if let Some(base) = service_urls().get(g.key) {
        // live endpoint links (clickable on the host)
        b.push_str("<div class=\"eplist\">");
        for (label, path) in g.detail {
            b.push_str(&format!("<a class=\"ep\" href=\"{0}{1}\" rel=\"noopener\">{2} · {1}</a>", esc(base), esc(path), esc(label)));
        }
        b.push_str("</div>");
        // fetch + pretty-print each introspection endpoint
        let futs = g.detail.iter().map(|(label, path)| async move {
            let body = match fetch_json(&format!("{base}{path}")).await {
                Some(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
                None => match probe_up(&format!("{base}{path}")).await {
                    true => "(200 OK — non-JSON body)".into(),
                    false => "(unreachable)".into(),
                },
            };
            (*label, *path, body)
        });
        for (label, path, body) in futures::future::join_all(futs).await {
            b.push_str(&format!(
                "<div class=\"raw\"><div class=\"rt\">{} <span style=\"color:var(--mut2)\">{}</span></div><pre>{}</pre></div>",
                esc(label),
                esc(path),
                esc(&body)
            ));
        }
    } else {
        b.push_str("<div class=\"note\">This bundle has no configured URL in CATALYRST_SERVICE_URLS, so its live status can't be probed. Add a `");
        b.push_str(&esc(g.key));
        b.push_str("=http://host:port` entry to enable it.</div>");
    }

    b.push_str("</div></main>");
    page(&state, &format!("catalyrst — {}", g.title), "admin", &b).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_placeholders_are_all_filled() {
        const FILLED: &[&str] = &[
            "title", "realmname", "nav", "version", "commit", "network", "mode", "now", "main",
        ];
        let mut rest = TEMPLATE;
        while let Some(i) = rest.find("<!--SSR:") {
            let after = &rest[i + "<!--SSR:".len()..];
            let end = after.find("-->").expect("unterminated SSR placeholder");
            let key = &after[..end];
            assert!(FILLED.contains(&key), "template has unfilled placeholder: {key}");
            rest = &after[end + 3..];
        }
    }

    #[test]
    fn catalog_is_well_formed() {
        assert_eq!(CATALOG[0].key, "content", "content group must be first");
        for g in CATALOG {
            assert!(!g.title.is_empty() && !g.bundle.is_empty());
            assert!(!g.services.is_empty(), "{} has no services", g.key);
            // multi bundles must give every service a member key (for /health lookup)
            if g.multi {
                for s in g.services {
                    assert!(!s.member.is_empty(), "{}/{} missing member key", g.key, s.name);
                }
            }
        }
        // every detail slug is unique
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
        assert_eq!(num(&serde_json::json!({"userCount": 12}), "userCount"), Some(12));
        assert_eq!(num(&serde_json::json!({"a": "x"}), "a"), None);
    }
}
