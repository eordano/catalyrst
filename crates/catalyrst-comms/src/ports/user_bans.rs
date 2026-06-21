use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

/// Serializes `DateTime<Utc>` as a fixed-3-digit millisecond ISO-8601 string,
/// byte-identical to JavaScript's `Date.prototype.toISOString()`
/// (`YYYY-MM-DDTHH:mm:ss.sssZ`). Upstream comms-gatekeeper stores these
/// timestamps as Postgres `TIMESTAMP`s and the node-postgres JSON serializer
/// emits them through `Date.toISOString`, so a client diffing the wire bytes
/// sees the same always-`.sss`-precision format. chrono's default
/// `AutoSi`/RFC3339 serializer instead emits variable subsecond precision
/// (0/6/9 digits, dot omitted on zero) which diverges.
pub mod ms_iso {
    use super::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
    }

    /// `Option<DateTime<Utc>>` variant: `None` serializes as JSON `null`,
    /// matching upstream's nullable `expiresAt`/`liftedAt` columns.
    pub mod option {
        use super::super::{DateTime, SecondsFormat, Utc};
        use serde::Serializer;

        pub fn serialize<S>(dt: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match dt {
                Some(dt) => s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true)),
                None => s.serialize_none(),
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UserBan {
    pub id: String,
    #[serde(rename = "bannedAddress")]
    pub banned_address: String,
    #[serde(rename = "bannedBy")]
    pub banned_by: String,
    pub reason: String,
    #[serde(rename = "customMessage")]
    pub custom_message: Option<String>,
    #[serde(rename = "bannedAt", serialize_with = "ms_iso::serialize")]
    pub banned_at: DateTime<Utc>,
    #[serde(rename = "expiresAt", serialize_with = "ms_iso::option::serialize")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedAt", serialize_with = "ms_iso::option::serialize")]
    pub lifted_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedBy")]
    pub lifted_by: Option<String>,
    #[serde(rename = "createdAt", serialize_with = "ms_iso::serialize")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserWarning {
    pub id: String,
    #[serde(rename = "warnedAddress")]
    pub warned_address: String,
    #[serde(rename = "warnedBy")]
    pub warned_by: String,
    pub reason: String,
    #[serde(rename = "warnedAt", serialize_with = "ms_iso::serialize")]
    pub warned_at: DateTime<Utc>,
    #[serde(rename = "createdAt", serialize_with = "ms_iso::serialize")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct BanStatus {
    #[serde(rename = "isBanned")]
    pub is_banned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ban: Option<UserBan>,
}

type BanRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    NaiveDateTime,
    Option<NaiveDateTime>,
    Option<NaiveDateTime>,
    Option<String>,
    NaiveDateTime,
);

type WarningRow = (Uuid, String, String, String, NaiveDateTime, NaiveDateTime);

const BAN_SELECT_FIELDS: &str =
    "id, banned_address, banned_by, reason, custom_message, banned_at, expires_at, lifted_at, lifted_by, created_at";

const WARNING_SELECT_FIELDS: &str = "id, warned_address, warned_by, reason, warned_at, created_at";

fn ban_from_row(row: BanRow) -> UserBan {
    let (
        id,
        banned_address,
        banned_by,
        reason,
        custom_message,
        banned_at,
        expires_at,
        lifted_at,
        lifted_by,
        created_at,
    ) = row;
    UserBan {
        id: id.to_string(),
        banned_address,
        banned_by,
        reason,
        custom_message,
        banned_at: DateTime::from_naive_utc_and_offset(banned_at, Utc),
        expires_at: expires_at.map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        lifted_at: lifted_at.map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        lifted_by,
        created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
    }
}

fn warning_from_row(row: WarningRow) -> UserWarning {
    let (id, warned_address, warned_by, reason, warned_at, created_at) = row;
    UserWarning {
        id: id.to_string(),
        warned_address,
        warned_by,
        reason,
        warned_at: DateTime::from_naive_utc_and_offset(warned_at, Utc),
        created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
    }
}

pub struct CreateBan {
    pub banned_address: String,
    pub banned_by: String,
    pub reason: String,
    pub custom_message: Option<String>,
    pub duration_ms: Option<i64>,
}

pub struct CreateWarning {
    pub warned_address: String,
    pub warned_by: String,
    pub reason: String,
}

#[derive(Debug)]
pub enum BanWriteError {
    AlreadyBanned(String),
    Db(ApiError),
}

#[derive(Debug)]
pub enum LiftError {
    NotFound(String),
    Db(ApiError),
}

impl From<sqlx::Error> for BanWriteError {
    fn from(e: sqlx::Error) -> Self {
        BanWriteError::Db(ApiError::from(e))
    }
}

impl From<sqlx::Error> for LiftError {
    fn from(e: sqlx::Error) -> Self {
        LiftError::Db(ApiError::from(e))
    }
}

pub struct UserBansComponent {
    pool: PgPool,
}

impl UserBansComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn is_banned(&self, address: &str) -> Result<bool, ApiError> {
        let address = address.to_lowercase();
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_bans \
             WHERE banned_address = $1 AND lifted_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(&address)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(n > 0)
    }

    pub async fn get_status(&self, address: &str) -> Result<BanStatus, ApiError> {
        let address = address.to_lowercase();
        let row = sqlx::query_as::<_, BanRow>(&format!(
            "SELECT {BAN_SELECT_FIELDS} FROM user_bans \
             WHERE banned_address = $1 AND lifted_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now()) \
             ORDER BY banned_at DESC LIMIT 1"
        ))
        .bind(&address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(match row {
            Some(row) => BanStatus {
                is_banned: true,
                ban: Some(ban_from_row(row)),
            },
            None => BanStatus {
                is_banned: false,
                ban: None,
            },
        })
    }

    pub async fn create_ban(&self, input: CreateBan) -> Result<UserBan, BanWriteError> {
        let banned_address = input.banned_address.to_lowercase();
        let banned_by = input.banned_by.to_lowercase();

        if self
            .is_banned(&banned_address)
            .await
            .map_err(BanWriteError::Db)?
        {
            return Err(BanWriteError::AlreadyBanned(banned_address));
        }

        let expires_at = input
            .duration_ms
            .map(|d| Utc::now() + Duration::milliseconds(d));

        let row = sqlx::query_as::<_, BanRow>(&format!(
            "INSERT INTO user_bans \
               (banned_address, banned_by, reason, custom_message, expires_at, active) \
             VALUES ($1, $2, $3, $4, $5, TRUE) \
             RETURNING {BAN_SELECT_FIELDS}"
        ))
        .bind(&banned_address)
        .bind(&banned_by)
        .bind(&input.reason)
        .bind(&input.custom_message)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(ban_from_row(row))
    }

    pub async fn lift_ban(&self, address: &str, lifted_by: &str) -> Result<UserBan, LiftError> {
        let address = address.to_lowercase();
        let lifted_by = lifted_by.to_lowercase();
        let row = sqlx::query_as::<_, BanRow>(&format!(
            "UPDATE user_bans \
             SET lifted_at = now(), lifted_by = $2, active = FALSE \
             WHERE banned_address = $1 AND lifted_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now()) \
             RETURNING {BAN_SELECT_FIELDS}"
        ))
        .bind(&address)
        .bind(&lifted_by)
        .fetch_optional(&self.pool)
        .await?;

        row.map(ban_from_row).ok_or(LiftError::NotFound(address))
    }

    pub async fn get_active_bans(&self) -> Result<Vec<UserBan>, ApiError> {
        let rows = sqlx::query_as::<_, BanRow>(&format!(
            "SELECT {BAN_SELECT_FIELDS} FROM user_bans \
             WHERE lifted_at IS NULL AND (expires_at IS NULL OR expires_at > now()) \
             ORDER BY banned_at DESC"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ban_from_row).collect())
    }

    pub async fn create_warning(&self, input: CreateWarning) -> Result<UserWarning, ApiError> {
        let warned_address = input.warned_address.to_lowercase();
        let warned_by = input.warned_by.to_lowercase();
        let row = sqlx::query_as::<_, WarningRow>(&format!(
            "INSERT INTO user_warnings (warned_address, warned_by, reason) \
             VALUES ($1, $2, $3) \
             RETURNING {WARNING_SELECT_FIELDS}"
        ))
        .bind(&warned_address)
        .bind(&warned_by)
        .bind(&input.reason)
        .fetch_one(&self.pool)
        .await?;
        Ok(warning_from_row(row))
    }

    pub async fn get_warnings(&self, address: &str) -> Result<Vec<UserWarning>, ApiError> {
        let address = address.to_lowercase();
        let rows = sqlx::query_as::<_, WarningRow>(&format!(
            "SELECT {WARNING_SELECT_FIELDS} FROM user_warnings \
             WHERE warned_address = $1 ORDER BY warned_at DESC"
        ))
        .bind(&address)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(warning_from_row).collect())
    }
}

#[cfg(test)]
mod ms_iso_tests {
    use super::*;
    use chrono::TimeZone;

    fn ban_at(banned: DateTime<Utc>, expires: Option<DateTime<Utc>>) -> UserBan {
        UserBan {
            id: "00000000-0000-0000-0000-000000000001".into(),
            banned_address: "0xabc".into(),
            banned_by: "0xdef".into(),
            reason: "test".into(),
            custom_message: None,
            banned_at: banned,
            expires_at: expires,
            lifted_at: None,
            lifted_by: None,
            created_at: banned,
        }
    }

    #[test]
    fn ban_timestamps_serialize_fixed_3_digit_millis_iso() {
        // Whole-second instant: chrono's default RFC3339 omits the subsecond
        // dot entirely; the upstream JS wire always carries `.000`.
        let banned = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        let expires = Utc.timestamp_opt(1_718_900_500, 7_000_000).unwrap();
        let v = serde_json::to_value(ban_at(banned, Some(expires))).unwrap();
        assert_eq!(v["bannedAt"], "2024-06-20T16:13:20.000Z");
        assert_eq!(v["createdAt"], "2024-06-20T16:13:20.000Z");
        // 7ms subsecond stays 3-digit (not 6/9).
        assert_eq!(v["expiresAt"], "2024-06-20T16:21:40.007Z");
    }

    #[test]
    fn ban_nullable_timestamps_serialize_null() {
        let banned = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        let v = serde_json::to_value(ban_at(banned, None)).unwrap();
        assert!(v["expiresAt"].is_null());
        assert!(v["liftedAt"].is_null());
    }

    #[test]
    fn ban_truncates_sub_millisecond_precision() {
        // 123_456_789ns -> 123ms (drops the µs/ns tail, matching Date.toISOString).
        let banned = Utc.timestamp_opt(1_718_900_000, 123_456_789).unwrap();
        let v = serde_json::to_value(ban_at(banned, None)).unwrap();
        assert_eq!(v["bannedAt"], "2024-06-20T16:13:20.123Z");
    }

    #[test]
    fn warning_timestamps_serialize_fixed_3_digit_millis_iso() {
        let at = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        let w = UserWarning {
            id: "00000000-0000-0000-0000-000000000002".into(),
            warned_address: "0xabc".into(),
            warned_by: "0xdef".into(),
            reason: "test".into(),
            warned_at: at,
            created_at: at,
        };
        let v = serde_json::to_value(w).unwrap();
        assert_eq!(v["warnedAt"], "2024-06-20T16:13:20.000Z");
        assert_eq!(v["createdAt"], "2024-06-20T16:13:20.000Z");
    }
}
