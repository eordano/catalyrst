use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

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
                    return Err(format!("message_type.{}.{} must be a boolean", ty, chan_field))
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

#[derive(Clone)]
pub struct NotificationsComponent {
    pool: PgPool,
}

impl NotificationsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

    pub async fn get_subscription(
        &self,
        address: &str,
    ) -> Result<Option<Subscription>, ApiError> {
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

        Ok(row.map(|(address, email, unconfirmed_email, details)| Subscription {
            address,
            email,
            unconfirmed_email,
            details: normalize_details(&details),
        }))
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

    pub async fn set_email(
        &self,
        address: &str,
        email: &str,
        is_credits_workflow: bool,
    ) -> Result<Subscription, ApiError> {
        let token = Uuid::new_v4().to_string();
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Json)>(
            r#"
            INSERT INTO subscriptions
                (address, unconfirmed_email, email_confirmation_token, is_credits_workflow, details, updated_at)
            VALUES ($1, $2, $3, $4, '{}'::jsonb, now())
            ON CONFLICT (address) DO UPDATE
              SET unconfirmed_email = EXCLUDED.unconfirmed_email,
                  email_confirmation_token = EXCLUDED.email_confirmation_token,
                  is_credits_workflow = EXCLUDED.is_credits_workflow,
                  updated_at = now()
            RETURNING address, email, unconfirmed_email, details
            "#,
        )
        .bind(address)
        .bind(email)
        .bind(&token)
        .bind(is_credits_workflow)
        .fetch_one(&self.pool)
        .await?;

        Ok(Subscription {
            address: row.0,
            email: row.1,
            unconfirmed_email: row.2,
            details: normalize_details(&row.3),
        })
    }

    pub async fn confirm_email(
        &self,
        address: &str,
        code: &str,
    ) -> Result<Option<Subscription>, ApiError> {
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Json)>(
            r#"
            UPDATE subscriptions
            SET email = unconfirmed_email,
                unconfirmed_email = NULL,
                email_confirmation_token = NULL,
                updated_at = now()
            WHERE address = $1
              AND unconfirmed_email IS NOT NULL
              AND email_confirmation_token IS NOT NULL
              AND email_confirmation_token = $2
            RETURNING address, email, unconfirmed_email, details
            "#,
        )
        .bind(address)
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(address, email, unconfirmed_email, details)| Subscription {
            address,
            email,
            unconfirmed_email,
            details: normalize_details(&details),
        }))
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
