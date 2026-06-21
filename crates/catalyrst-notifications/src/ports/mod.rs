pub mod email;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::EmailConfig;
use crate::http::ApiError;
use email::{EmailSender, EmailSource};

#[derive(Debug, Serialize)]
pub struct NotificationItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
    pub address: String,
    pub timestamp: i64,
    pub read: bool,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Json,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionDetails {
    #[serde(default)]
    pub ignore_all_email: bool,
    #[serde(default)]
    pub ignore_all_in_app: bool,
    #[serde(default)]
    pub message_type: Json,
}

pub const NOTIFICATION_TYPES: &[&str] = &[
    "badge_granted",
    "bid_accepted",
    "bid_received",
    "events_started",
    "events_starts_soon",
    "event_created",
    "event_approved",
    "event_rejected",
    "event_deleted",
    "governance_announcement",
    "governance_authored_proposal_finished",
    "governance_coauthor_requested",
    "governance_cliff_ended",
    "governance_new_comment_on_project_update",
    "governance_new_comment_on_proposal",
    "governance_proposal_enacted",
    "governance_voting_ended_voter",
    "governance_pitch_passed",
    "governance_tender_passed",
    "governance_whale_vote",
    "governance_voted_on_behalf",
    "item_sold",
    "item_published",
    "rental_ended",
    "rental_started",
    "reward_assignment",
    "reward_campaign_out_of_funds",
    "reward_campaign_gas_price_higher_than_expected",
    "reward_campaign_out_of_stock",
    "reward_delayed",
    "reward_in_progress",
    "royalties_earned",
    "social_service_friendship_request",
    "social_service_friendship_accepted",
    "worlds_access_restored",
    "worlds_access_restricted",
    "worlds_missing_resources",
    "worlds_permission_granted",
    "worlds_permission_revoked",
    "credits_goal_completed",
    "credits_on_demand_granted",
    "streaming_key_reset",
    "streaming_key_revoke",
    "streaming_key_expired",
    "streaming_time_exceeded",
    "streaming_place_updated",
    "credits_reminder_complete_goals",
    "credits_reminder_claim_credits",
    "credits_reminder_usage",
    "credits_reminder_usage_24_hours",
    "credits_reminder_do_not_miss_out",
    "credits_new_season_reminder",
    "referral_invited_users_accepted",
    "referral_new_tier_reached",
    "community_deleted",
    "community_deleted_content_violation",
    "community_renamed",
    "community_member_banned",
    "community_member_removed",
    "community_request_to_join_received",
    "community_request_to_join_accepted",
    "community_invite_received",
    "community_ownership_transferred",
    "community_post_added",
    "community_voice_chat_started",
    "user_banned_from_scene",
    "user_unbanned_from_scene",
    "transfer_received",
    "tip_received",
    "banned",
    "ban_warning",
    "ban_lifted",
];

pub fn validate_subscription_details(body: &Json) -> Result<(), String> {
    let obj = body
        .as_object()
        .ok_or_else(|| "must be an object".to_string())?;

    for key in obj.keys() {
        if key != "ignore_all_email" && key != "ignore_all_in_app" && key != "message_type" {
            return Err(format!("unexpected property {}", key));
        }
    }

    for field in ["ignore_all_email", "ignore_all_in_app"] {
        match obj.get(field) {
            Some(Json::Bool(_)) => {}
            Some(_) => return Err(format!("{} must be a boolean", field)),
            None => return Err(format!("{} is required", field)),
        }
    }

    let mt = obj
        .get("message_type")
        .ok_or_else(|| "message_type is required".to_string())?
        .as_object()
        .ok_or_else(|| "message_type must be an object".to_string())?;

    for key in mt.keys() {
        if !NOTIFICATION_TYPES.contains(&key.as_str()) {
            return Err(format!("unexpected message_type property {}", key));
        }
    }

    for ty in NOTIFICATION_TYPES {
        let channel = mt
            .get(*ty)
            .ok_or_else(|| format!("message_type.{} is required", ty))?
            .as_object()
            .ok_or_else(|| format!("message_type.{} must be an object", ty))?;
        for chan_field in ["email", "in_app"] {
            match channel.get(chan_field) {
                Some(Json::Bool(_)) => {}
                Some(_) => {
                    return Err(format!(
                        "message_type.{}.{} must be a boolean",
                        ty, chan_field
                    ))
                }
                None => return Err(format!("message_type.{}.{} is required", ty, chan_field)),
            }
        }
    }

    Ok(())
}

pub fn default_message_type() -> Json {
    let mut map = serde_json::Map::with_capacity(NOTIFICATION_TYPES.len());
    for ty in NOTIFICATION_TYPES {
        map.insert(
            (*ty).to_string(),
            serde_json::json!({ "email": true, "in_app": true }),
        );
    }
    Json::Object(map)
}

pub fn normalize_details(stored: &Json) -> Json {
    let obj = stored.as_object();
    let ignore_all_email = obj
        .and_then(|m| m.get("ignore_all_email"))
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let ignore_all_in_app = obj
        .and_then(|m| m.get("ignore_all_in_app"))
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let stored_mt = obj
        .and_then(|m| m.get("message_type"))
        .and_then(Json::as_object);

    let mut message_type = serde_json::Map::with_capacity(NOTIFICATION_TYPES.len());
    for ty in NOTIFICATION_TYPES {
        let entry = stored_mt.and_then(|m| m.get(*ty)).and_then(Json::as_object);
        let email = entry
            .and_then(|e| e.get("email"))
            .and_then(Json::as_bool)
            .unwrap_or(true);
        let in_app = entry
            .and_then(|e| e.get("in_app"))
            .and_then(Json::as_bool)
            .unwrap_or(true);
        message_type.insert(
            (*ty).to_string(),
            serde_json::json!({ "email": email, "in_app": in_app }),
        );
    }

    serde_json::json!({
        "ignore_all_email": ignore_all_email,
        "ignore_all_in_app": ignore_all_in_app,
        "message_type": Json::Object(message_type),
    })
}

impl Default for SubscriptionDetails {
    fn default() -> Self {
        Self {
            ignore_all_email: false,
            ignore_all_in_app: false,
            message_type: default_message_type(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Subscription {
    pub address: String,
    pub email: Option<String>,
    #[serde(rename = "unconfirmedEmail", skip_serializing_if = "Option::is_none")]
    pub unconfirmed_email: Option<String>,
    pub details: Json,
}

/// Outcome of a `set_email` call. The HTTP layer always replies 204 (no body);
/// this enum exists so business-rule rejections (cross-account uniqueness,
/// blacklist) surface as `ApiError` from the handler, while the no-op paths
/// (empty-email clear, same-confirmed-email short-circuit) skip email delivery.
pub enum SetEmailOutcome {
    /// Nothing further to do (cleared, or already confirmed to the same email).
    NoEmailSent,
    /// A pending row was written; deliver this confirmation.
    SendConfirmation { source: EmailSource, code: String },
}

#[derive(Clone)]
pub struct NotificationsComponent {
    pool: PgPool,
    pub email: EmailSender,
}

impl NotificationsComponent {
    pub fn new(pool: PgPool, email_cfg: EmailConfig) -> Self {
        Self {
            pool,
            email: EmailSender::new(email_cfg),
        }
    }

    pub async fn list(
        &self,
        address: &str,
        limit: i64,
        from: Option<i64>,
        only_unread: bool,
    ) -> Result<Vec<NotificationItem>, ApiError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, i64, bool, String, String, Json)>(
            r#"
            SELECT id, type, address, timestamp, read,
                   to_char(created_at AT TIME ZONE 'UTC',
                           'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                   to_char(
                       COALESCE(
                           CASE WHEN read_at IS NOT NULL
                                THEN to_timestamp(read_at / 1000.0) END,
                           created_at
                       ) AT TIME ZONE 'UTC',
                       'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'
                   ) AS updated_at,
                   metadata
            FROM notifications
            WHERE address = $1
              AND ($2::bigint IS NULL OR timestamp > $2)
              AND ($3 = FALSE OR read = FALSE)
            ORDER BY timestamp DESC
            LIMIT $4
            "#,
        )
        .bind(address)
        .bind(from)
        .bind(only_unread)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, kind, address, timestamp, read, created_at, updated_at, metadata)| {
                    NotificationItem {
                        id,
                        kind,
                        address,
                        timestamp,
                        read,
                        created_at,
                        updated_at,
                        metadata,
                    }
                },
            )
            .collect())
    }

    /// Insert one admin broadcast notification per target address.
    ///
    /// When `addresses` is `None` the broadcast targets every address known to
    /// the `subscriptions` table. `broadcast_id` is recorded on every inserted
    /// row (via `broadcast_address`) so the whole fan-out can be correlated.
    /// Returns the number of rows inserted.
    pub async fn broadcast(
        &self,
        broadcast_id: &str,
        kind: &str,
        metadata: &Json,
        addresses: Option<&[String]>,
    ) -> Result<u64, ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        let targets: Vec<String> = match addresses {
            Some(list) => list.iter().map(|a| a.to_lowercase()).collect(),
            None => {
                sqlx::query_scalar::<_, String>("SELECT address FROM subscriptions")
                    .fetch_all(&self.pool)
                    .await?
            }
        };

        if targets.is_empty() {
            return Ok(0);
        }

        // One row per address; a fresh UUID per row. UNNEST keeps this a single
        // round-trip regardless of audience size.
        let ids: Vec<Uuid> = (0..targets.len()).map(|_| Uuid::new_v4()).collect();
        let res = sqlx::query(
            r#"
            INSERT INTO notifications
                (id, address, type, metadata, broadcast_address, timestamp)
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::text[],
                ARRAY(SELECT $3::text FROM generate_series(1, array_length($1, 1))),
                ARRAY(SELECT $4::jsonb FROM generate_series(1, array_length($1, 1))),
                ARRAY(SELECT $5::text FROM generate_series(1, array_length($1, 1))),
                ARRAY(SELECT $6::bigint FROM generate_series(1, array_length($1, 1)))
            )
            "#,
        )
        .bind(&ids)
        .bind(&targets)
        .bind(kind)
        .bind(metadata)
        .bind(broadcast_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    pub async fn mark_read(&self, address: &str, ids: &[Uuid]) -> Result<u64, ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let res = sqlx::query(
            r#"
            UPDATE notifications
            SET read = TRUE, read_at = $3
            WHERE address = $1 AND id = ANY($2) AND read = FALSE
            "#,
        )
        .bind(address)
        .bind(ids)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn get_subscription(&self, address: &str) -> Result<Option<Subscription>, ApiError> {
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Json)>(
            r#"
            SELECT address, email, unconfirmed_email, details
            FROM subscriptions
            WHERE address = $1
            "#,
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(address, email, unconfirmed_email, details)| Subscription {
                address,
                email,
                unconfirmed_email,
                details: normalize_details(&details),
            },
        ))
    }

    pub async fn put_subscription_details(
        &self,
        address: &str,
        details: &Json,
    ) -> Result<Subscription, ApiError> {
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Json)>(
            r#"
            INSERT INTO subscriptions (address, details, updated_at)
            VALUES ($1, $2, now())
            ON CONFLICT (address) DO UPDATE
              SET details = EXCLUDED.details, updated_at = now()
            RETURNING address, email, unconfirmed_email, details
            "#,
        )
        .bind(address)
        .bind(details)
        .fetch_one(&self.pool)
        .await?;

        Ok(Subscription {
            address: row.0,
            email: row.1,
            unconfirmed_email: row.2,
            details: normalize_details(&row.3),
        })
    }

    /// Apply the upstream set-email state machine over a single transaction and
    /// return what (if anything) needs to be emailed. The HTTP layer performs
    /// delivery and replies 204 in all non-error cases.
    ///
    /// Branches (mirroring notifications-workers' set-email-handler):
    /// - empty email: clear any pending row + confirmed email, set
    ///   `ignore_all_email`, no email sent.
    /// - same confirmed email: short-circuit, no email sent.
    /// - cross-account: the email is already confirmed by a DIFFERENT address,
    ///   so reject with 400 (ApiError::BadRequest).
    /// - otherwise: upsert a pending `unconfirmed_emails` row with a fresh
    ///   32-char code and request delivery.
    pub async fn set_email(
        &self,
        address: &str,
        email: &str,
        is_credits_workflow: bool,
    ) -> Result<SetEmailOutcome, ApiError> {
        let address = address.to_lowercase();
        let email = email.trim();

        let mut tx = self.pool.begin().await?;

        // Empty email -> clear and disable all email notifications.
        if email.is_empty() {
            sqlx::query("DELETE FROM unconfirmed_emails WHERE address = $1")
                .bind(&address)
                .execute(&mut *tx)
                .await?;

            sqlx::query(
                r#"
                INSERT INTO subscriptions (address, email, details, updated_at)
                VALUES ($1, NULL, jsonb_build_object('ignore_all_email', true), now())
                ON CONFLICT (address) DO UPDATE
                  SET email = NULL,
                      unconfirmed_email = NULL,
                      email_confirmation_token = NULL,
                      details = jsonb_set(
                          COALESCE(subscriptions.details, '{}'::jsonb),
                          '{ignore_all_email}', 'true'::jsonb, true),
                      updated_at = now()
                "#,
            )
            .bind(&address)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            return Ok(SetEmailOutcome::NoEmailSent);
        }

        let email_lc = email.to_lowercase();

        // Already confirmed to the same email by this address -> short-circuit.
        let current_email: Option<String> =
            sqlx::query_scalar("SELECT email FROM subscriptions WHERE address = $1")
                .bind(&address)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();

        if current_email
            .as_deref()
            .map(|e| e.eq_ignore_ascii_case(email))
            .unwrap_or(false)
        {
            tx.commit().await?;
            return Ok(SetEmailOutcome::NoEmailSent);
        }

        // Cross-account uniqueness: the email is confirmed by a DIFFERENT
        // address -> reject.
        let taken_by_other: Option<String> = sqlx::query_scalar(
            "SELECT address FROM subscriptions WHERE lower(email) = $1 AND address <> $2 LIMIT 1",
        )
        .bind(&email_lc)
        .bind(&address)
        .fetch_optional(&mut *tx)
        .await?;

        if taken_by_other.is_some() {
            return Err(ApiError::bad_request("Email already registered"));
        }

        let source = EmailSource::from_credits_workflow(is_credits_workflow);
        let code = email::make_code();

        sqlx::query(
            r#"
            INSERT INTO unconfirmed_emails (address, email, code, source, created_at)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT (address) DO UPDATE
              SET email = EXCLUDED.email,
                  code = EXCLUDED.code,
                  source = EXCLUDED.source,
                  created_at = now()
            "#,
        )
        .bind(&address)
        .bind(email)
        .bind(&code)
        .bind(source.as_str())
        .execute(&mut *tx)
        .await?;

        // Ensure a subscriptions row exists and mirror the pending email +
        // credits flag for read-back via GET /subscription.
        sqlx::query(
            r#"
            INSERT INTO subscriptions
                (address, unconfirmed_email, email_confirmation_token, is_credits_workflow, details, updated_at)
            VALUES ($1, $2, $3, $4, '{}'::jsonb, now())
            ON CONFLICT (address) DO UPDATE
              SET unconfirmed_email = EXCLUDED.unconfirmed_email,
                  email_confirmation_token = EXCLUDED.email_confirmation_token,
                  is_credits_workflow = EXCLUDED.is_credits_workflow,
                  updated_at = now()
            "#,
        )
        .bind(&address)
        .bind(email)
        .bind(&code)
        .bind(is_credits_workflow)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(SetEmailOutcome::SendConfirmation { source, code })
    }

    /// Promote a pending email to confirmed when the presented code matches.
    /// Returns the resolved `EmailSource` (for the redirect the caller logs) on
    /// success, `None` when no matching pending row exists.
    pub async fn confirm_email(
        &self,
        address: &str,
        code: &str,
    ) -> Result<Option<EmailSource>, ApiError> {
        let address = address.to_lowercase();

        let mut tx = self.pool.begin().await?;

        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT email, source FROM unconfirmed_emails WHERE address = $1 AND code = $2",
        )
        .bind(&address)
        .bind(code)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((email, source)) = row else {
            tx.rollback().await?;
            return Ok(None);
        };

        sqlx::query(
            r#"
            INSERT INTO subscriptions (address, email, unconfirmed_email, email_confirmation_token, details, updated_at)
            VALUES ($1, $2, NULL, NULL, '{}'::jsonb, now())
            ON CONFLICT (address) DO UPDATE
              SET email = EXCLUDED.email,
                  unconfirmed_email = NULL,
                  email_confirmation_token = NULL,
                  updated_at = now()
            "#,
        )
        .bind(&address)
        .bind(&email)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM unconfirmed_emails WHERE address = $1")
            .bind(&address)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(Some(EmailSource::parse(&source)))
    }

    pub async fn is_opted_out(
        &self,
        address: &str,
        scope: &str,
        scope_id: &str,
    ) -> Result<bool, ApiError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
              SELECT 1 FROM subscription_opt_outs
              WHERE address = $1 AND scope = $2 AND scope_id = $3
            )
            "#,
        )
        .bind(address)
        .bind(scope)
        .bind(scope_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    pub async fn create_opt_out(
        &self,
        address: &str,
        scope: &str,
        scope_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO subscription_opt_outs (address, scope, scope_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (address, scope, scope_id) DO NOTHING
            "#,
        )
        .bind(address)
        .bind(scope)
        .bind(scope_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_opt_out(
        &self,
        address: &str,
        scope: &str,
        scope_id: &str,
    ) -> Result<bool, ApiError> {
        let res = sqlx::query(
            r#"
            DELETE FROM subscription_opt_outs
            WHERE address = $1 AND scope = $2 AND scope_id = $3
            "#,
        )
        .bind(address)
        .bind(scope)
        .bind(scope_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }
}
