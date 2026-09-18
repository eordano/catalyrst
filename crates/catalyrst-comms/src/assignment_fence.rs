use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool, Postgres, Row, Transaction,
};
use std::str::FromStr;
use std::time::Duration;

const REALM_LANE: &str = "realm";
const OWNER_LAPSE_SECONDS: i32 = 90;
const MAX_ASSIGNMENT_BYTES: usize = 64 * 1024 - 2048;
const RECONNECTING_READER_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenFence {
    pub audience: String,
    pub lane: String,
    pub owner_session: String,
    pub owner_epoch: u64,
    pub assignment_revision: u64,
    pub fencing_token: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentDocument {
    pub island_id: String,
    pub connection_string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_island_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_expires_at_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub peers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealmAssignmentSnapshot {
    pub audience: String,
    pub wallet: String,
    pub lane: String,
    pub authority_incarnation: String,
    pub owner_session: String,
    pub owner_epoch: u64,
    pub assignment_revision: u64,
    pub fencing_token: u64,
    pub island_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StoredAssignmentDocument {
    island_id: String,
    #[serde(rename = "connectionString")]
    _connection_string: String,
    #[serde(default, rename = "fromIslandId")]
    _from_island_id: Option<String>,
    #[serde(default, rename = "tokenExpiresAtUnix")]
    _token_expires_at_unix: Option<u64>,
    #[serde(default, rename = "peers")]
    _peers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FenceError {
    #[error("assignment authority unavailable")]
    Unavailable,
    #[error("assignment owner changed")]
    Conflict,
    #[error("wallet has no fenced assignment owner")]
    Unowned,
    #[error("assignment authority data is invalid")]
    Invalid,
}

#[async_trait]
pub trait RealmAssignmentLease: Send {
    fn token_fence(&self) -> TokenFence;
    fn previous_island(&self) -> Option<String>;
    fn previous_token_expires_at_unix(&self) -> Option<u64> {
        None
    }
    async fn commit(self: Box<Self>, assignment: AssignmentDocument) -> Result<(), FenceError>;
}

#[async_trait]
pub trait AssignmentFence: Send + Sync {
    async fn lock_realm(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<Box<dyn RealmAssignmentLease>, FenceError>;
}

#[async_trait]
pub trait RealmAssignmentReader: Send + Sync {
    async fn current_realm_assignment(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError>;
}

pub struct PgAssignmentFence {
    pool: PgPool,
    audience: String,
}

pub struct ReconnectingPgAssignmentReader {
    inner: PgAssignmentFence,
}

fn validate_audience(audience: &str) -> Result<(), FenceError> {
    if audience.is_empty() || audience.len() > 256 {
        Err(FenceError::Invalid)
    } else {
        Ok(())
    }
}

impl PgAssignmentFence {
    pub async fn connect(url: &str, audience: String) -> Result<Self, FenceError> {
        validate_audience(&audience)?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|_| FenceError::Unavailable)?;
        let exists: bool =
            sqlx::query_scalar("SELECT to_regclass('archipelago_v4_assignments') IS NOT NULL")
                .fetch_one(&pool)
                .await
                .map_err(|_| FenceError::Unavailable)?;
        if !exists {
            return Err(FenceError::Unavailable);
        }
        Ok(Self { pool, audience })
    }
}

impl ReconnectingPgAssignmentReader {
    pub fn new(url: &str, audience: String) -> Result<Self, FenceError> {
        validate_audience(&audience)?;
        let options = PgConnectOptions::from_str(url).map_err(|_| FenceError::Invalid)?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(RECONNECTING_READER_TIMEOUT)
            .connect_lazy_with(options);
        Ok(Self {
            inner: PgAssignmentFence { pool, audience },
        })
    }
}

#[async_trait]
impl RealmAssignmentReader for ReconnectingPgAssignmentReader {
    async fn current_realm_assignment(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError> {
        tokio::time::timeout(
            RECONNECTING_READER_TIMEOUT,
            self.inner.current_realm_assignment(wallet, session),
        )
        .await
        .unwrap_or(Err(FenceError::Unavailable))
    }
}

#[async_trait]
impl AssignmentFence for ReconnectingPgAssignmentReader {
    async fn lock_realm(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<Box<dyn RealmAssignmentLease>, FenceError> {
        tokio::time::timeout(
            RECONNECTING_READER_TIMEOUT,
            self.inner.lock_realm(wallet, session),
        )
        .await
        .unwrap_or(Err(FenceError::Unavailable))
    }
}

struct PgRealmLease {
    tx: Transaction<'static, Postgres>,
    audience: String,
    wallet: String,
    session: String,
    owner_epoch: i64,
    assignment_revision: i64,
    fencing_token: i64,
    previous_island: Option<String>,
    previous_token_expires_at_unix: Option<u64>,
}

#[async_trait]
impl AssignmentFence for PgAssignmentFence {
    async fn lock_realm(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<Box<dyn RealmAssignmentLease>, FenceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| FenceError::Unavailable)?;
        for statement in [
            "SET LOCAL lock_timeout = '3000ms'",
            "SET LOCAL statement_timeout = '5000ms'",
            "SET LOCAL idle_in_transaction_session_timeout = '30000ms'",
        ] {
            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|_| FenceError::Unavailable)?;
        }
        let row = sqlx::query(
            "WITH locked AS MATERIALIZED (
                 SELECT owner_session, owner_epoch, assignment_revision, fencing_token,
                        assignment_json, updated_at
                 FROM archipelago_v4_assignments
                 WHERE deployment_audience = $1
                   AND owner_address = $2
                   AND lane_key = $3
                 FOR UPDATE
             )
             SELECT *, updated_at < clock_timestamp() - $4 * interval '1 second' AS owner_lapsed
             FROM locked",
        )
        .bind(&self.audience)
        .bind(wallet)
        .bind(REALM_LANE)
        .bind(OWNER_LAPSE_SECONDS)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| FenceError::Unavailable)?
        .ok_or(FenceError::Unowned)?;
        let owner_session: String = row
            .try_get("owner_session")
            .map_err(|_| FenceError::Invalid)?;
        let owner_lapsed: bool = row
            .try_get("owner_lapsed")
            .map_err(|_| FenceError::Invalid)?;
        if owner_session != session || owner_lapsed {
            return Err(FenceError::Conflict);
        }
        let owner_epoch: i64 = row
            .try_get("owner_epoch")
            .map_err(|_| FenceError::Invalid)?;
        let assignment_revision: i64 = row
            .try_get("assignment_revision")
            .map_err(|_| FenceError::Invalid)?;
        let fencing_token: i64 = row
            .try_get("fencing_token")
            .map_err(|_| FenceError::Invalid)?;
        if owner_epoch <= 0
            || assignment_revision <= 0
            || assignment_revision == i64::MAX
            || fencing_token <= 0
        {
            return Err(FenceError::Invalid);
        }
        let previous: Option<serde_json::Value> = row
            .try_get("assignment_json")
            .map_err(|_| FenceError::Invalid)?;
        let previous_island = previous
            .as_ref()
            .and_then(|value| value.get("islandId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let previous_token_expires_at_unix = previous
            .as_ref()
            .and_then(|value| value.get("tokenExpiresAtUnix"))
            .and_then(serde_json::Value::as_u64)
            .filter(|expires_at| *expires_at > 0);
        Ok(Box::new(PgRealmLease {
            tx,
            audience: self.audience.clone(),
            wallet: wallet.to_string(),
            session: owner_session,
            owner_epoch,
            assignment_revision,
            fencing_token,
            previous_island,
            previous_token_expires_at_unix,
        }))
    }
}

#[async_trait]
impl RealmAssignmentReader for PgAssignmentFence {
    async fn current_realm_assignment(
        &self,
        wallet: &str,
        session: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError> {
        let row = sqlx::query(
            "SELECT deployment_audience, owner_address, lane_key, authority_incarnation,
                    owner_session, owner_epoch, assignment_revision, fencing_token,
                    assignment_json,
                    updated_at < clock_timestamp() - $4 * interval '1 second' AS owner_lapsed
             FROM archipelago_v4_assignments
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = $3",
        )
        .bind(&self.audience)
        .bind(wallet)
        .bind(REALM_LANE)
        .bind(OWNER_LAPSE_SECONDS)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| FenceError::Unavailable)?
        .ok_or(FenceError::Unowned)?;

        let audience: String = row
            .try_get("deployment_audience")
            .map_err(|_| FenceError::Invalid)?;
        let wallet: String = row
            .try_get("owner_address")
            .map_err(|_| FenceError::Invalid)?;
        let lane: String = row.try_get("lane_key").map_err(|_| FenceError::Invalid)?;
        let authority_incarnation: String = row
            .try_get("authority_incarnation")
            .map_err(|_| FenceError::Invalid)?;
        let owner_session: String = row
            .try_get("owner_session")
            .map_err(|_| FenceError::Invalid)?;
        let owner_lapsed: bool = row
            .try_get("owner_lapsed")
            .map_err(|_| FenceError::Invalid)?;
        if owner_lapsed {
            return Err(FenceError::Unowned);
        }
        if owner_session != session {
            return Err(FenceError::Conflict);
        }
        let owner_epoch: i64 = row
            .try_get("owner_epoch")
            .map_err(|_| FenceError::Invalid)?;
        let assignment_revision: i64 = row
            .try_get("assignment_revision")
            .map_err(|_| FenceError::Invalid)?;
        let fencing_token: i64 = row
            .try_get("fencing_token")
            .map_err(|_| FenceError::Invalid)?;
        if audience != self.audience
            || audience.is_empty()
            || audience.len() > 256
            || wallet.is_empty()
            || wallet.len() > 256
            || lane != REALM_LANE
            || authority_incarnation.is_empty()
            || authority_incarnation.len() > 256
            || owner_session.is_empty()
            || owner_session.len() > 256
            || owner_epoch <= 0
            || assignment_revision <= 0
            || fencing_token <= 0
        {
            return Err(FenceError::Invalid);
        }
        let assignment: serde_json::Value = row
            .try_get::<Option<serde_json::Value>, _>("assignment_json")
            .map_err(|_| FenceError::Invalid)?
            .ok_or(FenceError::Invalid)?;
        let encoded = serde_json::to_vec(&assignment).map_err(|_| FenceError::Invalid)?;
        if encoded.len() > MAX_ASSIGNMENT_BYTES {
            return Err(FenceError::Invalid);
        }
        let assignment: StoredAssignmentDocument =
            serde_json::from_slice(&encoded).map_err(|_| FenceError::Invalid)?;
        let island_id = assignment.island_id;
        if island_id.is_empty() {
            return Err(FenceError::Invalid);
        }

        Ok(RealmAssignmentSnapshot {
            audience,
            wallet,
            lane,
            authority_incarnation,
            owner_session,
            owner_epoch: owner_epoch as u64,
            assignment_revision: assignment_revision as u64,
            fencing_token: fencing_token as u64,
            island_id,
        })
    }
}

#[async_trait]
impl RealmAssignmentLease for PgRealmLease {
    fn token_fence(&self) -> TokenFence {
        TokenFence {
            audience: self.audience.clone(),
            lane: REALM_LANE.to_string(),
            owner_session: self.session.clone(),
            owner_epoch: self.owner_epoch as u64,
            assignment_revision: (self.assignment_revision + 1) as u64,
            fencing_token: self.fencing_token as u64,
        }
    }

    fn previous_island(&self) -> Option<String> {
        self.previous_island.clone()
    }

    fn previous_token_expires_at_unix(&self) -> Option<u64> {
        self.previous_token_expires_at_unix
    }

    async fn commit(mut self: Box<Self>, assignment: AssignmentDocument) -> Result<(), FenceError> {
        let assignment = encode_assignment(assignment)?;
        let updated = sqlx::query(
            "UPDATE archipelago_v4_assignments
             SET assignment_revision = assignment_revision + 1,
                 assignment_json = $7,
                 updated_at = now()
             WHERE deployment_audience = $1
               AND owner_address = $2
               AND lane_key = $3
               AND owner_session = $4
               AND owner_epoch = $5
               AND fencing_token = $6
               AND updated_at >= clock_timestamp() - $8 * interval '1 second'
             RETURNING assignment_revision",
        )
        .bind(&self.audience)
        .bind(&self.wallet)
        .bind(REALM_LANE)
        .bind(&self.session)
        .bind(self.owner_epoch)
        .bind(self.fencing_token)
        .bind(assignment)
        .bind(OWNER_LAPSE_SECONDS)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(|_| FenceError::Unavailable)?
        .ok_or(FenceError::Conflict)?;
        let revision: i64 = updated
            .try_get("assignment_revision")
            .map_err(|_| FenceError::Invalid)?;
        if revision != self.assignment_revision + 1 {
            return Err(FenceError::Conflict);
        }
        self.tx.commit().await.map_err(|_| FenceError::Unavailable)
    }
}

fn encode_assignment(assignment: AssignmentDocument) -> Result<serde_json::Value, FenceError> {
    let bytes = serde_json::to_vec(&assignment).map_err(|_| FenceError::Invalid)?;
    if bytes.len() > MAX_ASSIGNMENT_BYTES {
        return Err(FenceError::Invalid);
    }
    serde_json::from_slice(&bytes).map_err(|_| FenceError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_document_uses_the_shared_authority_shape() {
        let value = serde_json::to_value(AssignmentDocument {
            island_id: "island-a".into(),
            connection_string: "livekit:wss://example".into(),
            from_island_id: Some("island-old".into()),
            token_expires_at_unix: Some(1234),
            peers: Default::default(),
        })
        .unwrap();
        assert_eq!(value["islandId"], "island-a");
        assert_eq!(value["connectionString"], "livekit:wss://example");
        assert_eq!(value["fromIslandId"], "island-old");
        assert_eq!(value["tokenExpiresAtUnix"], 1234);
        assert!(value.get("peers").is_none());
    }

    #[test]
    fn assignment_document_must_fit_the_authority_frame_budget() {
        let assignment = AssignmentDocument {
            island_id: "island-a".into(),
            connection_string: "x".repeat(MAX_ASSIGNMENT_BYTES),
            from_island_id: None,
            token_expires_at_unix: None,
            peers: Default::default(),
        };
        assert_eq!(encode_assignment(assignment), Err(FenceError::Invalid));
    }
}
