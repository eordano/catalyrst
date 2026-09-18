use catalyrst_pulse::cluster::feed::{ClusterFeedPublisher, NoopClusterFeedPublisher};
use catalyrst_pulse::cluster::{
    ClusterOptions, ClusterTracker, DEFAULT_CLUSTERS_ENABLED, DEFAULT_DWELL_PASSES,
    DEFAULT_ID_PREFIX, DEFAULT_PASS_INTERVAL_MS, DEFAULT_SESSION_RETENTION_PASSES,
};
use catalyrst_pulse::hardening::{
    DisconnectReason, GameplayRateLimiter, DEFAULT_DISCRETE_BURST, DEFAULT_DISCRETE_RATE_PER_SEC,
    DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ,
};
use catalyrst_pulse::interest::{SpatialAreaOfInterest, SpatialAreaOfInterestOptions};
use catalyrst_pulse::server::{ENET_CAPACITY, WT_CAPACITY};
use catalyrst_pulse::transport::webtransport::config::{
    DEFAULT_MAX_DATAGRAM_BYTES, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_SERVICE_TIMEOUT_MS,
};
use catalyrst_pulse::transport::webtransport::WtConfig;
use catalyrst_pulse::PulseServer;
use std::env::VarError;
use std::sync::Arc;

const DEFAULT_BIND: &str = "0.0.0.0:9000";
const DEFAULT_WT_BIND: &str = "0.0.0.0:7743";
const DEFAULT_LOG_FILTER: &str = "catalyrst_pulse=info";

const BOOL_TRUE: &[&str] = &["1", "true", "yes", "on"];
const BOOL_FALSE: &[&str] = &["0", "false", "no", "off"];

/// Literal pairs on purpose: the deployment's env-reads check parses this table as the
/// crate's env contract and only reads a static `&[(&str, &str)]`; a table built at runtime is
/// invisible to it, and every read in the crate then fails the gate as undeclared. The defaults
/// quoted here are pinned to the constants by `tests::env_docs_defaults_track_the_constants`, so
/// a lockstep bump of a constant fails the build until the prose follows.
const ENV_DOCS: &[(&str, &str)] = &[
    ("PULSE_BIND", "ENet/UDP bind address (default 0.0.0.0:9000)"),
    (
        "PULSE_METRICS_BIND",
        "prometheus /metrics bind address (default 127.0.0.1:5005)",
    ),
    (
        "PULSE_INPUT_MAX_HZ",
        "gameplay input rate cap in Hz (default 20)",
    ),
    (
        "PULSE_INPUT_BURST",
        "gameplay input burst allowance (default 16)",
    ),
    (
        "PULSE_DISCRETE_RATE_PER_SEC",
        "discrete-action rate cap per second (default 20)",
    ),
    (
        "PULSE_DISCRETE_BURST",
        "discrete-action burst allowance (default 16)",
    ),
    (
        "PULSE_SCENE_LISTENER_MAX_PARCELS",
        "scene-listener AoI budget in parcels: sum of rect areas over every announced realm plus 4 per realm (default 4096)",
    ),
    (
        "PULSE_AOI_TIER0_RADIUS",
        "player AoI tier-0 radius in world units: peers this close get full-detail updates (default 30)",
    ),
    (
        "PULSE_AOI_TIER1_RADIUS",
        "player AoI tier-1 radius in world units: peers between tier-0 and this get reduced-detail updates (default 60)",
    ),
    (
        "PULSE_AOI_MAX_RADIUS",
        "player AoI cutoff in world units: peers between tier-1 and this get position-only updates, beyond it they are invisible; at most 800 (default 200)",
    ),
    (
        "PULSE_WT_ENABLED",
        "enables the WebTransport front door: 1/true/yes/on or 0/false/no/off, case-insensitive, unset or blank is off, anything else fails startup (default off)",
    ),
    (
        "PULSE_WT_BIND",
        "WebTransport bind address (default 0.0.0.0:7743)",
    ),
    (
        "PULSE_WT_CERT_PEM",
        "inline TLS certificate PEM for WebTransport (takes precedence over PULSE_WT_CERT_PATH)",
    ),
    (
        "PULSE_WT_CERT_PATH",
        "path to the WebTransport TLS certificate PEM",
    ),
    (
        "PULSE_WT_KEY_PEM",
        "inline TLS key PEM for WebTransport (takes precedence over PULSE_WT_KEY_PATH)",
    ),
    ("PULSE_WT_KEY_PATH", "path to the WebTransport TLS key PEM"),
    (
        "PULSE_WT_MAX_DATAGRAM_BYTES",
        "max WebTransport datagram size in bytes (default 1200)",
    ),
    (
        "PULSE_WT_MAX_MESSAGE_BYTES",
        "max WebTransport message size in bytes (default 4096)",
    ),
    (
        "PULSE_CLUSTERS_ENABLED",
        "derive peer clusters every pass: 1/true/yes/on or 0/false/no/off, case-insensitive, blank or unset keeps the default, anything else fails startup (default true)",
    ),
    (
        "PULSE_CLUSTERS_PASS_INTERVAL_MS",
        "milliseconds between clustering passes; also the upper bound on how stale a published assignment can be (default 1000)",
    ),
    (
        "PULSE_CLUSTERS_DWELL_PASSES",
        "consecutive passes that must agree on a new assignment before it is published; 1 disables the debounce (default 3)",
    ),
    (
        "PULSE_CLUSTERS_ID_PREFIX",
        "prefix of every minted cluster id, so two Pulse instances on one broker never mint the same one (default C)",
    ),
    (
        "PULSE_CLUSTERS_SESSION_RETENTION_PASSES",
        "passes a departed wallet's last published assignment is retained so a new session can name it as displaced; 0 disables the annotation (default 300)",
    ),
    (
        "PULSE_NATS_URL",
        "broker URL for the cluster feed; blank or unset leaves the tracker in stats-only mode, deriving clusters and reporting metrics while publishing nothing (default unset)",
    ),
    (
        "NATS_URL",
        "fallback broker URL, read only when PULSE_NATS_URL is blank or unset (default unset)",
    ),
    (
        "PULSE_NATS_SERVER_NAME",
        "name this server announces on the engine.discovery heartbeat, and the connection name the broker shows on /connz (default pulse)",
    ),
    (
        "COMMIT_HASH",
        "commit of the deployed build, announced on the engine.discovery heartbeat so consumers can tell two deployments of one version apart (default unknown)",
    ),
    (
        "PULSE_NATS_DISCOVERY_INTERVAL_MS",
        "milliseconds between engine.discovery heartbeats (default 10000)",
    ),
    (
        "PULSE_NATS_CHANNEL_CAPACITY",
        "distinct peers that may hold an undelivered assignment at once; past it the longest-admitted is evicted and counted on pulse_nats_dropped_total (default 1024)",
    ),
    ("RUST_LOG", "tracing filter (default catalyrst_pulse=info)"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-pulse", ENV_DOCS);

    catalyrst_envcfg::init_tracing(DEFAULT_LOG_FILTER);

    let metrics_bind = catalyrst_pulse::metrics::metrics_bind_from_env()?;
    catalyrst_pulse::metrics::install_prometheus_exporter(metrics_bind)?;
    tracing::info!(%metrics_bind, "pulse /metrics listening");
    let bind = std::env::var("PULSE_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()?;
    let wt = webtransport_config_from_env()?;
    let aoi = aoi_options_from_env()?;
    tracing::info!(
        tier0 = aoi.tier0_radius,
        tier1 = aoi.tier1_radius,
        max = aoi.max_radius,
        "player AoI radii"
    );
    let mut server = PulseServer::new();
    server.gameplay_limiter = GameplayRateLimiter::new(
        env_or("PULSE_INPUT_MAX_HZ", DEFAULT_INPUT_MAX_HZ)?,
        env_or("PULSE_INPUT_BURST", DEFAULT_INPUT_BURST)?,
        env_or("PULSE_DISCRETE_RATE_PER_SEC", DEFAULT_DISCRETE_RATE_PER_SEC)?,
        env_or("PULSE_DISCRETE_BURST", DEFAULT_DISCRETE_BURST)?,
    );
    server.aoi = SpatialAreaOfInterest::new(aoi);
    server.clusters = clusters_from_env()?;
    server.run_with_webtransport(bind, 50, wt).await
}

/// Player AoI radii from the environment, upstream's defaults where unset. A radius that fails to
/// parse or an ordering that empties a tier is a startup error, because the fallback would change
/// what every player sees, silently.
fn aoi_options_from_env() -> anyhow::Result<SpatialAreaOfInterestOptions> {
    let defaults = SpatialAreaOfInterestOptions::default();
    let options = SpatialAreaOfInterestOptions {
        tier0_radius: env_or("PULSE_AOI_TIER0_RADIUS", defaults.tier0_radius)?,
        tier1_radius: env_or("PULSE_AOI_TIER1_RADIUS", defaults.tier1_radius)?,
        max_radius: env_or("PULSE_AOI_MAX_RADIUS", defaults.max_radius)?,
    };
    options.validate()?;
    Ok(options)
}

/// Build the WebTransport config from the environment, or `None` when disabled. WebTransport is
/// off unless `PULSE_WT_ENABLED` is truthy; when on, it needs a certificate + key (inline PEM or
/// a file path) -- a browser cannot reach a raw ENet/UDP socket, so this is the browser front door.
fn webtransport_config_from_env() -> anyhow::Result<Option<WtConfig>> {
    if !env_bool("PULSE_WT_ENABLED")? {
        return Ok(None);
    }

    let bind_addr = std::env::var("PULSE_WT_BIND")
        .unwrap_or_else(|_| DEFAULT_WT_BIND.to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("PULSE_WT_BIND: {e}"))?;

    let cert_pem = read_pem("PULSE_WT_CERT_PEM", "PULSE_WT_CERT_PATH")?
        .ok_or_else(|| anyhow::anyhow!("PULSE_WT_ENABLED but no PULSE_WT_CERT_PEM/PATH set"))?;
    let key_pem = read_pem("PULSE_WT_KEY_PEM", "PULSE_WT_KEY_PATH")?
        .ok_or_else(|| anyhow::anyhow!("PULSE_WT_ENABLED but no PULSE_WT_KEY_PEM/PATH set"))?;

    Ok(Some(WtConfig {
        bind_addr,
        cert_pem,
        key_pem,
        slot_base: ENET_CAPACITY as u32,
        slot_capacity: WT_CAPACITY,
        max_datagram_bytes: env_or("PULSE_WT_MAX_DATAGRAM_BYTES", DEFAULT_MAX_DATAGRAM_BYTES)?,
        max_message_bytes: env_or("PULSE_WT_MAX_MESSAGE_BYTES", DEFAULT_MAX_MESSAGE_BYTES)?,
        service_timeout_ms: DEFAULT_SERVICE_TIMEOUT_MS,
        server_full_reason: DisconnectReason::ServerFull.code(),
    }))
}

/// The cluster tracker, or `None` when clustering is off entirely -- which leaves every cluster
/// path out of the server loop rather than running a tracker that publishes nothing. Publishing
/// nothing is the separate, and far more common, stats-only mode: clustering on with no broker URL.
fn clusters_from_env() -> anyhow::Result<Option<ClusterTracker>> {
    if !env_bool_or("PULSE_CLUSTERS_ENABLED", DEFAULT_CLUSTERS_ENABLED)? {
        tracing::info!("peer clustering disabled");
        return Ok(None);
    }
    let pass_interval_ms = env_or("PULSE_CLUSTERS_PASS_INTERVAL_MS", DEFAULT_PASS_INTERVAL_MS)?;
    if pass_interval_ms == 0 {
        tracing::info!("peer clustering disabled: PULSE_CLUSTERS_PASS_INTERVAL_MS is not positive");
        return Ok(None);
    }
    let options = ClusterOptions {
        enabled: true,
        pass_interval_ms,
        dwell_passes: env_or("PULSE_CLUSTERS_DWELL_PASSES", DEFAULT_DWELL_PASSES)?,
        id_prefix: env_or("PULSE_CLUSTERS_ID_PREFIX", DEFAULT_ID_PREFIX.to_string())?,
        session_retention_passes: env_or(
            "PULSE_CLUSTERS_SESSION_RETENTION_PASSES",
            DEFAULT_SESSION_RETENTION_PASSES,
        )?,
    };
    tracing::info!(
        pass_interval_ms = options.pass_interval_ms,
        dwell_passes = options.dwell_passes,
        id_prefix = %options.id_prefix,
        "peer clustering enabled"
    );
    Ok(Some(ClusterTracker::new(
        options,
        ENET_CAPACITY + WT_CAPACITY,
        cluster_feed_from_env()?,
    )))
}

/// The deployed build's commit, which is what a discovery consumer tells deployments apart by.
/// Unset or blank advertises `unknown`, which is upstream's fallback -- the crate version is
/// deliberately not used, because every build of one version shares it.
#[cfg(feature = "nats")]
fn commit_hash_from_env() -> String {
    use catalyrst_pulse::cluster::nats::DEFAULT_COMMIT_HASH;

    match std::env::var("COMMIT_HASH") {
        Ok(hash) if !hash.trim().is_empty() => hash.trim().to_string(),
        _ => DEFAULT_COMMIT_HASH.to_string(),
    }
}

/// The broker URL, `PULSE_NATS_URL` first and the shared `NATS_URL` after it, so a host that
/// already exports one broker for its services needs no Pulse-specific copy of it.
fn nats_url_from_env() -> anyhow::Result<Option<String>> {
    for key in ["PULSE_NATS_URL", "NATS_URL"] {
        let value = env_value(key, std::env::var(key))?;
        match value.as_deref().map(str::trim) {
            Some(url) if !url.is_empty() => return Ok(Some(url.to_string())),
            _ => continue,
        }
    }
    Ok(None)
}

#[cfg(feature = "nats")]
fn cluster_feed_from_env() -> anyhow::Result<Arc<dyn ClusterFeedPublisher>> {
    use catalyrst_pulse::cluster::nats::{
        NatsClusterFeed, NatsFeedOptions, DEFAULT_CHANNEL_CAPACITY, DEFAULT_DISCOVERY_INTERVAL_MS,
        DEFAULT_SERVER_NAME,
    };

    let Some(url) = nats_url_from_env()? else {
        tracing::info!("cluster feed in stats-only mode: no broker URL set");
        return Ok(Arc::new(NoopClusterFeedPublisher));
    };
    let options = NatsFeedOptions {
        url,
        server_name: env_or("PULSE_NATS_SERVER_NAME", DEFAULT_SERVER_NAME.to_string())?,
        commit_hash: commit_hash_from_env(),
        discovery_interval_ms: env_or(
            "PULSE_NATS_DISCOVERY_INTERVAL_MS",
            DEFAULT_DISCOVERY_INTERVAL_MS,
        )?,
        capacity: env_or("PULSE_NATS_CHANNEL_CAPACITY", DEFAULT_CHANNEL_CAPACITY)?,
    };
    if options.capacity == 0 {
        tracing::warn!(
            "PULSE_NATS_CHANNEL_CAPACITY is not positive: each assignment evicts the previous one, \
             so almost everything is lost -- watch pulse_nats_dropped_total"
        );
    }
    if options.discovery_interval_ms == 0 {
        tracing::warn!(
            "PULSE_NATS_DISCOVERY_INTERVAL_MS is not positive: assignments and topology still \
             publish, the service is not advertised on engine.discovery"
        );
    }
    tracing::info!(url = %options.url, server_name = %options.server_name, "cluster feed publishing");
    Ok(Arc::new(NatsClusterFeed::spawn(options)))
}

/// Built without a broker client: a URL that cannot be honoured fails startup rather than being
/// ignored, so a deployment never silently runs blind while its operator believes it publishes.
#[cfg(not(feature = "nats"))]
fn cluster_feed_from_env() -> anyhow::Result<Arc<dyn ClusterFeedPublisher>> {
    if let Some(url) = nats_url_from_env()? {
        anyhow::bail!("a broker URL is set (`{url}`) but this build has no `nats` feature");
    }
    Ok(Arc::new(NoopClusterFeedPublisher))
}

fn env_bool(key: &str) -> anyhow::Result<bool> {
    parse_bool(key, env_value(key, std::env::var(key))?)
}

fn env_bool_or(key: &str, default: bool) -> anyhow::Result<bool> {
    parse_bool_or(key, env_value(key, std::env::var(key))?, default)
}

/// Unset and blank mean off; the documented spellings match trimmed and case-insensitively;
/// anything else is a startup error, because reading a typo as "off" would close the browser
/// front door silently.
fn parse_bool(key: &str, raw: Option<String>) -> anyhow::Result<bool> {
    parse_bool_or(key, raw, false)
}

/// As `parse_bool`, for a setting whose unset state is on: unset and blank both take `default`,
/// which keeps a blanked-out env template reading as the shipped behaviour rather than silently
/// turning a feature off.
fn parse_bool_or(key: &str, raw: Option<String>, default: bool) -> anyhow::Result<bool> {
    let Some(raw) = raw else {
        return Ok(default);
    };
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        Ok(default)
    } else if BOOL_FALSE.contains(&value.as_str()) {
        Ok(false)
    } else if BOOL_TRUE.contains(&value.as_str()) {
        Ok(true)
    } else {
        anyhow::bail!(
            "{key} `{}`: expected one of {} or {}",
            raw.trim(),
            BOOL_TRUE.join("/"),
            BOOL_FALSE.join("/")
        )
    }
}

fn env_or<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    parse_or(key, env_value(key, std::env::var(key))?, default)
}

/// Unset is the default; a value that is not UTF-8 fails startup naming the key, because reading
/// it as unset would swap an operator's explicit setting for the default without a trace.
fn env_value(key: &str, raw: Result<String, VarError>) -> anyhow::Result<Option<String>> {
    match raw {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(value)) => {
            anyhow::bail!("{key} `{}`: not valid UTF-8", value.to_string_lossy())
        }
    }
}

/// Unset and blank both mean the default; anything else must parse.
fn parse_or<T>(key: &str, raw: Option<String>, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match raw.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value
            .parse()
            .map_err(|e| anyhow::anyhow!("{key} `{value}`: {e}")),
        _ => Ok(default),
    }
}

/// Read a PEM blob from an inline env var (takes precedence) or a file path.
fn read_pem(inline_key: &str, path_key: &str) -> anyhow::Result<Option<String>> {
    if let Ok(pem) = std::env::var(inline_key) {
        if !pem.trim().is_empty() {
            return Ok(Some(pem));
        }
    }
    if let Ok(path) = std::env::var(path_key) {
        if !path.trim().is_empty() {
            return Ok(Some(
                std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("reading {path_key}={path}: {e}"))?,
            ));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_pulse::hardening::{
        DEFAULT_SCENE_LISTENER_MAX_PARCELS, SCENE_LISTENER_REALM_BUDGET_COST,
    };
    use catalyrst_pulse::interest::{
        AOI_MAX_RADIUS_CEILING, DEFAULT_AOI_MAX_RADIUS, DEFAULT_AOI_TIER0_RADIUS,
        DEFAULT_AOI_TIER1_RADIUS,
    };
    use catalyrst_pulse::metrics::DEFAULT_METRICS_BIND;

    fn doc_for(key: &str) -> &'static str {
        ENV_DOCS
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, doc)| *doc)
            .unwrap_or_else(|| panic!("{key} is read by this crate but missing from ENV_DOCS"))
    }

    #[test]
    fn env_docs_defaults_track_the_constants() {
        let pinned: [(&str, String); 20] = [
            ("PULSE_BIND", DEFAULT_BIND.to_string()),
            ("PULSE_METRICS_BIND", DEFAULT_METRICS_BIND.to_string()),
            ("PULSE_INPUT_MAX_HZ", DEFAULT_INPUT_MAX_HZ.to_string()),
            ("PULSE_INPUT_BURST", DEFAULT_INPUT_BURST.to_string()),
            (
                "PULSE_DISCRETE_RATE_PER_SEC",
                DEFAULT_DISCRETE_RATE_PER_SEC.to_string(),
            ),
            ("PULSE_DISCRETE_BURST", DEFAULT_DISCRETE_BURST.to_string()),
            (
                "PULSE_SCENE_LISTENER_MAX_PARCELS",
                DEFAULT_SCENE_LISTENER_MAX_PARCELS.to_string(),
            ),
            (
                "PULSE_AOI_TIER0_RADIUS",
                DEFAULT_AOI_TIER0_RADIUS.to_string(),
            ),
            (
                "PULSE_AOI_TIER1_RADIUS",
                DEFAULT_AOI_TIER1_RADIUS.to_string(),
            ),
            ("PULSE_AOI_MAX_RADIUS", DEFAULT_AOI_MAX_RADIUS.to_string()),
            ("PULSE_WT_BIND", DEFAULT_WT_BIND.to_string()),
            (
                "PULSE_CLUSTERS_ENABLED",
                DEFAULT_CLUSTERS_ENABLED.to_string(),
            ),
            (
                "PULSE_CLUSTERS_PASS_INTERVAL_MS",
                DEFAULT_PASS_INTERVAL_MS.to_string(),
            ),
            (
                "PULSE_CLUSTERS_DWELL_PASSES",
                DEFAULT_DWELL_PASSES.to_string(),
            ),
            ("PULSE_CLUSTERS_ID_PREFIX", DEFAULT_ID_PREFIX.to_string()),
            (
                "PULSE_CLUSTERS_SESSION_RETENTION_PASSES",
                DEFAULT_SESSION_RETENTION_PASSES.to_string(),
            ),
            ("PULSE_NATS_URL", "unset".to_string()),
            ("NATS_URL", "unset".to_string()),
            (
                "PULSE_WT_MAX_DATAGRAM_BYTES",
                DEFAULT_MAX_DATAGRAM_BYTES.to_string(),
            ),
            ("RUST_LOG", DEFAULT_LOG_FILTER.to_string()),
        ];
        for (key, default) in &pinned {
            let doc = doc_for(key);
            let suffix = format!("(default {default})");
            assert!(
                doc.ends_with(&suffix),
                "{key}: `{doc}` must end with `{suffix}`"
            );
        }
        assert!(doc_for("PULSE_WT_MAX_MESSAGE_BYTES")
            .ends_with(&format!("(default {DEFAULT_MAX_MESSAGE_BYTES})")));
        assert_nats_feed_defaults_are_documented();
        assert!(
            doc_for("PULSE_SCENE_LISTENER_MAX_PARCELS").contains(&format!(
                "plus {SCENE_LISTENER_REALM_BUDGET_COST} per realm"
            )),
            "the per-realm budget cost is documented next to the parcel budget"
        );
        assert!(
            doc_for("PULSE_AOI_MAX_RADIUS").contains(&format!("at most {AOI_MAX_RADIUS_CEILING}")),
            "the radius ceiling is documented next to the default"
        );
        let wt = doc_for("PULSE_WT_ENABLED");
        assert!(
            wt.contains(&BOOL_TRUE.join("/")) && wt.contains(&BOOL_FALSE.join("/")),
            "the accepted boolean spellings are documented: `{wt}`"
        );
    }

    #[cfg(feature = "nats")]
    fn assert_nats_feed_defaults_are_documented() {
        use catalyrst_pulse::cluster::nats::{
            DEFAULT_CHANNEL_CAPACITY, DEFAULT_COMMIT_HASH, DEFAULT_DISCOVERY_INTERVAL_MS,
            DEFAULT_SERVER_NAME,
        };
        for (key, default) in [
            ("COMMIT_HASH", DEFAULT_COMMIT_HASH.to_string()),
            ("PULSE_NATS_SERVER_NAME", DEFAULT_SERVER_NAME.to_string()),
            (
                "PULSE_NATS_DISCOVERY_INTERVAL_MS",
                DEFAULT_DISCOVERY_INTERVAL_MS.to_string(),
            ),
            (
                "PULSE_NATS_CHANNEL_CAPACITY",
                DEFAULT_CHANNEL_CAPACITY.to_string(),
            ),
        ] {
            let doc = doc_for(key);
            let suffix = format!("(default {default})");
            assert!(
                doc.ends_with(&suffix),
                "{key}: `{doc}` must end with `{suffix}`"
            );
        }
    }

    /// The keys stay documented without the feature, because the same env template feeds a build
    /// with it and one without.
    #[cfg(not(feature = "nats"))]
    fn assert_nats_feed_defaults_are_documented() {
        for key in [
            "COMMIT_HASH",
            "PULSE_NATS_SERVER_NAME",
            "PULSE_NATS_DISCOVERY_INTERVAL_MS",
            "PULSE_NATS_CHANNEL_CAPACITY",
        ] {
            let _ = doc_for(key);
        }
    }

    #[test]
    fn env_docs_keys_are_unique() {
        let mut keys: Vec<&str> = ENV_DOCS.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate ENV_DOCS key");
    }

    #[test]
    fn parse_bool_is_strict_case_insensitive_and_off_when_unset() {
        let key = "PULSE_WT_ENABLED";
        assert!(!parse_bool(key, None).unwrap());
        assert!(!parse_bool(key, Some("   ".into())).unwrap());
        for on in ["1", "true", "TRUE", " Yes ", "On", "yES"] {
            assert!(parse_bool(key, Some(on.into())).unwrap(), "{on:?}");
        }
        for off in ["0", "false", "FALSE", " No ", "Off", "fAlSe"] {
            assert!(!parse_bool(key, Some(off.into())).unwrap(), "{off:?}");
        }
        for bad in ["maybe", "2", "enabled", "t", "y", "-1", "true false"] {
            let err = parse_bool(key, Some(format!(" {bad} "))).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!(
                    "PULSE_WT_ENABLED `{bad}`: expected one of 1/true/yes/on or 0/false/no/off"
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn parse_or_defaults_on_unset_or_blank_and_rejects_garbage() {
        assert_eq!(parse_or("K", None, 30.0f32).unwrap(), 30.0);
        assert_eq!(parse_or("K", Some("  ".into()), 30.0f32).unwrap(), 30.0);
        assert_eq!(parse_or("K", Some(" 75.5 ".into()), 30.0f32).unwrap(), 75.5);
        assert_eq!(parse_or("K", Some("7".into()), 1u32).unwrap(), 7);
        let err = parse_or("PULSE_AOI_MAX_RADIUS", Some("far".into()), 200.0f32).unwrap_err();
        assert!(
            err.to_string().starts_with("PULSE_AOI_MAX_RADIUS `far`"),
            "{err}"
        );
        for garbage in ["lots", "-1", "16.5", "0x10"] {
            let err = parse_or("PULSE_INPUT_BURST", Some(garbage.into()), 16u32).unwrap_err();
            assert!(
                err.to_string()
                    .starts_with(&format!("PULSE_INPUT_BURST `{garbage}`")),
                "{err}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn env_value_distinguishes_unset_from_not_unicode() {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(
            env_value("K", Ok("7".into())).unwrap(),
            Some("7".to_string())
        );
        assert_eq!(env_value("K", Err(VarError::NotPresent)).unwrap(), None);
        let raw = std::ffi::OsString::from_vec(vec![b'o', b'n', 0xff]);
        let err = env_value("PULSE_WT_ENABLED", Err(VarError::NotUnicode(raw))).unwrap_err();
        assert_eq!(
            err.to_string(),
            "PULSE_WT_ENABLED `on\u{fffd}`: not valid UTF-8"
        );
    }
}
