//! Runtime configuration. Mirrors the env vars read by the upstream
//! `createDotEnvConfigComponent` flow plus our `HTTP_SERVER_*` convention.

use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    /// Local path to a compiled scene `game.js`. Upstream `LOCAL_SCENE_PATH`.
    /// When set, the scene is loaded under the name `localScene` at startup.
    pub local_scene_path: Option<String>,

    /// World content-server base URL. Upstream `WORLD_SERVER_URL`. Used by
    /// `/debugging/reload` to pull a deployed world's scene.
    pub world_server_url: Option<String>,

    /// Shared secret guarding `/debugging/reload`. Upstream `DEBUGGING_SECRET`.
    pub debugging_secret: Option<String>,

    /// Bearer token gating the admin control routes (`/admin/scene/*`):
    /// kick-all, CRDT inspect, reset-state. Compared in constant time against
    /// `Authorization: Bearer <token>`. Sourced from
    /// `CATALYRST_SCENE_STATE_ADMIN_TOKEN`, falling back to `DEBUGGING_SECRET`
    /// so a single-host deploy that already armed the scene-control secret need
    /// not set a second value. Unset (both) ⇒ every admin route returns 403
    /// (fail-closed / default-safe).
    pub admin_token: Option<String>,

    /// Base URL the server signs auth payloads against. Upstream `HTTP_BASE_URL`.
    pub http_base_url: Option<String>,

    /// Seconds a client has to send its Auth frame before being dropped.
    /// Upstream hardcodes 5s (`authTimeout`).
    pub auth_timeout_secs: u64,

    /// When true, scenes load with the scene-logic-free [`RelayRuntime`] instead
    /// of executing their `game.js` in V8. `DISABLE_JS_RUNTIME=1`.
    ///
    /// [`RelayRuntime`]: crate::runtime::RelayRuntime
    pub disable_js_runtime: bool,

    /// Realm name reported to scenes via `~system/Runtime.getRealm()`.
    /// `REALM_NAME` (defaults to `dcl-one`).
    pub realm_name: Option<String>,

    pub commit_hash: String,

    // --- Per-scene V8 sandbox safety limits ---
    /// Hard V8 heap cap, in MiB, for each scene isolate. When the heap nears
    /// this limit V8 invokes the near-heap-limit callback, which terminates the
    /// scene's execution (rather than letting V8 abort the whole process).
    /// `JS_HEAP_LIMIT_MB` (default 384).
    pub js_heap_limit_mb: usize,

    /// Wall-clock budget, in milliseconds, for a single `onStart`/`onUpdate`
    /// tick. A watchdog thread terminates the isolate's execution if one tick
    /// runs longer (catches infinite loops in scene JS). `JS_TICK_BUDGET_MS`
    /// (default 250).
    pub js_tick_budget_ms: u64,

    /// How long `shutdown()` waits for the JS thread to unwind after asking it
    /// to stop, in milliseconds, before giving up the join (so a wedged scene
    /// can't block `/debugging/reload` forever). `JS_SHUTDOWN_JOIN_MS`
    /// (default 2000).
    pub js_shutdown_join_ms: u64,

    // --- Per-client / per-scene backpressure caps ---
    /// Max queued outbound frames per client before the slow client is
    /// disconnected (its outbound sink is bounded). `CLIENT_OUTBOUND_MAX`
    /// (default 1024).
    pub client_outbound_max: usize,

    /// Max queued inbound CRDT batches per client awaiting the scene's
    /// `getMessages()` pull. Beyond this the client is dropped.
    /// `CLIENT_INBOUND_MAX` (default 1024).
    pub client_inbound_max: usize,

    /// Max authoritative (entity, component) cells the CRDT engine will hold
    /// per scene; writes beyond the cap are rejected. `CRDT_MAX_COMPONENTS`
    /// (default 100000).
    pub crdt_max_components: usize,

    /// Max WebSocket frame size accepted on `/ws`, in bytes (axum's default is
    /// 64 MiB). `WS_MAX_FRAME_BYTES` (default 2 MiB).
    pub ws_max_frame_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let http_host = env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let http_port: u16 = env::var("HTTP_SERVER_PORT")
            .unwrap_or_else(|_| "5153".into())
            .parse()
            .context("HTTP_SERVER_PORT must be u16")?;

        let opt = |k: &str| env::var(k).ok().filter(|s| !s.is_empty());

        let debugging_secret = opt("DEBUGGING_SECRET");
        // Prefer a dedicated admin bearer; fall back to DEBUGGING_SECRET so the
        // existing scene-control secret also unlocks the admin routes.
        let admin_token =
            opt("CATALYRST_SCENE_STATE_ADMIN_TOKEN").or_else(|| debugging_secret.clone());

        Ok(Self {
            http_host,
            http_port,
            local_scene_path: opt("LOCAL_SCENE_PATH"),
            world_server_url: opt("WORLD_SERVER_URL"),
            debugging_secret,
            admin_token,
            http_base_url: opt("HTTP_BASE_URL"),
            auth_timeout_secs: opt("AUTH_TIMEOUT_SECS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            disable_js_runtime: opt("DISABLE_JS_RUNTIME")
                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            realm_name: opt("REALM_NAME"),
            commit_hash: env::var("COMMIT_HASH").unwrap_or_default(),
            js_heap_limit_mb: parse_or("JS_HEAP_LIMIT_MB", 384),
            js_tick_budget_ms: parse_or("JS_TICK_BUDGET_MS", 250),
            js_shutdown_join_ms: parse_or("JS_SHUTDOWN_JOIN_MS", 2000),
            client_outbound_max: parse_or("CLIENT_OUTBOUND_MAX", 1024),
            client_inbound_max: parse_or("CLIENT_INBOUND_MAX", 1024),
            crdt_max_components: parse_or("CRDT_MAX_COMPONENTS", 100_000),
            ws_max_frame_bytes: parse_or("WS_MAX_FRAME_BYTES", 2 * 1024 * 1024),
        })
    }
}

/// Parse an env var as `T`, falling back to `default` if unset or unparseable.
fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
