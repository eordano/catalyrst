use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u8 = 4;
pub const SUBPROTOCOL: &str = "archipelago-v4";
pub const OWNER_RENEWAL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
pub const OWNER_LAPSE: std::time::Duration = std::time::Duration::from_secs(90);
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_LANES_PER_SOCKET: usize = 16;
pub const MAX_REQUEST_ID_BYTES: usize = 96;
pub const MAX_OPERATION_ID_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LaneKey(String);

impl LaneKey {
    pub fn parse(raw: &str) -> Result<Self, V4Error> {
        if raw == "realm" {
            return Ok(Self(raw.to_string()));
        }
        let Some((kind, id)) = raw.split_once(':') else {
            return Err(V4Error::invalid_lane());
        };
        if !matches!(kind, "scene" | "voice" | "world")
            || id.is_empty()
            || id.len() > 96
            || !id
                .as_bytes()
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'/'))
        {
            return Err(V4Error::invalid_lane());
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct V4Owner {
    pub address: String,
    pub session: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentPayload {
    pub island_id: String,
    pub connection_string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_island_id: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub peers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentState {
    pub lane: String,
    pub authority_incarnation: String,
    pub assignment_revision: u64,
    pub realm_revision: u64,
    pub owner_session: String,
    pub owner_epoch: u64,
    pub fencing_token: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment: Option<AssignmentPayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RetryClass {
    Never,
    AfterReconnect,
    Later,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicError {
    pub code: u16,
    pub name: &'static str,
    pub retry: RetryClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Error)]
pub enum V4Error {
    #[error("invalid frame")]
    InvalidFrame,
    #[error("protocol violation")]
    Protocol,
    #[error("authentication failed")]
    AuthFailed,
    #[error("stale connection epoch")]
    StaleEpoch,
    #[error("authority unavailable")]
    AuthorityUnavailable,
    #[error("authority conflict")]
    AuthorityConflict,
    #[error("gap requires snapshot")]
    GapRequiresSnapshot,
    #[error("limit exceeded")]
    LimitExceeded,
    #[error("invalid lane")]
    InvalidLane,
}

impl V4Error {
    pub fn public(&self) -> PublicError {
        match self {
            Self::InvalidFrame => PublicError {
                code: 1001,
                name: "invalid_frame",
                retry: RetryClass::Never,
                detail: None,
            },
            Self::Protocol => PublicError {
                code: 1002,
                name: "protocol_violation",
                retry: RetryClass::Never,
                detail: None,
            },
            Self::AuthFailed => PublicError {
                code: 1003,
                name: "auth_failed",
                retry: RetryClass::Never,
                detail: None,
            },
            Self::StaleEpoch => PublicError {
                code: 1004,
                name: "stale_epoch",
                retry: RetryClass::AfterReconnect,
                detail: None,
            },
            Self::AuthorityUnavailable => PublicError {
                code: 1005,
                name: "authority_unavailable",
                retry: RetryClass::Later,
                detail: None,
            },
            Self::AuthorityConflict => PublicError {
                code: 1006,
                name: "authority_conflict",
                retry: RetryClass::AfterReconnect,
                detail: None,
            },
            Self::GapRequiresSnapshot => PublicError {
                code: 1007,
                name: "gap_requires_snapshot",
                retry: RetryClass::Never,
                detail: None,
            },
            Self::LimitExceeded => PublicError {
                code: 1008,
                name: "limit_exceeded",
                retry: RetryClass::Never,
                detail: None,
            },
            Self::InvalidLane => PublicError {
                code: 1009,
                name: "invalid_lane",
                retry: RetryClass::Never,
                detail: None,
            },
        }
    }

    fn invalid_lane() -> Self {
        Self::InvalidLane
    }
}

#[derive(Clone)]
pub enum AssignmentAuthority {
    Pg(Arc<PgAssignmentAuthority>),
    Unavailable { incarnation: String },
}

impl AssignmentAuthority {
    pub async fn pg(pool: PgPool) -> Result<Arc<Self>, sqlx::Error> {
        Self::pg_with_namespace(pool, "default").await
    }

    pub async fn pg_with_namespace(
        pool: PgPool,
        namespace: impl Into<String>,
    ) -> Result<Arc<Self>, sqlx::Error> {
        Ok(Arc::new(Self::Pg(Arc::new(
            PgAssignmentAuthority::prepare(pool, namespace.into()).await?,
        ))))
    }

    pub fn unavailable(replica_id: &str) -> Arc<Self> {
        Arc::new(Self::Unavailable {
            incarnation: format!("unavailable:{replica_id}"),
        })
    }

    pub fn incarnation(&self) -> &str {
        match self {
            Self::Pg(pg) => &pg.incarnation,
            Self::Unavailable { incarnation } => incarnation,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Pg(_))
    }

    pub async fn claim_lanes(
        &self,
        owner: &V4Owner,
        lanes: &[LaneKey],
    ) -> Result<Vec<AssignmentState>, V4Error> {
        match self {
            Self::Pg(pg) => pg.claim_lanes(owner, lanes).await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn next_connection_epoch(&self) -> Result<u64, V4Error> {
        match self {
            Self::Pg(pg) => pg
                .next_connection_epoch()
                .await
                .map_err(|_| V4Error::AuthorityUnavailable),
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn publish_assignment(
        &self,
        owner: &V4Owner,
        lane: &LaneKey,
        assignment: AssignmentPayload,
    ) -> Result<AssignmentState, V4Error> {
        match self {
            Self::Pg(pg) => pg.publish_assignment(owner, lane, assignment).await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn snapshot(
        &self,
        owner: &V4Owner,
        lane: &LaneKey,
    ) -> Result<AssignmentState, V4Error> {
        match self {
            Self::Pg(pg) => pg.snapshot(owner, lane).await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn ack(
        &self,
        owner: &V4Owner,
        lane: &LaneKey,
        revision: u64,
    ) -> Result<AssignmentState, V4Error> {
        match self {
            Self::Pg(pg) => pg.ack(owner, lane, revision).await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn realm_holder(&self, address: &str) -> Result<Option<String>, V4Error> {
        match self {
            Self::Pg(pg) => pg.realm_holder(address).await,
            Self::Unavailable { .. } => Ok(None),
        }
    }

    pub async fn renew(&self, owner: &V4Owner) -> Result<(), V4Error> {
        match self {
            Self::Pg(pg) => pg.set_renewal(owner, "clock_timestamp()").await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }

    pub async fn release(&self, owner: &V4Owner) -> Result<(), V4Error> {
        match self {
            Self::Pg(pg) => pg.set_renewal(owner, "'-infinity'").await,
            Self::Unavailable { .. } => Err(V4Error::AuthorityUnavailable),
        }
    }
}

pub struct PgAssignmentAuthority {
    pool: PgPool,
    incarnation: String,
    namespace: String,
}

impl PgAssignmentAuthority {
    async fn prepare(pool: PgPool, namespace: String) -> Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('archipelago_v4_schema'))")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS archipelago_v4_meta (
                key text PRIMARY KEY,
                value text NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("CREATE SEQUENCE IF NOT EXISTS archipelago_v4_connection_epoch")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS archipelago_v4_assignments (
                deployment_audience text NOT NULL,
                owner_address text NOT NULL,
                lane_key text NOT NULL,
                authority_incarnation text NOT NULL,
                assignment_revision bigint NOT NULL,
                owner_session text NOT NULL,
                owner_epoch bigint NOT NULL,
                fencing_token bigint NOT NULL,
                assignment_json jsonb,
                acknowledged_revision bigint NOT NULL DEFAULT 0,
                updated_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY (deployment_audience, owner_address, lane_key)
            )",
        )
        .execute(&mut *tx)
        .await?;
        let minted = Uuid::new_v4().to_string();
        let incarnation: String = sqlx::query_scalar(
            "INSERT INTO archipelago_v4_meta (key, value)
             VALUES ('authority_incarnation', $1)
             ON CONFLICT (key) DO UPDATE SET value = archipelago_v4_meta.value
             RETURNING value",
        )
        .bind(minted)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Self {
            pool,
            incarnation,
            namespace,
        })
    }

    async fn realm_holder(&self, address: &str) -> Result<Option<String>, V4Error> {
        sqlx::query_scalar(
            "SELECT owner_session FROM archipelago_v4_assignments
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = 'realm'
               AND updated_at >= now() - $3 * interval '1 second'",
        )
        .bind(&self.namespace)
        .bind(address)
        .bind(OWNER_LAPSE.as_secs() as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)
    }

    async fn set_renewal(&self, owner: &V4Owner, renewed_at: &'static str) -> Result<(), V4Error> {
        checked_epoch(owner.epoch)?;
        let changed = sqlx::query(sqlx::AssertSqlSafe(format!(
            "WITH held AS MATERIALIZED (
                SELECT lane_key, updated_at FROM archipelago_v4_assignments
                WHERE deployment_audience = $1 AND owner_address = $2
                  AND owner_session = $3 AND owner_epoch = $4
                ORDER BY lane_key FOR UPDATE
             )
             UPDATE archipelago_v4_assignments AS current SET updated_at = {renewed_at}
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND owner_session = $3
               AND owner_epoch = $4
               AND EXISTS (SELECT 1 FROM held WHERE held.lane_key = current.lane_key
                 AND held.updated_at >= clock_timestamp() - interval '90 seconds')"
        )))
        .bind(&self.namespace)
        .bind(&owner.address)
        .bind(&owner.session)
        .bind(owner.epoch as i64)
        .execute(&self.pool)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)?;
        if changed.rows_affected() == 0 {
            Err(V4Error::AuthorityConflict)
        } else {
            Ok(())
        }
    }

    async fn next_connection_epoch(&self) -> Result<u64, sqlx::Error> {
        let value: i64 = sqlx::query_scalar("SELECT nextval('archipelago_v4_connection_epoch')")
            .fetch_one(&self.pool)
            .await?;
        Ok(value.max(1) as u64)
    }

    async fn claim_lanes(
        &self,
        owner: &V4Owner,
        lanes: &[LaneKey],
    ) -> Result<Vec<AssignmentState>, V4Error> {
        checked_epoch(owner.epoch)?;
        if lanes.is_empty() || lanes.len() > MAX_LANES_PER_SOCKET {
            return Err(V4Error::LimitExceeded);
        }
        let mut lanes = lanes.to_vec();
        lanes.sort_unstable();
        if lanes.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(V4Error::InvalidLane);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| V4Error::AuthorityUnavailable)?;
        let mut out = Vec::with_capacity(lanes.len());
        for lane in &lanes {
            out.push(self.claim_lane(&mut tx, owner, lane).await?);
        }
        tx.commit()
            .await
            .map_err(|_| V4Error::AuthorityUnavailable)?;
        Ok(out)
    }

    async fn claim_lane(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        owner: &V4Owner,
        lane: &LaneKey,
    ) -> Result<AssignmentState, V4Error> {
        let row = sqlx::query(
            "INSERT INTO archipelago_v4_assignments (
                deployment_audience, owner_address, lane_key, authority_incarnation, assignment_revision,
                owner_session, owner_epoch, fencing_token
             )
             VALUES ($1, $2, $3, $4, 1, $5, $6, 1)
             ON CONFLICT (deployment_audience, owner_address, lane_key) DO UPDATE SET
                assignment_revision = CASE
                    WHEN archipelago_v4_assignments.owner_session = EXCLUDED.owner_session
                     AND archipelago_v4_assignments.owner_epoch = EXCLUDED.owner_epoch
                    THEN archipelago_v4_assignments.assignment_revision
                    ELSE archipelago_v4_assignments.assignment_revision + 1
                END,
                owner_address = EXCLUDED.owner_address,
                owner_session = EXCLUDED.owner_session,
                owner_epoch = EXCLUDED.owner_epoch,
                fencing_token = CASE
                    WHEN archipelago_v4_assignments.owner_session = EXCLUDED.owner_session
                     AND archipelago_v4_assignments.owner_epoch = EXCLUDED.owner_epoch
                    THEN archipelago_v4_assignments.fencing_token
                    ELSE archipelago_v4_assignments.fencing_token + 1
                END,
                assignment_json = CASE
                    WHEN archipelago_v4_assignments.owner_session = EXCLUDED.owner_session
                     AND archipelago_v4_assignments.owner_epoch = EXCLUDED.owner_epoch
                    THEN archipelago_v4_assignments.assignment_json
                    ELSE NULL
                END,
                acknowledged_revision = CASE
                    WHEN archipelago_v4_assignments.owner_session = EXCLUDED.owner_session
                     AND archipelago_v4_assignments.owner_epoch = EXCLUDED.owner_epoch
                    THEN archipelago_v4_assignments.acknowledged_revision
                    ELSE 0
                END,
                updated_at = clock_timestamp()
             WHERE EXCLUDED.owner_epoch > archipelago_v4_assignments.owner_epoch
                OR (
                    EXCLUDED.owner_epoch = archipelago_v4_assignments.owner_epoch
                    AND EXCLUDED.owner_session = archipelago_v4_assignments.owner_session
                    AND archipelago_v4_assignments.updated_at >= clock_timestamp() - interval '90 seconds'
                )
             RETURNING lane_key, authority_incarnation, assignment_revision, owner_session,
                       owner_epoch, fencing_token, assignment_json",
        )
        .bind(&self.namespace)
        .bind(&owner.address)
        .bind(lane.as_str())
        .bind(&self.incarnation)
        .bind(&owner.session)
        .bind(owner.epoch as i64)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)?;
        row.ok_or(V4Error::AuthorityConflict)
            .and_then(state_from_row)
    }

    async fn publish_assignment(
        &self,
        owner: &V4Owner,
        lane: &LaneKey,
        assignment: AssignmentPayload,
    ) -> Result<AssignmentState, V4Error> {
        checked_epoch(owner.epoch)?;
        let assignment = serde_json::to_value(assignment).map_err(|_| V4Error::InvalidFrame)?;
        if serde_json::to_vec(&assignment)
            .map_err(|_| V4Error::InvalidFrame)?
            .len()
            > MAX_FRAME_BYTES - 2048
        {
            return Err(V4Error::LimitExceeded);
        }
        let row = sqlx::query(
            "WITH held AS MATERIALIZED (
                SELECT updated_at FROM archipelago_v4_assignments
                WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = $3
                  AND owner_session = $4 AND owner_epoch = $5 FOR UPDATE
             )
             UPDATE archipelago_v4_assignments
             SET assignment_revision = assignment_revision + 1,
                 assignment_json = $6,
                 updated_at = clock_timestamp()
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = $3
               AND owner_session = $4
               AND owner_epoch = $5
               AND (SELECT updated_at FROM held) >= clock_timestamp() - interval '90 seconds'
             RETURNING lane_key, authority_incarnation, assignment_revision, owner_session,
                       owner_epoch, fencing_token, assignment_json",
        )
        .bind(&self.namespace)
        .bind(&owner.address)
        .bind(lane.as_str())
        .bind(&owner.session)
        .bind(owner.epoch as i64)
        .bind(assignment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)?;
        row.ok_or(V4Error::AuthorityConflict)
            .and_then(state_from_row)
    }

    async fn snapshot(&self, owner: &V4Owner, lane: &LaneKey) -> Result<AssignmentState, V4Error> {
        checked_epoch(owner.epoch)?;
        let row = sqlx::query(
            "SELECT lane_key, authority_incarnation, assignment_revision, owner_session,
                    owner_epoch, fencing_token, assignment_json,
                    updated_at >= clock_timestamp() - interval '90 seconds' AS live
             FROM archipelago_v4_assignments
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = $3
               AND owner_session = $4
               AND owner_epoch = $5",
        )
        .bind(&self.namespace)
        .bind(&owner.address)
        .bind(lane.as_str())
        .bind(&owner.session)
        .bind(owner.epoch as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)?;
        let row = row.ok_or(V4Error::AuthorityConflict)?;
        if !row
            .try_get::<bool, _>("live")
            .map_err(|_| V4Error::AuthorityUnavailable)?
        {
            return Err(V4Error::AuthorityUnavailable);
        }
        state_from_row(row)
    }

    async fn ack(
        &self,
        owner: &V4Owner,
        lane: &LaneKey,
        revision: u64,
    ) -> Result<AssignmentState, V4Error> {
        checked_epoch(owner.epoch)?;
        let revision = checked_i64(revision)?;
        let row = sqlx::query(
            "WITH held AS MATERIALIZED (
                SELECT updated_at FROM archipelago_v4_assignments
                WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = $3
                  AND owner_session = $4 AND owner_epoch = $5 FOR UPDATE
             )
             UPDATE archipelago_v4_assignments
             SET acknowledged_revision = GREATEST(acknowledged_revision, $6),
                 updated_at = clock_timestamp()
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = $3
               AND owner_session = $4
               AND owner_epoch = $5
               AND assignment_revision >= $6
               AND (SELECT updated_at FROM held) >= clock_timestamp() - interval '90 seconds'
             RETURNING lane_key, authority_incarnation, assignment_revision, owner_session,
                       owner_epoch, fencing_token, assignment_json",
        )
        .bind(&self.namespace)
        .bind(&owner.address)
        .bind(lane.as_str())
        .bind(&owner.session)
        .bind(owner.epoch as i64)
        .bind(revision)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| V4Error::AuthorityUnavailable)?;
        row.ok_or(V4Error::AuthorityConflict)
            .and_then(state_from_row)
    }
}

fn state_from_row(row: sqlx::postgres::PgRow) -> Result<AssignmentState, V4Error> {
    let failure = |_| V4Error::AuthorityUnavailable;
    let revision: i64 = row.try_get("assignment_revision").map_err(failure)?;
    let owner_epoch: i64 = row.try_get("owner_epoch").map_err(failure)?;
    let fencing_token: i64 = row.try_get("fencing_token").map_err(failure)?;
    let assignment: Option<serde_json::Value> = row.try_get("assignment_json").map_err(failure)?;
    if revision <= 0 || owner_epoch <= 0 || fencing_token <= 0 {
        return Err(V4Error::AuthorityUnavailable);
    }
    Ok(AssignmentState {
        lane: row.try_get("lane_key").map_err(failure)?,
        authority_incarnation: row.try_get("authority_incarnation").map_err(failure)?,
        assignment_revision: revision as u64,
        realm_revision: revision as u64,
        owner_session: row.try_get("owner_session").map_err(failure)?,
        owner_epoch: owner_epoch as u64,
        fencing_token: fencing_token as u64,
        assignment: assignment
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| V4Error::AuthorityUnavailable)?,
    })
}

fn checked_i64(value: u64) -> Result<i64, V4Error> {
    i64::try_from(value).map_err(|_| V4Error::LimitExceeded)
}

fn checked_epoch(value: u64) -> Result<i64, V4Error> {
    if value == 0 {
        return Err(V4Error::StaleEpoch);
    }
    checked_i64(value)
}

#[derive(Clone, Debug)]
pub struct SigningTranscript<'a> {
    pub audience: &'a str,
    pub replica_id: &'a str,
    pub authority_incarnation: &'a str,
    pub connection_id: &'a str,
    pub connection_epoch: u64,
    pub request_id: &'a str,
    pub expires_in_ms: u64,
    pub challenge_id: &'a str,
    pub challenge: &'a str,
    pub address: &'a str,
    pub session_id: &'a str,
    pub lanes: &'a [LaneKey],
    pub capabilities: &'a [String],
}

pub fn canonical_signing_payload(input: SigningTranscript<'_>) -> String {
    let mut lanes: Vec<&str> = input.lanes.iter().map(LaneKey::as_str).collect();
    lanes.sort_unstable();
    let mut capabilities: Vec<&str> = input.capabilities.iter().map(String::as_str).collect();
    capabilities.sort_unstable();
    format!(
        "dcl:archipelago:v4\nprotocol_version={}\naudience={}\nreplica_id={}\nauthority_incarnation={}\nconnection_id={}\nconnection_epoch={}\nrequest_id={}\nexpires_in_ms={}\nchallenge_id={}\nchallenge={}\naddress={}\nsession_id={}\nlanes={}\ncapabilities={}",
        PROTOCOL_VERSION,
        input.audience,
        input.replica_id,
        input.authority_incarnation,
        input.connection_id,
        input.connection_epoch,
        input.request_id,
        input.expires_in_ms,
        input.challenge_id,
        input.challenge,
        input.address.to_ascii_lowercase(),
        input.session_id.to_ascii_lowercase(),
        lanes.join(","),
        capabilities.join(","),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_assignment_expiry_is_not_part_of_the_client_payload() {
        let document = serde_json::json!({
            "islandId": "island-current",
            "connectionString": "livekit:wss://example.invalid/current",
            "tokenExpiresAtUnix": 2_000_000_000_u64
        });
        let assignment: AssignmentPayload = serde_json::from_value(document).unwrap();
        assert_eq!(
            serde_json::to_value(assignment).unwrap(),
            serde_json::json!({
                "islandId": "island-current",
                "connectionString": "livekit:wss://example.invalid/current"
            })
        );
    }

    #[test]
    fn lanes_are_typed_and_independent() {
        assert!(LaneKey::parse("realm").is_ok());
        assert!(LaneKey::parse("scene:abc").is_ok());
        assert!(LaneKey::parse("voice:room-1").is_ok());
        assert!(LaneKey::parse("world:world.example").is_ok());
        assert!(LaneKey::parse("realm:scene").is_err());
        assert!(LaneKey::parse("scene:").is_err());
    }

    #[test]
    fn signing_payload_is_canonical() {
        let lanes = vec![
            LaneKey::parse("scene:b").unwrap(),
            LaneKey::parse("realm").unwrap(),
        ];
        let caps = vec!["resume".to_string(), "ack".to_string()];
        let payload = canonical_signing_payload(SigningTranscript {
            audience: "aud",
            replica_id: "rep",
            authority_incarnation: "auth",
            connection_id: "conn",
            connection_epoch: 1,
            request_id: "req",
            expires_in_ms: 60_000,
            challenge_id: "cid",
            challenge: "chal",
            address: "0xAB",
            session_id: "0xCD",
            lanes: &lanes,
            capabilities: &caps,
        });
        assert!(payload.contains("lanes=realm,scene:b"));
        assert!(payload.contains("capabilities=ack,resume"));
        assert!(payload.contains("request_id=req"));
        assert!(payload.contains("expires_in_ms=60000"));
        assert!(payload.contains("address=0xab"));
    }
}
