use anyhow::{anyhow, Result};
use catalyrst_envcfg::{
    env_bool, get_int, get_port, get_u64, optional_endpoint, required, required_endpoint,
};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub http_base_url: String,
    pub network_id: i64,

    pub squid_database_url: Option<String>,
    pub global_scenes_urn: Option<String>,
    pub content_public_url: String,
    pub lambdas_public_url: String,

    pub livekit_host: String,
    pub livekit_ws_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_configured: bool,
    pub livekit_webhook_key: Option<String>,
    pub max_users_per_world: i64,

    pub contents_upstream_url: Option<String>,
    pub contents_dir: std::path::PathBuf,

    pub comms_gatekeeper_url: Option<String>,
    pub comms_gatekeeper_auth_token: Option<String>,

    pub denylist_json_url: Option<String>,

    pub dcl_lists_url: Option<String>,

    pub admin_token: Option<String>,

    pub max_in_flight_upload_bytes: u64,
    pub max_concurrent_uploads: u64,
    pub max_in_flight_upload_files: u64,
    pub multipart_upload_timeout_ms: u64,
    pub deployment_processing_timeout_ms: u64,

    /// The five `WORLDS_FED_*` keys, grouped rather than splayed across this struct
    /// so that "which knobs belong to federation" is answerable by reading one type.
    /// Parsed and validated at boot by [`crate::fed::config::WorldsFedConfig::from_env`];
    /// a zero or unparseable cap is a startup failure, not a runtime surprise.
    pub federation: crate::fed::config::WorldsFedConfig,
}

fn positive_limit(name: &str, value: u64) -> Result<u64> {
    if value == 0 {
        return Err(anyhow!("{name} must be a positive integer, got {value}"));
    }
    Ok(value)
}

fn validate_upload_limits(max_in_flight_upload_bytes: u64) -> Result<()> {
    let max_upload = crate::handlers::deploy::MAX_UPLOAD_SIZE_BYTES as u64;
    if max_in_flight_upload_bytes < max_upload {
        return Err(anyhow!(
            "MAX_IN_FLIGHT_UPLOAD_BYTES ({max_in_flight_upload_bytes}) must be greater than or \
             equal to maxSizeInBytes ({max_upload})"
        ));
    }
    Ok(())
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
                 placeholders; running on the dev defaults — tokens will parse locally but \
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
        let http_port = get_port("HTTP_SERVER_PORT", 5146)?;

        let max_in_flight_upload_bytes = positive_limit(
            "MAX_IN_FLIGHT_UPLOAD_BYTES",
            get_u64(
                "MAX_IN_FLIGHT_UPLOAD_BYTES",
                crate::upload_limits::DEFAULT_MAX_IN_FLIGHT_UPLOAD_BYTES,
            )?,
        )?;
        let max_concurrent_uploads = positive_limit(
            "MAX_CONCURRENT_UPLOADS",
            get_u64(
                "MAX_CONCURRENT_UPLOADS",
                crate::upload_limits::DEFAULT_MAX_CONCURRENT_UPLOADS,
            )?,
        )?;
        let max_in_flight_upload_files = positive_limit(
            "MAX_IN_FLIGHT_UPLOAD_FILES",
            get_u64(
                "MAX_IN_FLIGHT_UPLOAD_FILES",
                crate::upload_limits::DEFAULT_MAX_IN_FLIGHT_UPLOAD_FILES,
            )?,
        )?;
        let multipart_upload_timeout_ms = positive_limit(
            "MULTIPART_UPLOAD_TIMEOUT_MS",
            get_u64(
                "MULTIPART_UPLOAD_TIMEOUT_MS",
                crate::upload_limits::DEFAULT_MULTIPART_UPLOAD_TIMEOUT_MS,
            )?,
        )?;
        let deployment_processing_timeout_ms = positive_limit(
            "DEPLOYMENT_PROCESSING_TIMEOUT_MS",
            get_u64(
                "DEPLOYMENT_PROCESSING_TIMEOUT_MS",
                crate::upload_limits::DEFAULT_DEPLOYMENT_PROCESSING_TIMEOUT_MS,
            )?,
        )?;
        validate_upload_limits(max_in_flight_upload_bytes)?;

        let (livekit_api_key, livekit_api_secret, livekit_configured) = resolve_livekit_env(
            env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
            env_bool("LIVEKIT_ALLOW_DEV_CREDS", false),
        )?;

        let http_base_url = env::var("HTTP_BASE_URL")
            .unwrap_or_else(|_| format!("http://127.0.0.1:{}", http_port))
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port,
            database_url: required("WORLDS_PG_CONNECTION_STRING")?,
            http_base_url,
            network_id: get_int("NETWORK_ID", 1)?,
            squid_database_url: env::var("SQUID_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            global_scenes_urn: env::var("GLOBAL_SCENES_URN").ok().filter(|s| !s.is_empty()),
            content_public_url: required_endpoint("CONTENT_PUBLIC_URL")?,
            lambdas_public_url: required_endpoint("LAMBDAS_PUBLIC_URL")?,
            livekit_host: env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string()),
            livekit_ws_url: env::var("LIVEKIT_WS_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "wss://{}",
                        env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string())
                    )
                }),
            livekit_api_key,
            livekit_api_secret,
            livekit_configured,
            livekit_webhook_key: env::var("LIVEKIT_WEBHOOK_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            max_users_per_world: get_int("MAX_USERS_PER_WORLD", 100)?,
            contents_dir: std::path::PathBuf::from(
                env::var("WORLDS_CONTENT_DIR")
                    .unwrap_or_else(|_| "./data/worlds/contents".to_string()),
            ),
            contents_upstream_url: optional_endpoint("CONTENTS_UPSTREAM_URL")
                .map(|s| s.trim_end_matches('/').to_string()),
            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string()),
            comms_gatekeeper_auth_token: env::var("COMMS_GATEKEEPER_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            denylist_json_url: env::var("DENYLIST_JSON_URL").ok().filter(|s| !s.is_empty()),
            dcl_lists_url: env::var("DCL_LISTS_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string()),
            admin_token: env::var("CATALYRST_WORLDS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            max_in_flight_upload_bytes,
            max_concurrent_uploads,
            max_in_flight_upload_files,
            multipart_upload_timeout_ms,
            deployment_processing_timeout_ms,
            federation: crate::fed::config::WorldsFedConfig::from_env()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_knobs_reject_zero_with_upstream_message_shape() {
        assert_eq!(positive_limit("MAX_CONCURRENT_UPLOADS", 40).unwrap(), 40);
        let err = positive_limit("MAX_CONCURRENT_UPLOADS", 0).unwrap_err();
        assert_eq!(
            err.to_string(),
            "MAX_CONCURRENT_UPLOADS must be a positive integer, got 0"
        );
    }

    #[test]
    fn livekit_placeholder_creds_are_treated_as_unset() {
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

        let (k, s, configured) =
            resolve_livekit_env("APIabc".into(), "supersecret".into(), false).unwrap();
        assert_eq!((k.as_str(), s.as_str()), ("APIabc", "supersecret"));
        assert!(configured);
    }

    #[test]
    fn byte_budget_must_cover_the_deploy_payload_cap() {
        let max_upload = crate::handlers::deploy::MAX_UPLOAD_SIZE_BYTES as u64;
        assert!(validate_upload_limits(max_upload).is_ok());
        assert!(validate_upload_limits(max_upload + 1).is_ok());
        let err = validate_upload_limits(max_upload - 1).unwrap_err();
        assert!(
            err.to_string()
                .contains("must be greater than or equal to maxSizeInBytes"),
            "unexpected message: {err}"
        );
    }
}
