pub mod email;
pub mod seen;

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::EmailConfig;
use crate::http::ApiError;
use email::{EmailSender, EmailSource};
use seen::SeenDebounce;

macro_rules! list_sql {
    () => {
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
            "#
    };
}

const LIST_SQL: &str = list_sql!();

/// The reader_seen upsert rides along as a data-modifying CTE; GREATEST keeps
/// concurrent replicas from moving the mark backwards.
const TOUCH_AND_LIST_SQL: &str = concat!(
    r#"
            WITH seen AS (
                INSERT INTO notification_reader_seen (address, last_fetch_at)
                VALUES ($1, $5::bigint)
                ON CONFLICT (address) DO UPDATE
                  SET last_fetch_at = GREATEST(notification_reader_seen.last_fetch_at,
                                               EXCLUDED.last_fetch_at)
            )
    "#,
    list_sql!()
);

type ListRow = (Uuid, String, String, i64, bool, String, String, Json);

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct NotificationItem {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub id: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
    pub address: String,
    #[serde(serialize_with = "serialize_i64_as_str")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub timestamp: i64,
    pub read: bool,
    pub created_at: String,
    pub updated_at: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub metadata: Json,
}

fn serialize_i64_as_str<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct NotificationsListResponse {
    pub notifications: Vec<NotificationItem>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct MarkReadResponse {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated: u64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct OptOutResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct CommunityOptOutStatus {
    pub scope: String,
    #[serde(rename = "scopeId")]
    pub scope_id: String,
    #[serde(rename = "optedOut")]
    pub opted_out: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/")
)]
pub struct BroadcastResponse {
    pub ok: bool,
    #[serde(rename = "broadcastId")]
    pub broadcast_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub recipients: u64,
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
    "friend_first_wear",
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
        let Some(entry) = mt.get(*ty) else {
            if *ty == "friend_first_wear" {
                continue;
            }
            return Err(format!("message_type.{} is required", ty));
        };
        let channel = entry
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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "notifications/", rename_all = "camelCase")
)]
pub struct Subscription {
    pub address: String,
    pub email: Option<String>,
    #[serde(rename = "unconfirmedEmail")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub unconfirmed_email: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub details: Json,
}

pub enum SetEmailOutcome {
    NoEmailSent,

    SendConfirmation { source: EmailSource, code: String },
}

#[derive(Clone)]
pub struct NotificationsComponent {
    pool: PgPool,
    pub email: EmailSender,
    seen: Arc<SeenDebounce>,
}

impl NotificationsComponent {
    pub fn new(pool: PgPool, email_cfg: EmailConfig) -> Self {
        Self {
            pool,
            email: EmailSender::new(email_cfg),
            seen: Arc::new(SeenDebounce::default()),
        }
    }

    /// Lists and, at most once per address per `SEEN_WINDOW`, records the fetch in
    /// `notification_reader_seen` inside the same statement. The touch stays
    /// non-fatal: if the combined form fails the plain list runs once.
    pub async fn list_and_touch(
        &self,
        address: &str,
        limit: i64,
        from: Option<i64>,
        only_unread: bool,
    ) -> Result<Vec<NotificationItem>, ApiError> {
        let key = address.to_lowercase();
        if self.seen.claim(&key, Instant::now()) {
            let now_ms = chrono::Utc::now().timestamp_millis();
            match self
                .fetch_list(address, limit, from, only_unread, Some(now_ms))
                .await
            {
                Ok(items) => return Ok(items),
                Err(err) => {
                    self.seen.release(&key);
                    tracing::warn!(error = %err, "reader_seen touch failed; listing without it");
                }
            }
        }
        self.list(address, limit, from, only_unread).await
    }

    pub async fn list(
        &self,
        address: &str,
        limit: i64,
        from: Option<i64>,
        only_unread: bool,
    ) -> Result<Vec<NotificationItem>, ApiError> {
        self.fetch_list(address, limit, from, only_unread, None)
            .await
            .map_err(Into::into)
    }

    async fn fetch_list(
        &self,
        address: &str,
        limit: i64,
        from: Option<i64>,
        only_unread: bool,
        touch_ms: Option<i64>,
    ) -> Result<Vec<NotificationItem>, sqlx::Error> {
        let sql = if touch_ms.is_some() {
            TOUCH_AND_LIST_SQL
        } else {
            LIST_SQL
        };
        let mut query = sqlx::query_as::<_, ListRow>(sql)
            .bind(address)
            .bind(from)
            .bind(only_unread)
            .bind(limit);
        if let Some(now_ms) = touch_ms {
            query = query.bind(now_ms);
        }
        let rows = query.fetch_all(&self.pool).await?;

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

    pub async fn broadcast(
        &self,
        broadcast_id: &str,
        kind: &str,
        metadata: &Json,
        addresses: Option<&[String]>,
    ) -> Result<u64, ApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        let Some(list) = addresses else {
            let res = sqlx::query(
                r#"
                INSERT INTO notifications
                    (id, address, type, metadata, broadcast_address, timestamp)
                SELECT gen_random_uuid(), address, $1, $2, $3, $4
                FROM subscriptions
                "#,
            )
            .bind(kind)
            .bind(metadata)
            .bind(broadcast_id)
            .bind(now_ms)
            .execute(&self.pool)
            .await?;
            return Ok(res.rows_affected());
        };

        let targets: Vec<String> = list.iter().map(|a| a.to_lowercase()).collect();
        if targets.is_empty() {
            return Ok(0);
        }

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

    pub async fn set_email(
        &self,
        address: &str,
        email: &str,
        is_credits_workflow: bool,
    ) -> Result<SetEmailOutcome, ApiError> {
        let address = address.to_lowercase();
        let email = email.trim();

        if email.is_empty() {
            sqlx::query(
                r#"
                WITH cleared AS (
                    DELETE FROM unconfirmed_emails WHERE address = $1
                )
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
            .execute(&self.pool)
            .await?;
            return Ok(SetEmailOutcome::NoEmailSent);
        }

        let email_lc = email.to_lowercase();

        let (current_email, taken_by_other): (Option<String>, bool) = sqlx::query_as(
            r#"
            SELECT (SELECT email FROM subscriptions WHERE address = $2),
                   EXISTS (SELECT 1 FROM subscriptions
                           WHERE lower(email) = $1 AND address <> $2)
            "#,
        )
        .bind(&email_lc)
        .bind(&address)
        .fetch_one(&self.pool)
        .await?;

        if current_email
            .as_deref()
            .map(|e| e.eq_ignore_ascii_case(email))
            .unwrap_or(false)
        {
            return Ok(SetEmailOutcome::NoEmailSent);
        }

        if taken_by_other {
            return Err(ApiError::bad_request("Email already registered"));
        }

        let source = EmailSource::from_credits_workflow(is_credits_workflow);
        let code = email::make_code();

        sqlx::query(
            r#"
            WITH pending AS (
                INSERT INTO unconfirmed_emails (address, email, code, source, created_at)
                VALUES ($1, $2, $3, $4, now())
                ON CONFLICT (address) DO UPDATE
                  SET email = EXCLUDED.email,
                      code = EXCLUDED.code,
                      source = EXCLUDED.source,
                      created_at = now()
            )
            INSERT INTO subscriptions
                (address, unconfirmed_email, email_confirmation_token, is_credits_workflow, details, updated_at)
            VALUES ($1, $2, $3, $5, '{}'::jsonb, now())
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
        .bind(source.as_str())
        .bind(is_credits_workflow)
        .execute(&self.pool)
        .await?;

        Ok(SetEmailOutcome::SendConfirmation { source, code })
    }

    pub async fn confirm_email(
        &self,
        address: &str,
        code: &str,
    ) -> Result<Option<EmailSource>, ApiError> {
        let address = address.to_lowercase();

        let row: Option<(String,)> = sqlx::query_as(
            r#"
            WITH claimed AS (
                DELETE FROM unconfirmed_emails
                WHERE address = $1 AND code = $2
                RETURNING email, source
            ), promoted AS (
                INSERT INTO subscriptions
                    (address, email, unconfirmed_email, email_confirmation_token, details, updated_at)
                SELECT $1, email, NULL, NULL, '{}'::jsonb, now() FROM claimed
                ON CONFLICT (address) DO UPDATE
                  SET email = EXCLUDED.email,
                      unconfirmed_email = NULL,
                      email_confirmation_token = NULL,
                      updated_at = now()
            )
            SELECT source FROM claimed
            "#,
        )
        .bind(&address)
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(source,)| EmailSource::parse(&source)))
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

#[cfg(test)]
mod pg_tests {
    use super::*;
    use crate::config::EmailConfig;
    use sqlx::postgres::PgPoolOptions;

    struct Scratch {
        admin: PgPool,
        pool: PgPool,
        schema: String,
    }

    impl Scratch {
        async fn create(tag: &str) -> Option<Self> {
            let Ok(url) = std::env::var("CATALYRST_TEST_PG") else {
                eprintln!("SKIPPED {tag}: CATALYRST_TEST_PG unset");
                return None;
            };
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            let schema = format!("ntf_{tag}_{}_{nanos}", std::process::id());
            let admin = PgPoolOptions::new()
                .max_connections(2)
                .connect(&url)
                .await
                .expect("connect admin");
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
                .execute(&admin)
                .await
                .expect("create schema");
            let sep = if url.contains('?') { '&' } else { '?' };
            let scoped = format!("{url}{sep}options=-c%20search_path%3D{schema}");
            let pool = PgPoolOptions::new()
                .max_connections(4)
                .connect(&scoped)
                .await
                .expect("connect scoped");
            for sql in [
                include_str!("../../migrations/0001_initial.sql"),
                include_str!("../../migrations/0002_unconfirmed_emails.sql"),
                include_str!("../../migrations/0007_reader_seen.sql"),
            ] {
                sqlx::raw_sql(sql).execute(&pool).await.expect("migrate");
            }
            Some(Self {
                admin,
                pool,
                schema,
            })
        }

        fn component(&self) -> NotificationsComponent {
            NotificationsComponent::new(self.pool.clone(), EmailConfig::default())
        }

        async fn drop(self) {
            self.pool.close().await;
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP SCHEMA {} CASCADE",
                self.schema
            )))
            .execute(&self.admin)
            .await;
        }

        async fn seed_notification(&self, address: &str, ts: i64, read: bool) -> Uuid {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO notifications (id, address, type, timestamp, read) VALUES ($1, $2, 'bid_received', $3, $4)",
            )
            .bind(id)
            .bind(address)
            .bind(ts)
            .bind(read)
            .execute(&self.pool)
            .await
            .unwrap();
            id
        }

        async fn last_fetch_at(&self, address: &str) -> Option<i64> {
            sqlx::query_scalar(
                "SELECT last_fetch_at FROM notification_reader_seen WHERE address = $1",
            )
            .bind(address)
            .fetch_optional(&self.pool)
            .await
            .unwrap()
        }
    }

    const A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[tokio::test]
    async fn list_and_touch_records_the_fetch_once_per_window() {
        let Some(db) = Scratch::create("touch").await else {
            return;
        };
        let a_old = db.seed_notification(A, 100, true).await;
        let a_new = db.seed_notification(A, 200, false).await;
        db.seed_notification(B, 300, false).await;
        let c = db.component();

        let before = chrono::Utc::now().timestamp_millis();
        let items = c.list_and_touch(A, 50, None, false).await.unwrap();
        assert_eq!(
            items.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![a_new, a_old]
        );
        let first = db.last_fetch_at(A).await.expect("touched");
        assert!(first >= before);
        assert_eq!(db.last_fetch_at(B).await, None);

        sqlx::query("UPDATE notification_reader_seen SET last_fetch_at = 1 WHERE address = $1")
            .bind(A)
            .execute(&db.pool)
            .await
            .unwrap();
        let items = c.list_and_touch(A, 50, Some(150), true).await.unwrap();
        assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![a_new]);
        assert_eq!(
            db.last_fetch_at(A).await,
            Some(1),
            "second poll inside the window must not touch"
        );

        let plain = c.list(A, 50, Some(150), true).await.unwrap();
        let combined = c
            .fetch_list(A, 50, Some(150), true, Some(before))
            .await
            .unwrap();
        assert_eq!(
            plain
                .iter()
                .map(|i| (
                    i.id,
                    i.timestamp,
                    i.read,
                    i.created_at.clone(),
                    i.updated_at.clone()
                ))
                .collect::<Vec<_>>(),
            combined
                .iter()
                .map(|i| (
                    i.id,
                    i.timestamp,
                    i.read,
                    i.created_at.clone(),
                    i.updated_at.clone()
                ))
                .collect::<Vec<_>>(),
        );
        assert_eq!(db.last_fetch_at(A).await, Some(before));

        let future = before + 10_000_000;
        sqlx::query("UPDATE notification_reader_seen SET last_fetch_at = $2 WHERE address = $1")
            .bind(A)
            .bind(future)
            .execute(&db.pool)
            .await
            .unwrap();
        db.component()
            .list_and_touch(A, 50, None, false)
            .await
            .unwrap();
        assert_eq!(
            db.last_fetch_at(A).await,
            Some(future),
            "GREATEST never moves the mark back"
        );

        db.drop().await;
    }

    #[tokio::test]
    async fn confirm_email_promotes_and_clears_in_one_statement() {
        let Some(db) = Scratch::create("confirm").await else {
            return;
        };
        let c = db.component();
        sqlx::query(
            "INSERT INTO unconfirmed_emails (address, email, code, source) VALUES ($1, 'Me@Example.com', 'right-code', 'credits')",
        )
        .bind(A)
        .execute(&db.pool)
        .await
        .unwrap();

        assert_eq!(c.confirm_email(A, "wrong-code").await.unwrap(), None);
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM unconfirmed_emails")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(pending, 1, "a wrong code must not consume the pending row");
        assert!(c.get_subscription(A).await.unwrap().is_none());

        assert_eq!(
            c.confirm_email(&A.to_uppercase(), "right-code")
                .await
                .unwrap(),
            Some(EmailSource::Credits)
        );
        let sub = c.get_subscription(A).await.unwrap().expect("promoted");
        assert_eq!(sub.email.as_deref(), Some("Me@Example.com"));
        assert_eq!(sub.unconfirmed_email, None);
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM unconfirmed_emails")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(pending, 0);
        assert_eq!(c.confirm_email(A, "right-code").await.unwrap(), None);

        db.drop().await;
    }

    #[tokio::test]
    async fn set_email_flow_keeps_its_outcomes() {
        let Some(db) = Scratch::create("setemail").await else {
            return;
        };
        let c = db.component();

        let SetEmailOutcome::SendConfirmation { source, code } =
            c.set_email(A, " Foo@Example.com ", true).await.unwrap()
        else {
            panic!("a new email must send a confirmation")
        };
        assert_eq!(source, EmailSource::Credits);
        let (pending_email, pending_code, pending_source): (String, String, String) =
            sqlx::query_as("SELECT email, code, source FROM unconfirmed_emails WHERE address = $1")
                .bind(A)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(
            (
                pending_email.as_str(),
                pending_code.as_str(),
                pending_source.as_str()
            ),
            ("Foo@Example.com", code.as_str(), "credits")
        );
        let (unconfirmed, token, credits): (Option<String>, Option<String>, bool) = sqlx::query_as(
            "SELECT unconfirmed_email, email_confirmation_token, is_credits_workflow FROM subscriptions WHERE address = $1",
        )
        .bind(A)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            (unconfirmed.as_deref(), token.as_deref(), credits),
            (Some("Foo@Example.com"), Some(code.as_str()), true)
        );

        assert_eq!(
            c.confirm_email(A, &code).await.unwrap(),
            Some(EmailSource::Credits)
        );
        assert!(matches!(
            c.set_email(A, "foo@example.com", false).await.unwrap(),
            SetEmailOutcome::NoEmailSent
        ));
        assert!(
            c.set_email(B, "FOO@EXAMPLE.COM", false).await.is_err(),
            "taken by another address"
        );
        assert!(
            c.get_subscription(B).await.unwrap().is_none(),
            "a refused set-email writes nothing"
        );

        assert!(matches!(
            c.set_email(A, "", false).await.unwrap(),
            SetEmailOutcome::NoEmailSent
        ));
        let sub = c.get_subscription(A).await.unwrap().unwrap();
        assert_eq!(sub.email, None);
        assert_eq!(sub.details["ignore_all_email"], serde_json::json!(true));
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM unconfirmed_emails")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(pending, 0);

        db.drop().await;
    }

    #[tokio::test]
    async fn broadcast_to_everyone_is_a_single_insert_select() {
        let Some(db) = Scratch::create("broadcast").await else {
            return;
        };
        let c = db.component();
        assert_eq!(
            c.broadcast("bc-0", "bid_received", &serde_json::json!({}), None)
                .await
                .unwrap(),
            0
        );
        for addr in [A, B] {
            sqlx::query("INSERT INTO subscriptions (address) VALUES ($1)")
                .bind(addr)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        let meta = serde_json::json!({"title": "hi"});
        assert_eq!(
            c.broadcast("bc-1", "bid_received", &meta, None)
                .await
                .unwrap(),
            2
        );
        let rows: Vec<(String, String, Json, Option<String>)> = sqlx::query_as(
            "SELECT address, type, metadata, broadcast_address FROM notifications WHERE broadcast_address = 'bc-1' ORDER BY address",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, A);
        assert_eq!(rows[1].0, B);
        assert!(rows
            .iter()
            .all(|r| r.1 == "bid_received" && r.2 == meta && r.3.as_deref() == Some("bc-1")));
        assert_eq!(
            c.broadcast("bc-2", "bid_received", &meta, Some(&[A.to_uppercase()]))
                .await
                .unwrap(),
            1
        );
        let listed = c.list(A, 50, None, false).await.unwrap();
        assert_eq!(listed.len(), 2);

        db.drop().await;
    }
}
