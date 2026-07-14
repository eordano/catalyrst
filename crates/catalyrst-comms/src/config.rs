use anyhow::{anyhow, Result};
use catalyrst_envcfg::{env_bool, get_port, local_endpoint, required, required_endpoint};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub livekit_host: String,
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

    /// `FED_PEER_ID` -- this catalyst's federation identity, stamped as
    /// `epoch_author` on MLS groups it creates. `None` falls back to the
    /// DB-persisted per-instance id minted by migration 0009 (resolved in
    /// `build_state`), never to a shared literal that collides across
    /// instances.
    pub fed_peer_id: Option<String>,
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

impl Config {
    pub fn from_env() -> Result<Self> {
        let (livekit_api_key, livekit_api_secret, livekit_configured) = resolve_livekit_env(
            env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
            env_bool("LIVEKIT_ALLOW_DEV_CREDS", false),
        )?;

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5138)?,
            database_url: required("COMMS_PG_CONNECTION_STRING")?,
            livekit_host: env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string()),
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
            fed_peer_id: env::var("FED_PEER_ID")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_livekit_env, Config};

    /// `CATALYST_URL` resolves to this service's own loopback port (5141), never
    /// an upstream Decentraland host, and -- the regression this pins -- a blank
    /// value falls back to that default instead of becoming an empty string that
    /// would make every catalyst call hit `http:///...`. Mirrors the
    /// `resolve_livekit_env` tests: exercise the real env-resolution path.
    #[test]
    fn catalyst_url_defaults_to_local_5141_and_blank_falls_back() {
        // from_env needs these to construct a Config at all; the dev-creds
        // opt-in keeps the LiveKit gate from refusing to boot mid-test.
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
        // devkey/devsecret (any case) must NOT count as configured: without
        // the opt-in the service refuses to boot...
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
        // ...and with it, the gate + warning path runs with configured=false.
        let (k, s, configured) =
            resolve_livekit_env("devkey".into(), "devsecret".into(), true).unwrap();
        assert_eq!((k.as_str(), s.as_str()), ("devkey", "devsecret"));
        assert!(!configured);
    }
}
