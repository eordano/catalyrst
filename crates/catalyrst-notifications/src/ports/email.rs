use std::sync::OnceLock;

use rand::RngExt;
use serde_json::json;

use crate::config::EmailConfig;
use crate::http::ApiError;

/// Charset + length of the confirmation code, matching upstream `makeId(32)`.
const CODE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
pub const CODE_LEN: usize = 32;

/// Generate a 32-char alphanumeric confirmation code (upstream `makeId`).
pub fn make_code() -> String {
    let mut rng = rand::rng();
    (0..CODE_LEN)
        .map(|_| {
            let idx = rng.random_range(0..CODE_CHARSET.len());
            CODE_CHARSET[idx] as char
        })
        .collect()
}

/// Which confirmation-email template/flow to render. Mirrors upstream's
/// VALIDATE_EMAIL vs VALIDATE_CREDITS_EMAIL split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSource {
    Account,
    Credits,
}

impl EmailSource {
    pub fn as_str(self) -> &'static str {
        match self {
            EmailSource::Account => "account",
            EmailSource::Credits => "credits",
        }
    }

    pub fn from_credits_workflow(is_credits: bool) -> Self {
        if is_credits {
            EmailSource::Credits
        } else {
            EmailSource::Account
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "credits" => EmailSource::Credits,
            _ => EmailSource::Account,
        }
    }
}

#[derive(Clone)]
pub struct EmailSender {
    cfg: EmailConfig,
}

impl EmailSender {
    pub fn new(cfg: EmailConfig) -> Self {
        Self { cfg }
    }

    /// True when a real SendGrid send is wired (api key + from + template).
    pub fn is_enabled(&self, source: EmailSource) -> bool {
        self.cfg.sendgrid_api_key.is_some()
            && self.cfg.from_email.is_some()
            && self.template_id(source).is_some()
    }

    /// Is the email domain on the configured blacklist? Lowercased compare.
    pub fn is_domain_blacklisted(&self, email: &str) -> bool {
        if self.cfg.domain_blacklist.is_empty() {
            return false;
        }
        match email.rsplit_once('@') {
            Some((_, domain)) => self
                .cfg
                .domain_blacklist
                .iter()
                .any(|d| d == &domain.to_lowercase()),
            None => false,
        }
    }

    pub fn turnstile_secret(&self) -> Option<&str> {
        self.cfg.turnstile_secret_key.as_deref()
    }

    fn template_id(&self, source: EmailSource) -> Option<&str> {
        match source {
            EmailSource::Account => self.cfg.validate_email_template_id.as_deref(),
            EmailSource::Credits => self.cfg.validate_credits_email_template_id.as_deref(),
        }
    }

    fn base_url(&self, source: EmailSource) -> &str {
        match source {
            EmailSource::Account => &self.cfg.account_base_url,
            EmailSource::Credits => &self.cfg.marketplace_base_url,
        }
    }

    /// Path segment of the confirm-email page for this source, matching the
    /// `account` repo route contract (locations.ts):
    /// - account  -> `/confirm-email-challenge/<code>` (unifiedEmailConfirmation;
    ///   the legacy `/confirm-email/<code>` route redirects here)
    /// - credits  -> `/credits-email-confirmed/<code>`
    fn confirm_path(source: EmailSource, code: &str) -> String {
        match source {
            EmailSource::Account => format!("/confirm-email-challenge/{code}"),
            EmailSource::Credits => format!("/credits-email-confirmed/{code}"),
        }
    }

    /// The confirm-email link the recipient clicks. The page is unprotected, so
    /// the address travels as a query param alongside the source (the
    /// UnifiedEmailConfirmation page reads `address` + `source` from the query).
    /// `address` is a normalized `0x`-prefixed hex eth address — query-safe — so
    /// it is interpolated raw, matching upstream's link construction.
    pub fn confirm_url(&self, source: EmailSource, address: &str, code: &str) -> String {
        format!(
            "{}{}?address={}&source={}",
            self.base_url(source).trim_end_matches('/'),
            Self::confirm_path(source, code),
            address,
            source.as_str()
        )
    }

    /// Render and send the confirmation email via SendGrid v3. Returns the
    /// SendGrid message id on success.
    ///
    /// When delivery is NOT configured this fails with a 500 rather than
    /// silently succeeding: the upstream feature is non-functional without a
    /// real send (the user never receives the code), so reporting success would
    /// strand the confirm loop. The caller surfaces this as a hard 5xx, never a
    /// silent 204.
    pub async fn send_confirmation(
        &self,
        source: EmailSource,
        to: &str,
        address: &str,
        code: &str,
    ) -> Result<Option<String>, ApiError> {
        let confirm_url = self.confirm_url(source, address, code);

        let (Some(api_key), Some(from), Some(template_id)) = (
            self.cfg.sendgrid_api_key.as_deref(),
            self.cfg.from_email.as_deref(),
            self.template_id(source),
        ) else {
            tracing::error!(
                to = %to,
                source = source.as_str(),
                confirm_url = %confirm_url,
                "SendGrid not configured; cannot send confirmation email"
            );
            return Err(ApiError::Internal(
                "email delivery is not configured".into(),
            ));
        };

        // SendGrid v3 dynamic-template payload. The template renders the link
        // from `confirmUrl`; `code`/`address` are passed for templates that
        // build their own CTA.
        let body = json!({
            "personalizations": [{
                "to": [{ "email": to }],
                "dynamic_template_data": {
                    "confirmUrl": confirm_url,
                    "code": code,
                    "address": address,
                    "source": source.as_str(),
                }
            }],
            "from": { "email": from },
            "template_id": template_id,
        });

        let resp = client()
            .post("https://api.sendgrid.com/v3/mail/send")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("sendgrid request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, detail = %detail, "SendGrid send failed");
            return Err(ApiError::Internal(
                "failed to send confirmation email".into(),
            ));
        }

        let message_id = resp
            .headers()
            .get("x-message-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        tracing::info!(to = %to, source = source.as_str(), ?message_id, "confirmation email sent");
        Ok(message_id)
    }
}

/// Verify a Cloudflare Turnstile token server-side via siteverify. Returns
/// `Ok(true)` on a verified token, `Ok(false)` when the token is rejected.
/// When no secret is configured returns `Ok(true)` (turnstile disabled).
pub async fn verify_turnstile(secret: Option<&str>, token: Option<&str>) -> Result<bool, ApiError> {
    let Some(secret) = secret else {
        return Ok(true);
    };
    // A secret is configured -> a token is required.
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return Ok(false);
    };

    let resp = client()
        .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("turnstile request failed: {e}")))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("turnstile decode failed: {e}")))?;

    Ok(body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build notifications email reqwest client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sender() -> EmailSender {
        EmailSender::new(EmailConfig {
            account_base_url: "https://account.decentraland.org".into(),
            marketplace_base_url: "https://decentraland.org/marketplace".into(),
            ..Default::default()
        })
    }

    const ADDR: &str = "0x1234567890abcdef1234567890abcdef12345678";
    const CODE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";

    // Account confirm links land on the `/confirm-email-challenge/<code>` route
    // (account repo locations.ts: unifiedEmailConfirmation), NOT the legacy
    // `/confirm-email/<code>` path.
    #[test]
    fn account_confirm_url_uses_challenge_route() {
        let url = sender().confirm_url(EmailSource::Account, ADDR, CODE);
        assert_eq!(
            url,
            format!(
                "https://account.decentraland.org/confirm-email-challenge/{CODE}?address={ADDR}&source=account"
            )
        );
        assert!(
            !url.contains("/confirm-email/"),
            "must not use legacy route"
        );
    }

    // Credits confirm links land on `/credits-email-confirmed/<code>`.
    #[test]
    fn credits_confirm_url_uses_credits_route() {
        let url = sender().confirm_url(EmailSource::Credits, ADDR, CODE);
        assert_eq!(
            url,
            format!(
                "https://decentraland.org/marketplace/credits-email-confirmed/{CODE}?address={ADDR}&source=credits"
            )
        );
    }

    // A trailing slash on the base URL must not double up before the route.
    #[test]
    fn base_url_trailing_slash_trimmed() {
        let s = EmailSender::new(EmailConfig {
            account_base_url: "https://account.decentraland.org/".into(),
            ..Default::default()
        });
        let url = s.confirm_url(EmailSource::Account, ADDR, CODE);
        assert!(url.starts_with("https://account.decentraland.org/confirm-email-challenge/"));
        assert!(!url.contains(".org//"));
    }

    // With no SendGrid config, a send must NOT silently succeed (which would let
    // the handler return a 204 with no email delivered) — it must hard-fail 5xx.
    #[tokio::test]
    async fn unconfigured_mailer_hard_fails() {
        let err = sender()
            .send_confirmation(EmailSource::Account, "user@example.com", ADDR, CODE)
            .await
            .expect_err("unconfigured mailer must error, never silently succeed");
        assert!(matches!(err, ApiError::Internal(_)));
    }
}
