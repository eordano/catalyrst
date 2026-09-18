use anyhow::{anyhow, Result};
use catalyrst_envcfg::{env_bool, get_port, local_endpoint, required, required_endpoint};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub livekit_host: String,
    /// `LIVEKIT_API_HOST` -- the endpoint this service calls for room admin,
    /// ingress and participant listing when it differs from the host explorers
    /// dial. `None` derives it from `livekit_host`.
    pub livekit_api_host: Option<String>,
    /// `LIVEKIT_WS_URL` -- the signaling URL handed to explorers when it
    /// differs from the API side. `None` derives it from `livekit_host`.
    pub livekit_ws_url: Option<String>,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_webhook_key: Option<String>,
    pub livekit_configured: bool,

    pub private_messages_room_id: String,
    pub places_api_url: String,
    pub catalyst_url: String,

    pub world_content_url: String,

    pub lambdas_url: String,
    pub dapps_database_url: Option<String>,
    pub dapps_schema: String,

    pub places_database_url: Option<String>,
    pub authoritative_server_address: Option<String>,
    pub moderator_token: Option<String>,
    pub moderator_addresses: Vec<String>,

    pub gatekeeper_auth_token: Option<String>,
    pub pulse_room_authority_key: Option<[u8; 32]>,

    /// `FED_PEER_ID` -- this catalyst's federation identity, stamped as
    /// `epoch_author` on MLS groups it creates. `None` falls back to the
    /// DB-persisted per-instance id minted by migration 0009 (resolved in
    /// `build_state`), never to a shared literal that collides across
    /// instances.
    pub fed_peer_id: Option<String>,

    pub cluster: ClusterConfig,
}

/// The Pulse cluster feed's settings. Env names are bare and upstream-matching like the rest of
/// this crate's, and `NATS_URL` is deliberately the same flat name catalyrst-pulse and the
/// archipelago services read, so one broker URL serves the whole platform unmodified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterConfig {
    /// `None` leaves the whole feed inert: nothing connects and nothing subscribes.
    pub nats_url: Option<String>,
    /// Off unless `CLUSTER_SUBSCRIBER_ENABLED` is exactly `true`. Off subscribes to nothing and
    /// is indistinguishable from the subscriber not existing.
    pub enabled: bool,
    /// Shared by the minting and connect subscriptions, so exactly one replica answers each
    /// event. Without it every replica would mint, and one client would be handed N different
    /// tokens for the same move.
    pub queue_group: String,
    /// Base delay between displaced-session removal attempts. A configured 0 is a real value: a
    /// retry with no sleep before it, not an unset variable.
    pub takeover_retry_delay_ms: u64,
    /// Island JWT lifetime. Environment values are capped at
    /// [`MAX_ISLAND_TOKEN_TTL_SECONDS`] so the wallet-work deadline always contains quarantine.
    pub island_token_ttl_seconds: u64,
    /// Wait out old island JWTs when a same-room owner is replaced. Keep this enabled for
    /// self-hosted LiveKit; disabling it is safe only when the deployment provides LiveKit
    /// Cloud's cutoff-based token revocation.
    pub self_hosted_token_quarantine: bool,
    /// Graceful shutdown budget; 0 cancels immediately. Both paths join cancelled work afterward.
    pub drain_timeout_ms: u64,
    pub peer_state_max: usize,
    pub peer_state_ttl_ms: u64,
    pub assignment_mirror_max: usize,
    pub assignment_mirror_ttl_ms: u64,
    pub control_database_url: Option<String>,
    pub control_v4_audience: Option<String>,
}

pub const DEFAULT_CLUSTER_QUEUE_GROUP: &str = "catalyrst-comms-cluster";
/// Short on purpose: the client uses the string within a second of receiving it, and the
/// self-hosted takeover quarantine must wait out this lifetime plus LiveKit's verifier leeway.
pub const DEFAULT_ISLAND_TOKEN_TTL_SECONDS: u64 = 60;
/// Keeps the maximum same-room quarantine at 122 seconds, within the wallet-work deadline.
pub const MAX_ISLAND_TOKEN_TTL_SECONDS: u64 = 60;
pub const DEFAULT_TAKEOVER_RETRY_DELAY_MS: u64 = 100;
/// Long enough for a mint and its publish to finish, short enough that one request with no timeout
/// of its own cannot hold shutdown open until the orchestrator kills the process.
pub const DEFAULT_DRAIN_TIMEOUT_MS: u64 = 5_000;

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            nats_url: None,
            enabled: false,
            queue_group: DEFAULT_CLUSTER_QUEUE_GROUP.to_string(),
            takeover_retry_delay_ms: DEFAULT_TAKEOVER_RETRY_DELAY_MS,
            island_token_ttl_seconds: DEFAULT_ISLAND_TOKEN_TTL_SECONDS,
            self_hosted_token_quarantine: true,
            drain_timeout_ms: DEFAULT_DRAIN_TIMEOUT_MS,
            peer_state_max: crate::peer_state::DEFAULT_PEER_STATE_MAX,
            peer_state_ttl_ms: crate::peer_state::DEFAULT_PEER_STATE_TTL_MS,
            assignment_mirror_max: crate::peer_state::DEFAULT_ASSIGNMENT_MIRROR_MAX,
            assignment_mirror_ttl_ms: crate::peer_state::DEFAULT_ASSIGNMENT_MIRROR_TTL_MS,
            control_database_url: None,
            control_v4_audience: None,
        }
    }
}

/// A set, parsable, strictly positive value; anything else is the default. Guards every sizing
/// knob whose zero would silently disable the thing it sizes.
fn positive_or<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr + PartialOrd + Default + Copy,
{
    env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<T>().ok())
        .filter(|v| *v > T::default())
        .unwrap_or(default)
}

impl ClusterConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            nats_url: env::var("NATS_URL")
                .ok()
                .map(|u| u.trim().to_string())
                .filter(|u| !u.is_empty()),
            enabled: env::var("CLUSTER_SUBSCRIBER_ENABLED").as_deref() == Ok("true"),
            queue_group: env::var("NATS_QUEUE_GROUP")
                .ok()
                .map(|g| g.trim().to_string())
                .filter(|g| !g.is_empty())
                .unwrap_or(defaults.queue_group),
            takeover_retry_delay_ms: env::var("CLUSTER_TAKEOVER_RETRY_DELAY_MS")
                .ok()
                .and_then(|raw| raw.trim().parse().ok())
                .unwrap_or(defaults.takeover_retry_delay_ms),
            drain_timeout_ms: env::var("CLUSTER_DRAIN_TIMEOUT_MS")
                .ok()
                .and_then(|raw| raw.trim().parse().ok())
                .unwrap_or(defaults.drain_timeout_ms),
            island_token_ttl_seconds: positive_or(
                "CLUSTER_ISLAND_TOKEN_TTL_SECONDS",
                defaults.island_token_ttl_seconds,
            )
            .min(MAX_ISLAND_TOKEN_TTL_SECONDS),
            self_hosted_token_quarantine: env_bool(
                "CLUSTER_SELF_HOSTED_TOKEN_QUARANTINE",
                defaults.self_hosted_token_quarantine,
            ),
            peer_state_max: positive_or("CLUSTER_PEER_STATE_MAX", defaults.peer_state_max),
            peer_state_ttl_ms: positive_or("CLUSTER_PEER_STATE_TTL_MS", defaults.peer_state_ttl_ms),
            assignment_mirror_max: positive_or(
                "CLUSTER_ASSIGNMENT_MIRROR_MAX",
                defaults.assignment_mirror_max,
            ),
            assignment_mirror_ttl_ms: positive_or(
                "CLUSTER_ASSIGNMENT_MIRROR_TTL_MS",
                defaults.assignment_mirror_ttl_ms,
            ),
            control_database_url: env::var("COMMS_CONTROL_PG_CONNECTION_STRING")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            control_v4_audience: env::var("COMMS_CONTROL_V4_AUDIENCE")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }
    }
}

fn parse_moderator_addresses(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\n'])
        .map(|s| s.trim().to_lowercase())
        .filter(|a| catalyrst_types::is_eth_address(a))
        .collect()
}

/// Maps [`catalyrst_livekit::resolve_creds`] onto this config's
/// `(api_key, api_secret, livekit_configured)` triple. Unset, blank, and
/// placeholder (`devkey`/`devsecret`, any case) credentials all count as
/// unconfigured: without the `LIVEKIT_ALLOW_DEV_CREDS` opt-in the service
/// refuses to boot instead of silently minting tokens no real SFU accepts.
fn resolve_livekit_env(
    api_key: String,
    api_secret: String,
    allow_dev_creds: bool,
) -> Result<(String, String, bool)> {
    use catalyrst_livekit::ResolvedCreds;
    match catalyrst_livekit::resolve_creds(api_key, api_secret, allow_dev_creds) {
        ResolvedCreds::Configured {
            api_key,
            api_secret,
        } => Ok((api_key, api_secret, true)),
        ResolvedCreds::DevFallback => {
            tracing::warn!(
                "LIVEKIT_API_KEY / LIVEKIT_API_SECRET unset or still the devkey/devsecret \
                 placeholders; running on the dev defaults \u{2014} tokens will parse locally but \
                 will NOT be accepted by a real LiveKit cluster"
            );
            Ok((
                catalyrst_livekit::DEV_API_KEY.to_string(),
                catalyrst_livekit::DEV_API_SECRET.to_string(),
                false,
            ))
        }
        ResolvedCreds::Unconfigured => Err(anyhow!(
            "LIVEKIT_API_KEY / LIVEKIT_API_SECRET are unset or still the devkey/devsecret \
             placeholders; set real credentials, or set LIVEKIT_ALLOW_DEV_CREDS=1 to run \
             with the dev defaults"
        )),
    }
}

fn parse_pulse_room_authority_key(
    raw: Option<&str>,
    livekit_secret: &str,
) -> Result<Option<[u8; 32]>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let Some(raw) = raw else {
        return Ok(None);
    };
    let decoded = URL_SAFE_NO_PAD.decode(raw).map_err(|_| {
        anyhow!("PULSE_ROOM_AUTHORITY_KEY must be 32 base64url bytes without padding")
    })?;
    if raw == livekit_secret || decoded == livekit_secret.as_bytes() {
        return Err(anyhow!(
            "PULSE_ROOM_AUTHORITY_KEY must not reuse the LiveKit secret"
        ));
    }
    decoded
        .try_into()
        .map(Some)
        .map_err(|_| anyhow!("PULSE_ROOM_AUTHORITY_KEY must be 32 base64url bytes without padding"))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let (livekit_api_key, livekit_api_secret, livekit_configured) = resolve_livekit_env(
            env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
            env_bool("LIVEKIT_ALLOW_DEV_CREDS", false),
        )?;

        let cluster = ClusterConfig::from_env();
        let pulse_room_authority_key = parse_pulse_room_authority_key(
            env::var("PULSE_ROOM_AUTHORITY_KEY").ok().as_deref(),
            &livekit_api_secret,
        )?;
        if cluster.control_database_url.is_some() != cluster.control_v4_audience.is_some() {
            return Err(anyhow!(
                "COMMS_CONTROL_PG_CONNECTION_STRING and COMMS_CONTROL_V4_AUDIENCE must be set together"
            ));
        }
        if cluster
            .control_v4_audience
            .as_ref()
            .is_some_and(|audience| audience.len() > 256)
        {
            return Err(anyhow!("COMMS_CONTROL_V4_AUDIENCE exceeds 256 bytes"));
        }

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5138)?,
            database_url: required("COMMS_PG_CONNECTION_STRING")?,
            livekit_host: env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string()),
            livekit_api_host: env::var("LIVEKIT_API_HOST")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            livekit_ws_url: env::var("LIVEKIT_WS_URL")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            livekit_api_key,
            livekit_api_secret,
            livekit_webhook_key: env::var("LIVEKIT_WEBHOOK_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            livekit_configured,
            private_messages_room_id: env::var("PRIVATE_MESSAGES_ROOM_ID")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "private-messages".to_string()),
            places_api_url: env::var("PLACES_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5134".to_string()),
            catalyst_url: local_endpoint("CATALYST_URL", 5141),
            world_content_url: local_endpoint("WORLD_CONTENT_URL", 5142)
                .trim_end_matches('/')
                .to_string(),
            lambdas_url: required_endpoint("LAMBDAS_URL")?,
            dapps_database_url: env::var("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            dapps_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),
            places_database_url: env::var("PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            authoritative_server_address: env::var("AUTHORITATIVE_SERVER_ADDRESS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
            moderator_token: env::var("MODERATOR_TOKEN").ok().filter(|s| !s.is_empty()),
            moderator_addresses: env::var("PLATFORM_USER_MODERATORS")
                .ok()
                .map(|s| parse_moderator_addresses(&s))
                .unwrap_or_default(),
            gatekeeper_auth_token: env::var("COMMS_GATEKEEPER_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            pulse_room_authority_key,
            fed_peer_id: env::var("FED_PEER_ID")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            cluster,
        })
    }

    /// The client signaling URL and the API base this service calls, each
    /// overridable on its own and otherwise derived from `LIVEKIT_HOST`.
    pub fn livekit_endpoints(&self) -> catalyrst_livekit::LivekitEndpoints {
        catalyrst_livekit::resolve_endpoints(
            &self.livekit_host,
            self.livekit_api_host.as_deref(),
            self.livekit_ws_url.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_livekit_env, Config};

    #[test]
    fn room_authority_key_is_explicit_canonical_and_separate() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let parse = super::parse_pulse_room_authority_key;
        assert_eq!(parse(None, "secret").unwrap(), None);
        let encoded = URL_SAFE_NO_PAD.encode([7; 32]);
        assert_eq!(parse(Some(&encoded), "secret").unwrap(), Some([7; 32]));
        for key in [
            "",
            "not a key",
            &format!("{encoded}="),
            &URL_SAFE_NO_PAD.encode([7; 31]),
        ] {
            assert!(parse(Some(key), "secret").is_err());
        }
        assert!(parse(Some(&encoded), &encoded).is_err());
        let raw_secret = "01234567890123456789012345678901";
        assert!(parse(Some(&URL_SAFE_NO_PAD.encode(raw_secret)), raw_secret).is_err());
    }

    /// `CATALYST_URL` resolves to this service's own loopback port (5141), never
    /// an upstream Decentraland host, and -- the regression this pins -- a blank
    /// value falls back to that default instead of becoming an empty string that
    /// would make every catalyst call hit `http:///...`. Mirrors the
    /// `resolve_livekit_env` tests: exercise the real env-resolution path.
    #[test]
    fn catalyst_url_defaults_to_local_5141_and_blank_falls_back() {
        std::env::set_var("COMMS_PG_CONNECTION_STRING", "postgres://localhost/x");
        std::env::set_var("LAMBDAS_URL", "http://127.0.0.1:1");
        std::env::set_var("LIVEKIT_ALLOW_DEV_CREDS", "1");

        std::env::remove_var("CATALYST_URL");
        assert_eq!(
            Config::from_env().unwrap().catalyst_url,
            "http://127.0.0.1:5141",
            "unset CATALYST_URL must resolve to the local catalyst loopback"
        );

        std::env::set_var("CATALYST_URL", "   ");
        assert_eq!(
            Config::from_env().unwrap().catalyst_url,
            "http://127.0.0.1:5141",
            "a blank CATALYST_URL must fall back to the loopback default, not empty-string"
        );

        std::env::set_var("CATALYST_URL", "http://catalyst.internal:5141");
        assert_eq!(
            Config::from_env().unwrap().catalyst_url,
            "http://catalyst.internal:5141",
            "an explicit CATALYST_URL must pass through unchanged"
        );

        std::env::remove_var("LIVEKIT_API_HOST");
        std::env::set_var("LIVEKIT_WS_URL", "  ");
        let single = Config::from_env().unwrap();
        assert_eq!(
            (
                single.livekit_api_host.as_deref(),
                single.livekit_ws_url.as_deref()
            ),
            (None, None)
        );
        assert_eq!(
            single.livekit_endpoints(),
            catalyrst_livekit::resolve_endpoints(&single.livekit_host, None, None),
            "a blank LIVEKIT_WS_URL must fall back to LIVEKIT_HOST, not become empty"
        );

        std::env::set_var("LIVEKIT_API_HOST", "http://livekit-api.internal:7880");
        std::env::set_var("LIVEKIT_WS_URL", "wss://livekit.example.com");
        let split = Config::from_env().unwrap().livekit_endpoints();
        assert_eq!(split.client_url, "wss://livekit.example.com");
        assert_eq!(split.api_url, "http://livekit-api.internal:7880");

        std::env::remove_var("LIVEKIT_API_HOST");
        std::env::remove_var("LIVEKIT_WS_URL");
        std::env::remove_var("CATALYST_URL");
        std::env::remove_var("COMMS_PG_CONNECTION_STRING");
        std::env::remove_var("LAMBDAS_URL");
        std::env::remove_var("LIVEKIT_ALLOW_DEV_CREDS");
    }

    #[test]
    fn real_creds_configure_livekit() {
        let (k, s, configured) =
            resolve_livekit_env("APIabc".into(), "supersecret".into(), false).unwrap();
        assert_eq!((k.as_str(), s.as_str()), ("APIabc", "supersecret"));
        assert!(configured);
    }

    #[test]
    fn placeholder_creds_are_treated_as_unset() {
        for (k, s) in [
            ("devkey", "devsecret"),
            ("DevKey", "DEVSECRET"),
            ("devkey", "supersecret"),
            ("APIabc", "devsecret"),
            ("", ""),
        ] {
            assert!(
                resolve_livekit_env(k.into(), s.into(), false).is_err(),
                "({k:?}, {s:?}) must refuse to boot without LIVEKIT_ALLOW_DEV_CREDS"
            );
        }
        let (k, s, configured) =
            resolve_livekit_env("devkey".into(), "devsecret".into(), true).unwrap();
        assert_eq!((k.as_str(), s.as_str()), ("devkey", "devsecret"));
        assert!(!configured);
    }
}
