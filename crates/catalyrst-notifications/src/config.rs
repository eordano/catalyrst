use anyhow::Result;
use catalyrst_envcfg::{get_port, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,

    pub admin_token: Option<String>,
    pub email: EmailConfig,

    pub content_database_url: Option<String>,
    pub social_database_url: Option<String>,
    pub squid_database_url: Option<String>,
    pub telemetry_database_url: Option<String>,
    pub shop_item_base_url: String,
}

#[derive(Clone, Default)]
pub struct EmailConfig {
    pub sendgrid_api_key: Option<String>,
    pub from_email: Option<String>,

    pub validate_email_template_id: Option<String>,

    pub validate_credits_email_template_id: Option<String>,

    pub account_base_url: String,

    pub marketplace_base_url: String,

    pub turnstile_secret_key: Option<String>,

    pub domain_blacklist: Vec<String>,
}

impl EmailConfig {
    fn from_env() -> Self {
        let domain_blacklist = env::var("EMAIL_DOMAIN_BLACKLIST")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|d| d.trim().to_lowercase())
                    .filter(|d| !d.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Self {
            sendgrid_api_key: opt("SENDGRID_API_KEY"),
            from_email: opt("SENDGRID_FROM_EMAIL"),
            validate_email_template_id: opt("SENDGRID_VALIDATE_EMAIL_TEMPLATE_ID"),
            validate_credits_email_template_id: opt("SENDGRID_VALIDATE_CREDITS_EMAIL_TEMPLATE_ID"),
            account_base_url: env::var("ACCOUNT_BASE_URL")
                .unwrap_or_else(|_| "https://account.decentraland.org".to_string()),
            marketplace_base_url: env::var("MARKETPLACE_BASE_URL")
                .unwrap_or_else(|_| "https://decentraland.org/marketplace".to_string()),
            turnstile_secret_key: opt("TURNSTILE_SECRET_KEY"),
            domain_blacklist,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5148)?,
            database_url: required("NOTIFICATIONS_PG_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_NOTIFICATIONS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            email: EmailConfig::from_env(),
            content_database_url: opt("CONTENT_PG_CONNECTION_STRING"),
            social_database_url: opt("SOCIAL_PG_CONNECTION_STRING"),
            squid_database_url: opt("SQUID_PG_CONNECTION_STRING"),
            telemetry_database_url: opt("TELEMETRY_PG_CONNECTION_STRING"),
            shop_item_base_url: env::var("SHOP_ITEM_BASE_URL")
                .unwrap_or_else(|_| "https://decentraland.org/marketplace/".to_string()),
        })
    }
}

fn opt(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}
