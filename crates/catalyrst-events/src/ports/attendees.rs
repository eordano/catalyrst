use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::EventAttendeeRecord;

pub struct AttendeesComponent {
    pool: PgPool,
}

impl AttendeesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventAttendeeRecord>, ApiError> {
        let local: Vec<(String, Option<String>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT signer, signed_payload->>'user_name' AS user_name, signed_at \
             FROM event_attendance_local \
             WHERE event_id = $1 AND action = 'going' \
             ORDER BY signed_at DESC \
             LIMIT 500",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<EventAttendeeRecord> = Vec::new();
        for (signer, user_name, signed_at) in local {
            let key = signer.to_lowercase();
            if seen.insert(key) {
                out.push(EventAttendeeRecord {
                    event_id: event_id.to_string(),
                    user: signer,
                    user_name,
                    created_at: signed_at,
                });
            }
        }

        let row: Option<(Value, Option<DateTime<Utc>>)> =
            sqlx::query_as("SELECT raw, fetched_at FROM event WHERE id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;
        if let Some((raw, fetched_at)) = row {
            let cached_ts = raw
                .get("updated_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .or(fetched_at)
                .unwrap_or_else(Utc::now);
            if let Some(arr) = raw.get("latest_attendees").and_then(|v| v.as_array()) {
                for v in arr {
                    if let Some(addr) = v.as_str() {
                        if seen.insert(addr.to_lowercase()) {
                            out.push(EventAttendeeRecord {
                                event_id: event_id.to_string(),
                                user: addr.to_string(),
                                user_name: None,
                                created_at: cached_ts,
                            });
                        }
                    }
                }
            }
        }

        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        out.truncate(500);
        Ok(out)
    }

    pub async fn rsvp_going(
        &self,
        event_id: &str,
        signer: &str,
        user_name: Option<&str>,
        signed_payload: Value,
    ) -> Result<Vec<EventAttendeeRecord>, ApiError> {
        let payload = if signed_payload.is_null() {
            json!({ "user_name": user_name })
        } else {
            signed_payload
        };
        sqlx::query(
            "INSERT INTO event_attendance_local \
               (event_id, signer, signed_payload, action, signed_at) \
             VALUES ($1, $2, $3, 'going', now()) \
             ON CONFLICT (event_id, signer) DO UPDATE \
               SET signed_payload = EXCLUDED.signed_payload, \
                   action = 'going', \
                   signed_at = now()",
        )
        .bind(event_id)
        .bind(signer.to_lowercase())
        .bind(payload)
        .execute(&self.pool)
        .await?;
        self.list_for_event(event_id).await
    }

    pub async fn rsvp_cancel(
        &self,
        event_id: &str,
        signer: &str,
    ) -> Result<Vec<EventAttendeeRecord>, ApiError> {
        sqlx::query("DELETE FROM event_attendance_local WHERE event_id = $1 AND signer = $2")
            .bind(event_id)
            .bind(signer.to_lowercase())
            .execute(&self.pool)
            .await?;
        self.list_for_event(event_id).await
    }
}
