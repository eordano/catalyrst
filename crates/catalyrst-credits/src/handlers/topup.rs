use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::ports::economy::{verify_mana_payment, ManaPaymentVerification};
use crate::ports::pricing::CREDIT_USD;
use crate::AppState;

const QUOTE_MAX_CREDITS: i64 = 100_000;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct ManaTopupOut {
    #[serde(rename = "creditsGranted")]
    credits_granted: String,
    available: String,
    #[serde(rename = "txHash")]
    tx_hash: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct ManaTopupQuoteOut {
    credits: String,
    #[serde(rename = "weiSuggested")]
    wei_suggested: String,
    #[serde(rename = "manaUsd")]
    mana_usd: String,
}

#[derive(Debug, Deserialize)]
pub struct ManaTopupBody {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
}

fn validate_tx_hash(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    let hex = s.strip_prefix("0x").unwrap_or("");
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request(
            "invalid txHash: expected 0x + 64 hex chars",
        ));
    }
    Ok(s.to_lowercase())
}

fn topup_idempotency_key(tx_hash_lowercase: &str) -> String {
    format!("mana-topup:{tx_hash_lowercase}")
}

#[derive(Debug, PartialEq, Eq)]
enum TopupDecision {
    Pending,
    Granted { value_wei: String },
}

fn decide(verification: ManaPaymentVerification, signer: &str) -> Result<TopupDecision, ApiError> {
    match verification {
        ManaPaymentVerification::Pending => Ok(TopupDecision::Pending),
        ManaPaymentVerification::Reverted => Err(ApiError::unprocessable(
            "the transaction reverted on-chain; no MANA was transferred",
        )),
        ManaPaymentVerification::NoPayment => Err(ApiError::unprocessable(
            "the transaction is confirmed but contains no MANA transfer to the top-up address",
        )),
        ManaPaymentVerification::Confirmed {
            from, value_wei, ..
        } => {
            if from.to_lowercase() != signer.to_lowercase() {
                return Err(ApiError::forbidden(
                    "the MANA payment was sent by a different wallet; only the payer can claim this top-up",
                ));
            }
            Ok(TopupDecision::Granted { value_wei })
        }
    }
}

async fn credits_for_wei(
    pool: &sqlx::PgPool,
    value_wei: &str,
    mana_usd: &str,
) -> Result<String, ApiError> {
    let row = sqlx::query(
        "SELECT floor(($1::numeric / 1e18) * $2::numeric / $3::numeric)::text AS credits",
    )
    .bind(value_wei)
    .bind(mana_usd)
    .bind(CREDIT_USD)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<String, _>("credits"))
}

async fn quote_wei(pool: &sqlx::PgPool, credits: i64, mana_usd: &str) -> Result<String, ApiError> {
    let row = sqlx::query(
        "SELECT ceil(ceil($1::numeric * $2::numeric / $3::numeric * 1e18) * 102 / 100)::text \
         AS wei",
    )
    .bind(credits)
    .bind(CREDIT_USD)
    .bind(mana_usd)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<String, _>("wei"))
}

pub async fn mana_topup(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ManaTopupBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let signer = signer_from(&headers, "post", "/topup/mana")?;
    let Json(body) = body.map_err(|e| ApiError::bad_request(e.body_text()))?;
    let tx_hash = validate_tx_hash(&body.tx_hash)?;
    let idem = topup_idempotency_key(&tx_hash);

    if let Some(prior) = state.credits.find_grant_by_idempotency_key(&idem).await? {
        if prior.address.to_lowercase() != signer.to_lowercase() {
            return Err(ApiError::forbidden(
                "this transaction already granted Credits to a different wallet",
            ));
        }
        tracing::info!(tx_hash = %tx_hash, signer = %signer, "MANA top-up replay served");
        return Ok(Json(ManaTopupOut {
            credits_granted: prior.amount,
            available: prior.available,
            tx_hash,
        })
        .into_response());
    }

    let Some(token) = state.economy_admin_token.as_deref() else {
        return Err(ApiError::not_implemented(
            "MANA top-ups are disabled (CATALYRST_ECONOMY_ADMIN_TOKEN unset)",
        ));
    };

    let verification = verify_mana_payment(
        &state.economy_http,
        &state.economy_base_url,
        token,
        &tx_hash,
    )
    .await?;

    let value_wei = match decide(verification, &signer)? {
        TopupDecision::Pending => {
            return Ok((StatusCode::ACCEPTED, Json(json!({ "status": "pending" }))).into_response());
        }
        TopupDecision::Granted { value_wei } => value_wei,
    };

    let mana_usd = state.pricing.fetch_mana_usd().await?;
    let credits = credits_for_wei(&state.credits.pool, &value_wei, &mana_usd).await?;
    if !crate::ports::pricing::charge_is_positive(&credits) {
        return Err(ApiError::unprocessable(format!(
            "the MANA payment ({value_wei} wei at {mana_usd} USD/MANA) is worth less than 1 whole Credit"
        )));
    }

    let detail = json!({
        "source": "mana-topup",
        "txHash": tx_hash,
        "valueWei": value_wei,
        "manaUsd": mana_usd,
    });
    let outcome = state
        .credits
        .admin_grant_credits(
            &signer,
            &credits,
            "purchase",
            Some("MANA top-up"),
            Some("mana-topup"),
            Some(&idem),
            &detail,
        )
        .await?;

    tracing::info!(
        tx_hash = %tx_hash,
        signer = %signer,
        credits = %outcome.applied,
        value_wei = %value_wei,
        replayed = outcome.replayed,
        "MANA top-up granted"
    );

    Ok(Json(ManaTopupOut {
        credits_granted: outcome.applied,
        available: outcome.available,
        tx_hash,
    })
    .into_response())
}

#[derive(Debug, Deserialize)]
pub struct QuoteParams {
    #[serde(default)]
    credits: String,
}

fn validate_quote_credits(raw: &str) -> Result<i64, ApiError> {
    let n: i64 = raw
        .trim()
        .parse()
        .map_err(|_| ApiError::bad_request("credits must be a whole number"))?;
    if n <= 0 {
        return Err(ApiError::bad_request("credits must be positive"));
    }
    if n > QUOTE_MAX_CREDITS {
        return Err(ApiError::bad_request(format!(
            "credits must be at most {QUOTE_MAX_CREDITS}"
        )));
    }
    Ok(n)
}

pub async fn mana_topup_quote(
    State(state): State<AppState>,
    Query(params): Query<QuoteParams>,
) -> Result<Json<ManaTopupQuoteOut>, ApiError> {
    let credits = validate_quote_credits(&params.credits)?;
    let mana_usd = state.pricing.fetch_mana_usd().await?;
    let wei_suggested = quote_wei(&state.credits.pool, credits, &mana_usd).await?;
    Ok(Json(ManaTopupQuoteOut {
        credits: credits.to_string(),
        wei_suggested,
        mana_usd,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNER: &str = "0x2222222222222222222222222222222222222222";
    const OTHER: &str = "0x3333333333333333333333333333333333333333";

    fn confirmed(from: &str, wei: &str) -> ManaPaymentVerification {
        ManaPaymentVerification::Confirmed {
            from: from.into(),
            to: "0x1111111111111111111111111111111111111111".into(),
            value_wei: wei.into(),
        }
    }

    #[test]
    fn wire_identity_mana_topup_out() {
        let out = ManaTopupOut {
            credits_granted: "250".into(),
            available: "300".into(),
            tx_hash: "0xabc".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "creditsGranted": "250",
                "available": "300",
                "txHash": "0xabc",
            })
        );
    }

    #[test]
    fn wire_identity_mana_topup_quote_out() {
        let out = ManaTopupQuoteOut {
            credits: "250".into(),
            wei_suggested: "102000000000000000000".into(),
            mana_usd: "0.25".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "credits": "250",
                "weiSuggested": "102000000000000000000",
                "manaUsd": "0.25",
            })
        );
    }

    #[test]
    fn pending_maps_to_a_202_marker() {
        assert_eq!(
            decide(ManaPaymentVerification::Pending, SIGNER).unwrap(),
            TopupDecision::Pending
        );
    }

    #[test]
    fn reverted_and_no_payment_are_422() {
        for v in [
            ManaPaymentVerification::Reverted,
            ManaPaymentVerification::NoPayment,
        ] {
            let err = decide(v, SIGNER).unwrap_err();
            assert!(matches!(err, ApiError::Unprocessable(_)), "got {err:?}");
        }
    }

    #[test]
    fn someone_elses_payment_is_403() {
        let err = decide(confirmed(OTHER, "1000000000000000000"), SIGNER).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)), "got {err:?}");
    }

    #[test]
    fn own_payment_grants_case_insensitively() {
        let got = decide(
            confirmed(&SIGNER.to_uppercase().replace("0X", "0x"), "42"),
            SIGNER,
        )
        .unwrap();
        assert_eq!(
            got,
            TopupDecision::Granted {
                value_wei: "42".into()
            }
        );
    }

    #[test]
    fn tx_hash_validation_and_idempotency_key() {
        let mixed = format!("0x{}", "Ab".repeat(32));
        let lower = mixed.to_lowercase();
        assert_eq!(validate_tx_hash(&mixed).unwrap(), lower);
        assert_eq!(
            topup_idempotency_key(&validate_tx_hash(&mixed).unwrap()),
            topup_idempotency_key(&validate_tx_hash(&lower).unwrap()),
        );
        assert_eq!(topup_idempotency_key(&lower), format!("mana-topup:{lower}"));
        assert!(validate_tx_hash("0x1234").is_err());
        assert!(validate_tx_hash(&format!("0x{}", "g".repeat(64))).is_err());
        assert!(validate_tx_hash(&"a".repeat(66)).is_err());
        assert!(validate_tx_hash("").is_err());
    }

    #[test]
    fn quote_credits_validation() {
        assert_eq!(validate_quote_credits("1").unwrap(), 1);
        assert_eq!(validate_quote_credits(" 250 ").unwrap(), 250);
        assert_eq!(validate_quote_credits("100000").unwrap(), 100_000);
        assert!(validate_quote_credits("0").is_err());
        assert!(validate_quote_credits("-5").is_err());
        assert!(validate_quote_credits("2.5").is_err());
        assert!(validate_quote_credits("").is_err());
        assert!(validate_quote_credits("100001").is_err());
        assert!(validate_quote_credits("nope").is_err());
    }
}
