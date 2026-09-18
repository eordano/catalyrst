use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

pub mod ms_iso {
    use super::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
    }

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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "comms/", rename_all = "camelCase")
)]
pub struct UserBan {
    pub id: String,
    #[serde(rename = "bannedAddress")]
    pub banned_address: String,
    #[serde(rename = "bannedBy")]
    pub banned_by: String,
    pub reason: String,
    #[serde(rename = "customMessage")]
    pub custom_message: Option<String>,
    #[serde(rename = "bannedDeviceId")]
    pub banned_device_id: Option<String>,
    #[serde(rename = "bannedAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub banned_at: DateTime<Utc>,
    #[serde(rename = "expiresAt")]
    #[serde(serialize_with = "ms_iso::option::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedAt")]
    #[serde(serialize_with = "ms_iso::option::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub lifted_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedBy")]
    pub lifted_by: Option<String>,
    #[serde(rename = "createdAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

/// `UserBan` minus `banned_device_id`, the shape the unauthenticated
/// `GET /users/{address}/bans` publishes. The device id is a stable
/// cross-wallet machine identifier: two banned addresses carrying the same
/// one are publicly linkable, so only the moderator-gated `GET /bans` shows
/// it. Keep any new public route on this type, not `UserBan`.
#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "comms/", rename_all = "camelCase")
)]
pub struct PublicUserBan {
    pub id: String,
    #[serde(rename = "bannedAddress")]
    pub banned_address: String,
    #[serde(rename = "bannedBy")]
    pub banned_by: String,
    pub reason: String,
    #[serde(rename = "customMessage")]
    pub custom_message: Option<String>,
    #[serde(rename = "bannedAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub banned_at: DateTime<Utc>,
    #[serde(rename = "expiresAt")]
    #[serde(serialize_with = "ms_iso::option::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedAt")]
    #[serde(serialize_with = "ms_iso::option::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub lifted_at: Option<DateTime<Utc>>,
    #[serde(rename = "liftedBy")]
    pub lifted_by: Option<String>,
    #[serde(rename = "createdAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

impl From<UserBan> for PublicUserBan {
    fn from(ban: UserBan) -> Self {
        Self {
            id: ban.id,
            banned_address: ban.banned_address,
            banned_by: ban.banned_by,
            reason: ban.reason,
            custom_message: ban.custom_message,
            banned_at: ban.banned_at,
            expires_at: ban.expires_at,
            lifted_at: ban.lifted_at,
            lifted_by: ban.lifted_by,
            created_at: ban.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "comms/", rename_all = "camelCase")
)]
pub struct UserWarning {
    pub id: String,
    #[serde(rename = "warnedAddress")]
    pub warned_address: String,
    #[serde(rename = "warnedBy")]
    pub warned_by: String,
    pub reason: String,
    #[serde(rename = "warnedAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub warned_at: DateTime<Utc>,
    #[serde(rename = "createdAt")]
    #[serde(serialize_with = "ms_iso::serialize")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "comms/", rename_all = "camelCase")
)]
pub struct BanStatus {
    #[serde(rename = "isBanned")]
    pub is_banned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ban: Option<PublicUserBan>,
}

type BanRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    NaiveDateTime,
    Option<NaiveDateTime>,
    Option<NaiveDateTime>,
    Option<String>,
    NaiveDateTime,
);

type WarningRow = (Uuid, String, String, String, NaiveDateTime, NaiveDateTime);

const BAN_SELECT_FIELDS: &str =
    "id, banned_address, banned_by, reason, custom_message, banned_device_id, banned_at, expires_at, lifted_at, lifted_by, created_at";

const WARNING_SELECT_FIELDS: &str = "id, warned_address, warned_by, reason, warned_at, created_at";

/// Boolean expression over `$1` (lowercased address) and `$2` (the device id the
/// caller sent, or NULL to fall back to the device recorded for the address).
pub(crate) const CONNECTION_BAN_EXISTS: &str = "EXISTS (SELECT 1 FROM user_bans ub \
     WHERE (ub.banned_address = $1 \
            OR ub.banned_device_id = COALESCE($2::text, (SELECT NULLIF(p.device_id, '') \
                                                         FROM player_connection_info p \
                                                         WHERE p.address = $1))) \
       AND ub.lifted_at IS NULL \
       AND (ub.expires_at IS NULL OR ub.expires_at > now()))";

pub(crate) fn sent_device_id(device_id: Option<&str>) -> Option<String> {
    device_id
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(String::from)
}

fn ban_from_row(row: BanRow) -> UserBan {
    let (
        id,
        banned_address,
        banned_by,
        reason,
        custom_message,
        banned_device_id,
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
        banned_device_id,
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
    pub banned_device_id: Option<String>,
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
        .await?;
        Ok(n > 0)
    }

    /// Upstream's `getActiveBanForConnection`: the device the caller sends, else
    /// the one recorded for the address, matched alongside the address itself.
    pub async fn is_banned_for_connection(
        &self,
        address: &str,
        device_id: Option<&str>,
    ) -> Result<bool, ApiError> {
        let address = address.to_lowercase();
        let banned: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT {CONNECTION_BAN_EXISTS}"
        )))
        .bind(&address)
        .bind(sent_device_id(device_id))
        .fetch_one(&self.pool)
        .await?;
        Ok(banned)
    }

    /// Platform ban and scene ban for one connection in a single statement.
    pub async fn connection_gate(
        &self,
        address: &str,
        device_id: Option<&str>,
        place_id: &str,
    ) -> Result<(bool, bool), ApiError> {
        let address = address.to_lowercase();
        let row: (bool, bool) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {CONNECTION_BAN_EXISTS}, \
                    EXISTS (SELECT 1 FROM scene_bans WHERE place_id = $3 AND banned_address = $1)"
        )))
        .bind(&address)
        .bind(sent_device_id(device_id))
        .bind(place_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Index of the first address (in the given order) with an active platform ban
    /// on its wallet or its recorded device.
    pub async fn first_banned_for_connection(
        &self,
        addresses: &[&str],
    ) -> Result<Option<usize>, ApiError> {
        if addresses.is_empty() {
            return Ok(None);
        }
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        let idx: Option<i64> = sqlx::query_scalar(
            "SELECT a.ord - 1 FROM unnest($1::text[]) WITH ORDINALITY AS a(address, ord) \
             WHERE EXISTS (SELECT 1 FROM user_bans ub \
                 WHERE (ub.banned_address = a.address \
                        OR ub.banned_device_id = (SELECT NULLIF(p.device_id, '') \
                                                  FROM player_connection_info p \
                                                  WHERE p.address = a.address)) \
                   AND ub.lifted_at IS NULL \
                   AND (ub.expires_at IS NULL OR ub.expires_at > now())) \
             ORDER BY a.ord LIMIT 1",
        )
        .bind(&lowered)
        .fetch_optional(&self.pool)
        .await?;
        Ok(idx.map(|i| i as usize))
    }

    pub async fn get_status(&self, address: &str) -> Result<BanStatus, ApiError> {
        let address = address.to_lowercase();
        let row = sqlx::query_as::<_, BanRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {BAN_SELECT_FIELDS} FROM user_bans \
             WHERE banned_address = $1 AND lifted_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now()) \
             ORDER BY banned_at DESC LIMIT 1"
        )))
        .bind(&address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(match row {
            Some(row) => BanStatus {
                is_banned: true,
                ban: Some(ban_from_row(row).into()),
            },
            None => BanStatus {
                is_banned: false,
                ban: None,
            },
        })
    }

    pub async fn create_ban(&self, input: CreateBan) -> Result<UserBan, BanWriteError> {
        self.insert_ban(input, false).await
    }

    /// `create_ban` that snapshots the player's recorded device id when the
    /// caller sends none.
    pub async fn create_ban_with_recorded_device(
        &self,
        input: CreateBan,
    ) -> Result<UserBan, BanWriteError> {
        self.insert_ban(input, true).await
    }

    async fn insert_ban(
        &self,
        input: CreateBan,
        snapshot_device: bool,
    ) -> Result<UserBan, BanWriteError> {
        let banned_address = input.banned_address.to_lowercase();
        let banned_by = input.banned_by.to_lowercase();
        let expires_at = input
            .duration_ms
            .map(|d| Utc::now() + Duration::milliseconds(d));
        let banned_device_id = input.banned_device_id.filter(|s| !s.is_empty());

        let mut txn = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&banned_address)
            .execute(&mut *txn)
            .await?;

        let row = sqlx::query_as::<_, BanRow>(sqlx::AssertSqlSafe(format!(
            "WITH existing AS ( \
                SELECT count(*) AS n FROM user_bans \
                WHERE banned_address = $1 AND lifted_at IS NULL \
                  AND (expires_at IS NULL OR expires_at > now()) \
             ), ins AS ( \
                INSERT INTO user_bans \
                  (banned_address, banned_by, reason, custom_message, banned_device_id, expires_at, active) \
                SELECT $1, $2, $3, $4, \
                       CASE WHEN $7::bool \
                            THEN COALESCE($5::text, (SELECT NULLIF(p.device_id, '') \
                                                     FROM player_connection_info p \
                                                     WHERE p.address = $1)) \
                            ELSE $5::text END, \
                       $6, TRUE \
                WHERE (SELECT n FROM existing) = 0 \
                RETURNING {BAN_SELECT_FIELDS} \
             ) SELECT {BAN_SELECT_FIELDS} FROM ins"
        )))
        .bind(&banned_address)
        .bind(&banned_by)
        .bind(&input.reason)
        .bind(&input.custom_message)
        .bind(&banned_device_id)
        .bind(expires_at)
        .bind(snapshot_device)
        .fetch_optional(&mut *txn)
        .await?;

        let Some(row) = row else {
            return Err(BanWriteError::AlreadyBanned(banned_address));
        };

        txn.commit().await?;

        Ok(ban_from_row(row))
    }

    pub async fn lift_ban(&self, address: &str, lifted_by: &str) -> Result<UserBan, LiftError> {
        let address = address.to_lowercase();
        let lifted_by = lifted_by.to_lowercase();
        let row = sqlx::query_as::<_, BanRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE user_bans \
             SET lifted_at = now(), lifted_by = $2, active = FALSE \
             WHERE banned_address = $1 AND lifted_at IS NULL \
               AND (expires_at IS NULL OR expires_at > now()) \
             RETURNING {BAN_SELECT_FIELDS}"
        )))
        .bind(&address)
        .bind(&lifted_by)
        .fetch_optional(&self.pool)
        .await?;

        row.map(ban_from_row).ok_or(LiftError::NotFound(address))
    }

    pub async fn get_active_bans(&self) -> Result<Vec<UserBan>, ApiError> {
        let rows = sqlx::query_as::<_, BanRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {BAN_SELECT_FIELDS} FROM user_bans \
             WHERE lifted_at IS NULL AND (expires_at IS NULL OR expires_at > now()) \
             ORDER BY banned_at DESC"
        )))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ban_from_row).collect())
    }

    pub async fn create_warning(&self, input: CreateWarning) -> Result<UserWarning, ApiError> {
        let warned_address = input.warned_address.to_lowercase();
        let warned_by = input.warned_by.to_lowercase();
        let row = sqlx::query_as::<_, WarningRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO user_warnings (warned_address, warned_by, reason) \
             VALUES ($1, $2, $3) \
             RETURNING {WARNING_SELECT_FIELDS}"
        )))
        .bind(&warned_address)
        .bind(&warned_by)
        .bind(&input.reason)
        .fetch_one(&self.pool)
        .await?;
        Ok(warning_from_row(row))
    }

    pub async fn get_warnings(&self, address: &str) -> Result<Vec<UserWarning>, ApiError> {
        let address = address.to_lowercase();
        let rows = sqlx::query_as::<_, WarningRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {WARNING_SELECT_FIELDS} FROM user_warnings \
             WHERE warned_address = $1 ORDER BY warned_at DESC"
        )))
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
            banned_device_id: None,
            banned_at: banned,
            expires_at: expires,
            lifted_at: None,
            lifted_by: None,
            created_at: banned,
        }
    }

    #[test]
    fn ban_timestamps_serialize_fixed_3_digit_millis_iso() {
        let banned = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        let expires = Utc.timestamp_opt(1_718_900_500, 7_000_000).unwrap();
        let v = serde_json::to_value(ban_at(banned, Some(expires))).unwrap();
        assert_eq!(v["bannedAt"], "2024-06-20T16:13:20.000Z");
        assert_eq!(v["createdAt"], "2024-06-20T16:13:20.000Z");

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

#[cfg(test)]
mod public_shape_tests {
    use super::*;
    use chrono::TimeZone;

    fn device_ban() -> UserBan {
        let at = Utc.timestamp_opt(1_718_900_000, 0).unwrap();
        UserBan {
            id: "00000000-0000-0000-0000-000000000001".into(),
            banned_address: "0xabc".into(),
            banned_by: "0xdef".into(),
            reason: "spam".into(),
            custom_message: None,
            banned_device_id: Some("dev-1".into()),
            banned_at: at,
            expires_at: None,
            lifted_at: None,
            lifted_by: None,
            created_at: at,
        }
    }

    #[test]
    fn the_public_status_shape_never_carries_the_device_id() {
        let status = BanStatus {
            is_banned: true,
            ban: Some(device_ban().into()),
        };
        let v = serde_json::to_value(status).unwrap();
        let ban = &v["ban"];
        assert!(ban.get("bannedDeviceId").is_none());
        assert_eq!(ban["bannedAddress"], "0xabc");
        assert_eq!(ban["bannedAt"], "2024-06-20T16:13:20.000Z");
        assert_eq!(ban.as_object().unwrap().len(), 10);
    }

    #[test]
    fn the_moderator_list_shape_keeps_the_device_id() {
        let v = serde_json::to_value(vec![device_ban()]).unwrap();
        assert_eq!(v[0]["bannedDeviceId"], "dev-1");
        assert_eq!(v[0].as_object().unwrap().len(), 11);
    }
}
