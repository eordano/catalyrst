use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::EventAttendeeRecord;

pub struct AttendeesComponent {
    pool: PgPool,
}

const LOCAL_LIMIT: i64 = 500;

/// The local RSVPs (`{local}` supplies them so a write CTE can splice its own row in)
/// and the mirrored event ride in one row: `$1` is always the event id.
fn list_sql(local: &str) -> String {
    format!(
        "SELECT a.signers, a.names, a.times, e.raw, e.fetched_at \
         FROM (SELECT coalesce(array_agg(x.signer ORDER BY x.signed_at DESC), '{{}}') AS signers, \
                      coalesce(array_agg(x.user_name ORDER BY x.signed_at DESC), '{{}}') AS names, \
                      coalesce(array_agg(x.signed_at ORDER BY x.signed_at DESC), '{{}}') AS times \
               FROM ({local} ORDER BY signed_at DESC LIMIT {LOCAL_LIMIT}) x) a \
         LEFT JOIN event e ON e.id = $1"
    )
}

const LOCAL_GOING: &str = "SELECT signer, signed_payload->>'user_name' AS user_name, signed_at \
     FROM event_attendance_local WHERE event_id = $1 AND action = 'going'";

type ListRow = (
    Vec<String>,
    Vec<Option<String>>,
    Vec<DateTime<Utc>>,
    Option<Value>,
    Option<DateTime<Utc>>,
);

fn to_records(event_id: &str, row: ListRow) -> Vec<EventAttendeeRecord> {
    let (signers, names, times, raw, fetched_at) = row;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<EventAttendeeRecord> = Vec::new();
    for (i, signer) in signers.into_iter().enumerate() {
        if seen.insert(signer.to_lowercase()) {
            out.push(EventAttendeeRecord {
                event_id: event_id.to_string(),
                user: signer,
                user_name: names.get(i).cloned().flatten(),
                created_at: times.get(i).copied().unwrap_or_else(Utc::now),
            });
        }
    }

    if let Some(raw) = raw {
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
    out.truncate(LOCAL_LIMIT as usize);
    out
}

impl AttendeesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventAttendeeRecord>, ApiError> {
        let row: ListRow = sqlx::query_as(sqlx::AssertSqlSafe(list_sql(LOCAL_GOING)))
            .bind(event_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(to_records(event_id, row))
    }

    /// The upsert and the reply share a statement: the CTE snapshot cannot see the
    /// written row, so it is spliced in from RETURNING and the signer's stale row left out.
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
        let sql = format!(
            "WITH up AS ( \
               INSERT INTO event_attendance_local \
                 (event_id, signer, signed_payload, action, signed_at) \
               VALUES ($1, $2, $3, 'going', now()) \
               ON CONFLICT (event_id, signer) DO UPDATE \
                 SET signed_payload = EXCLUDED.signed_payload, \
                     action = 'going', \
                     signed_at = now() \
               RETURNING signer, signed_payload->>'user_name' AS user_name, signed_at \
             ) {}",
            list_sql(
                "SELECT signer, user_name, signed_at FROM up \
                 UNION ALL \
                 SELECT signer, signed_payload->>'user_name' AS user_name, signed_at \
                 FROM event_attendance_local \
                 WHERE event_id = $1 AND action = 'going' AND signer <> $2"
            )
        );
        let row: ListRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(event_id)
            .bind(signer.to_lowercase())
            .bind(payload)
            .fetch_one(&self.pool)
            .await?;
        Ok(to_records(event_id, row))
    }

    pub async fn rsvp_cancel(
        &self,
        event_id: &str,
        signer: &str,
    ) -> Result<Vec<EventAttendeeRecord>, ApiError> {
        let sql = format!(
            "WITH gone AS ( \
               DELETE FROM event_attendance_local WHERE event_id = $1 AND signer = $2 \
             ) {}",
            list_sql(
                "SELECT signer, signed_payload->>'user_name' AS user_name, signed_at \
                 FROM event_attendance_local \
                 WHERE event_id = $1 AND action = 'going' AND signer <> $2"
            )
        );
        let row: ListRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(event_id)
            .bind(signer.to_lowercase())
            .fetch_one(&self.pool)
            .await?;
        Ok(to_records(event_id, row))
    }
}
