use anyhow::{anyhow, Context, Result};
use std::env;

const DEFAULT_CAPTCHA_VERIFY_URL: &str = "https://hcaptcha.com/siteverify";

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,

    pub admin_token: Option<String>,

    pub captcha_secret: Option<String>,

    pub captcha_verify_url: String,

    pub stripe_secret_key: Option<String>,

    pub stripe_webhook_secret: Option<String>,

    pub credits_currency: String,

    pub market_base_url: String,

    pub price_base_url: String,

    pub economy_base_url: String,

    pub economy_admin_token: Option<String>,

    pub marketplace_markup_bps: i64,

    pub mana_price_max_staleness_secs: i64,

    pub checkout_fulfillment_mode: String,

    pub require_purchase_intent: bool,

    pub landiler_escrow_address: Option<String>,

    pub checkout_worker_interval_secs: u64,

    pub checkout_max_attempts: i32,

    pub usage_grants_database_url: Option<String>,

    pub progress_presence_database_url: Option<String>,

    pub escrow_lock_days: i32,

    pub mock_fulfillment: bool,

    pub mock_card: bool,
}

const DEFAULT_CREDITS_CURRENCY: &str = "usd";

const DEFAULT_MARKET_BASE_URL: &str = "http://127.0.0.1:5133";
const DEFAULT_PRICE_BASE_URL: &str = "http://127.0.0.1:5156";
const DEFAULT_ECONOMY_BASE_URL: &str = "http://127.0.0.1:5155";
const DEFAULT_MARKETPLACE_MARKUP_BPS: i64 = 2500;
const DEFAULT_MANA_PRICE_MAX_STALENESS_SECS: i64 = 300;
pub const DEFAULT_CHECKOUT_FULFILLMENT_MODE: &str = "secondary";
const DEFAULT_CHECKOUT_WORKER_INTERVAL_SECS: u64 = 5;
const DEFAULT_CHECKOUT_MAX_ATTEMPTS: i32 = 5;
const DEFAULT_ESCROW_LOCK_DAYS: i32 = 15;

pub const DEFAULT_REQUIRE_PURCHASE_INTENT: bool = true;

impl Config {
    pub fn from_env() -> Result<Self> {
        let cfg = Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5150)?,
            database_url: required("CREDITS_PG_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_CREDITS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            captcha_secret: env::var("CREDITS_CAPTCHA_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            captcha_verify_url: env::var("CREDITS_CAPTCHA_VERIFY_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_CAPTCHA_VERIFY_URL.to_string()),
            stripe_secret_key: env::var("STRIPE_SECRET_KEY").ok().filter(|s| !s.is_empty()),
            stripe_webhook_secret: env::var("STRIPE_WEBHOOK_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            credits_currency: env::var("CREDITS_CURRENCY")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_CREDITS_CURRENCY.to_string()),
            market_base_url: env_or("MARKET_BASE_URL", DEFAULT_MARKET_BASE_URL),
            price_base_url: env_or("PRICE_BASE_URL", DEFAULT_PRICE_BASE_URL),
            economy_base_url: env_or("ECONOMY_BASE_URL", DEFAULT_ECONOMY_BASE_URL),
            economy_admin_token: env::var("CATALYRST_ECONOMY_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            marketplace_markup_bps: get_i64(
                "MARKETPLACE_MARKUP_BPS",
                DEFAULT_MARKETPLACE_MARKUP_BPS,
            )?,
            mana_price_max_staleness_secs: get_i64(
                "MANA_PRICE_MAX_STALENESS_SECS",
                DEFAULT_MANA_PRICE_MAX_STALENESS_SECS,
            )?,
            checkout_fulfillment_mode: parse_fulfillment_mode(&env_or(
                "CHECKOUT_FULFILLMENT_MODE",
                DEFAULT_CHECKOUT_FULFILLMENT_MODE,
            ))?,
            require_purchase_intent: get_bool(
                "CREDITS_REQUIRE_PURCHASE_INTENT",
                DEFAULT_REQUIRE_PURCHASE_INTENT,
            )?,
            landiler_escrow_address: env::var("LANDILER_ESCROW_ADDRESS")
                .ok()
                .filter(|s| !s.is_empty()),
            checkout_worker_interval_secs: get_u64(
                "CHECKOUT_WORKER_INTERVAL_SECS",
                DEFAULT_CHECKOUT_WORKER_INTERVAL_SECS,
            )?,
            checkout_max_attempts: get_i32("CHECKOUT_MAX_ATTEMPTS", DEFAULT_CHECKOUT_MAX_ATTEMPTS)?,
            usage_grants_database_url: env::var("USAGE_GRANTS_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            progress_presence_database_url: env::var("PROGRESS_PRESENCE_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            escrow_lock_days: get_i32("ESCROW_LOCK_DAYS", DEFAULT_ESCROW_LOCK_DAYS)?,
            mock_fulfillment: get_bool("CREDITS_MOCK_FULFILLMENT", false)?,
            mock_card: get_bool("CREDITS_MOCK_CARD", false)?,
        };
        guard_admin_exposure(
            &cfg.http_host,
            cfg.admin_token.as_deref(),
            "CATALYRST_CREDITS_ADMIN_TOKEN",
        )?;
        Ok(cfg)
    }
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let h = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(h);
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

fn guard_admin_exposure(host: &str, admin_token: Option<&str>, token_env: &str) -> Result<()> {
    if is_loopback_host(host) {
        return Ok(());
    }
    if admin_token.is_none() {
        return Err(anyhow!(
            "refusing to start: HTTP_SERVER_HOST={host:?} is not a loopback address, which exposes \
             the loopback-only admin money endpoints to the network, and no {token_env} is set to \
             guard them. Bind 127.0.0.1 (front the public API with nginx) or set {token_env}."
        ));
    }
    tracing::warn!(
        host = %host,
        "HTTP_SERVER_HOST is non-loopback: the admin money surface is reachable from the network \
         and protected only by the bearer token. Prefer binding 127.0.0.1 behind nginx."
    );
    Ok(())
}

pub fn parse_fulfillment_mode(raw: &str) -> Result<String> {
    let mode = raw.trim().to_ascii_lowercase();
    match mode.as_str() {
        "secondary" | "primary" | "auto" => Ok(mode),
        other => Err(anyhow!(
            "invalid CHECKOUT_FULFILLMENT_MODE {other:?} (expected \"secondary\", \"primary\", or \"auto\")"
        )),
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn get_i64(key: &str, default: i64) -> Result<i64> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => s.parse::<i64>().with_context(|| format!("invalid {}", key)),
        _ => Ok(default),
    }
}

fn get_i32(key: &str, default: i32) -> Result<i32> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => s.parse::<i32>().with_context(|| format!("invalid {}", key)),
        _ => Ok(default),
    }
}

fn get_bool(key: &str, default: bool) -> Result<bool> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => match s.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(anyhow!("invalid {} (expected true/false)", key)),
        },
        _ => Ok(default),
    }
}

fn get_u64(key: &str, default: u64) -> Result<u64> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => s.parse::<u64>().with_context(|| format!("invalid {}", key)),
        _ => Ok(default),
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("missing required env var: {}", key))
}

fn get_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(s) => s.parse::<u16>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod intent_default_tests {
    use super::{get_bool, DEFAULT_REQUIRE_PURCHASE_INTENT};

    #[test]
    fn purchase_intent_enforcement_defaults_on() {
        const { assert!(DEFAULT_REQUIRE_PURCHASE_INTENT) };
        assert!(get_bool(
            "CREDITS_REQUIRE_PURCHASE_INTENT_TEST_UNSET_SENTINEL",
            DEFAULT_REQUIRE_PURCHASE_INTENT
        )
        .unwrap());
    }

    #[test]
    fn explicit_false_env_remains_the_escape_hatch() {
        let key = "CREDITS_REQUIRE_PURCHASE_INTENT_TEST_FALSE_SENTINEL";
        std::env::set_var(key, "false");
        assert!(!get_bool(key, DEFAULT_REQUIRE_PURCHASE_INTENT).unwrap());
        std::env::remove_var(key);
    }
}

#[cfg(test)]
mod fulfillment_mode_tests {
    use super::{parse_fulfillment_mode, DEFAULT_CHECKOUT_FULFILLMENT_MODE};

    #[test]
    fn default_mode_stays_secondary() {
        assert_eq!(DEFAULT_CHECKOUT_FULFILLMENT_MODE, "secondary");
        parse_fulfillment_mode(DEFAULT_CHECKOUT_FULFILLMENT_MODE).unwrap();
    }

    #[test]
    fn accepts_exactly_the_three_modes() {
        assert_eq!(parse_fulfillment_mode("secondary").unwrap(), "secondary");
        assert_eq!(parse_fulfillment_mode("primary").unwrap(), "primary");
        assert_eq!(parse_fulfillment_mode("auto").unwrap(), "auto");
        assert_eq!(parse_fulfillment_mode(" AUTO ").unwrap(), "auto");
        assert!(parse_fulfillment_mode("tertiary").is_err());
        assert!(parse_fulfillment_mode("").is_err());
        assert!(parse_fulfillment_mode("both").is_err());
    }
}

#[cfg(test)]
mod exposure_tests {
    use super::{guard_admin_exposure, is_loopback_host};

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host(" 127.0.0.1 "));
        assert!(is_loopback_host("127.5.6.7"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LocalHost"));

        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("::"));
        assert!(!is_loopback_host("203.0.113.10"));
        assert!(!is_loopback_host("10.0.0.5"));
        assert!(!is_loopback_host("example.com"));
        assert!(!is_loopback_host(""));
    }

    #[test]
    fn guard_allows_loopback_regardless_of_token() {
        assert!(guard_admin_exposure("127.0.0.1", None, "T").is_ok());
        assert!(guard_admin_exposure("localhost", Some("tok"), "T").is_ok());
    }

    #[test]
    fn guard_refuses_non_loopback_without_token() {
        assert!(guard_admin_exposure("0.0.0.0", None, "T").is_err());
        assert!(guard_admin_exposure("203.0.113.10", None, "T").is_err());
    }

    #[test]
    fn guard_allows_non_loopback_with_token() {
        assert!(guard_admin_exposure("0.0.0.0", Some("tok"), "T").is_ok());
    }
}
