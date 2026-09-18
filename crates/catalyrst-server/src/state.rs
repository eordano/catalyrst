use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use catalyrst_commons::cache::TtlMap;
use catalyrst_storage::StorageError;
use serde_json::Value;

use crate::sync::SnapshotMetadata;
use crate::wire_types::{
    ControllerDeployment, DeploymentsFilters, PointerChangeDelta, PointerChangesFilters,
};

/// Read surface over the content store: `Ok(None)` means provably absent, `Err` a storage fault -- a fault reported as absence makes a broken node advertise itself as empty to peers.
#[async_trait]
pub trait ContentStorage: Send + Sync {
    async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, StorageError>;

    async fn retrieve_stream(&self, hash: &str) -> Result<Option<(Body, u64)>, StorageError>;

    async fn retrieve_range(
        &self,
        hash: &str,
        start: u64,
        end: u64,
    ) -> Result<Option<Bytes>, StorageError>;

    async fn file_info(&self, hash: &str) -> Result<Option<FileInfo>, StorageError>;

    async fn exist_multiple(
        &self,
        hashes: &[String],
    ) -> Result<HashMap<String, bool>, StorageError>;

    /// One open answers the whole request: size, sniff window, range and body all come off the
    /// same descriptor. The default composes the older calls for backends that do not override it.
    async fn open(&self, hash: &str) -> Result<Option<OpenedContent>, StorageError> {
        let Some(info) = self.file_info(hash).await? else {
            return Ok(None);
        };
        let Some(bytes) = self.retrieve(hash).await? else {
            return Ok(None);
        };
        Ok(Some(OpenedContent {
            size: info.size.unwrap_or(bytes.len() as u64),
            encoding: info.encoding,
            reader: Box::new(std::io::Cursor::new(bytes)),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: Option<u64>,
    pub content_size: Option<u64>,
    pub encoding: Option<String>,
}

pub trait ContentReader: tokio::io::AsyncRead + tokio::io::AsyncSeek + Send + Unpin {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncSeek + Send + Unpin> ContentReader for T {}

pub struct OpenedContent {
    pub size: u64,
    pub encoding: Option<String>,
    pub reader: Box<dyn ContentReader>,
}

impl OpenedContent {
    /// The bytes in `start..=end` clamped to the file; the reader is left positioned after them.
    pub async fn read_range(&mut self, start: u64, end: u64) -> Result<Bytes, StorageError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        if start >= self.size {
            return Ok(Bytes::new());
        }
        let len = (end.min(self.size - 1) - start + 1) as usize;
        self.reader.seek(std::io::SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; len];
        let mut filled = 0;
        while filled < len {
            let n = self.reader.read(&mut buf[filled..]).await?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        buf.truncate(filled);
        Ok(Bytes::from(buf))
    }

    /// Streams the file from the start; `consumed` is whatever a preceding `read_range(0, ..)`
    /// took off the front, replayed instead of re-read.
    pub fn into_body(self, consumed: Bytes) -> Body {
        use futures::StreamExt;

        let rest = tokio_util::io::ReaderStream::with_capacity(self.reader, 64 * 1024);
        Body::from_stream(futures::stream::iter([Ok::<_, std::io::Error>(consumed)]).chain(rest))
    }
}

#[async_trait]
pub trait Database: Send + Sync {
    async fn deployment_committed(&self, _entity_id: &str) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn active_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError>;

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError>;

    async fn active_entity_ids_by_content_hash(
        &self,
        hash: &str,
    ) -> Result<Vec<String>, DatabaseError>;

    async fn get_deployments(
        &self,
        options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError>;

    async fn get_pointer_changes(
        &self,
        options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError>;

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError>;

    /// One page of `get_failed_deployments`; backends push the window into the query.
    async fn get_failed_deployments_page(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Value>, DatabaseError> {
        let all = self.get_failed_deployments().await?;
        Ok(all
            .into_iter()
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .collect())
    }

    /// Boot-time shape of the content schema; `None` makes callers probe the database themselves.
    fn deployment_schema(&self) -> Option<crate::schema_migrations::DeploymentSchema> {
        None
    }

    async fn get_audit_info(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError>;

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError>;

    /// Serialized form of `active_entities_by_pointers`: a backend holding entities as bytes hands
    /// them over without a parse/re-serialize round; the default serializes the parsed form.
    async fn active_entity_docs_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<EntityDoc>, DatabaseError> {
        Ok(EntityDoc::from_values(
            self.active_entities_by_pointers(pointers).await?,
        ))
    }

    async fn active_entity_docs_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<EntityDoc>, DatabaseError> {
        Ok(EntityDoc::from_values(
            self.active_entities_by_ids(ids).await?,
        ))
    }

    async fn active_entity_docs_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixDocsResult, DatabaseError> {
        let result = self
            .active_entities_by_prefix(prefix, offset, limit)
            .await?;
        Ok(PrefixDocsResult {
            total: result.total,
            entities: EntityDoc::from_values(result.entities),
        })
    }

    async fn clear_failed_deployment(&self, _entity_id: &str) -> Result<u64, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "clear_failed_deployment not supported by this backend".to_string(),
        ))
    }

    async fn clear_all_failed_deployments(&self) -> Result<u64, DatabaseError> {
        Err(DatabaseError::Unsupported(
            "clear_all_failed_deployments not supported by this backend".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct PrefixQueryResult {
    pub total: i64,
    pub entities: Vec<Value>,
}

/// An active entity as the JSON document a response carries, with its id alongside so the
/// denylist check needs no parse.
#[derive(Debug, Clone)]
pub struct EntityDoc {
    pub id: String,
    pub json: Bytes,
}

impl EntityDoc {
    pub fn from_value(value: &Value) -> Self {
        Self {
            id: value
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or_default()
                .to_string(),
            json: Bytes::from(serde_json::to_vec(value).unwrap_or_default()),
        }
    }

    pub fn from_values(values: Vec<Value>) -> Vec<Self> {
        values.iter().map(Self::from_value).collect()
    }

    pub fn to_value(&self) -> Option<Value> {
        serde_json::from_slice(&self.json).ok()
    }

    /// `[doc,doc,...]` by concatenation, the same bytes `Json(Vec<Value>)` would produce.
    pub fn json_array(docs: &[Self]) -> Bytes {
        let mut out = Vec::with_capacity(docs.iter().map(|d| d.json.len() + 1).sum::<usize>() + 2);
        out.push(b'[');
        for (i, doc) in docs.iter().enumerate() {
            if i > 0 {
                out.push(b',');
            }
            out.extend_from_slice(&doc.json);
        }
        out.push(b']');
        Bytes::from(out)
    }
}

#[derive(Debug, Clone)]
pub struct PrefixDocsResult {
    pub total: i64,
    pub entities: Vec<EntityDoc>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentQueryOptions {
    pub entity_types: Vec<String>,
    pub entity_ids: Vec<String>,
    pub pointers: Vec<String>,
    pub deployed_by: Vec<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub only_currently_pointed: Option<bool>,
    pub fields: Vec<String>,
    pub sorting_field: Option<String>,
    pub sorting_order: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub last_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploymentQueryResult {
    pub deployments: Vec<ControllerDeployment>,
    pub filters: DeploymentsFilters,
    pub pagination: PaginationResult,
}

#[derive(Debug, Clone, Default)]
pub struct PointerChangesQueryOptions {
    pub entity_types: Vec<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub include_auth_chain: bool,
    pub sorting_field: Option<String>,
    pub sorting_order: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub last_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PointerChangesQueryResult {
    pub deltas: Vec<PointerChangeDelta>,
    pub filters: PointerChangesFilters,
    pub pagination: PaginationResult,
}

#[derive(Debug, Clone)]
pub struct PaginationResult {
    pub offset: i64,
    pub limit: i64,
    pub more_data: bool,
    pub next: Option<String>,
    pub last_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("query failed: {0}")]
    QueryFailed(String),
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone)]
pub enum DeployFailure {
    Rejected(Vec<String>),
    /// This node could not decide, so the deposit is neither accepted nor at fault; answer 503 so
    /// the caller retries instead of being told their content does not exist.
    Unavailable(Vec<String>),
}

impl DeployFailure {
    pub fn errors(&self) -> &[String] {
        match self {
            DeployFailure::Rejected(errors) | DeployFailure::Unavailable(errors) => errors,
        }
    }
}

impl From<Vec<String>> for DeployFailure {
    fn from(errors: Vec<String>) -> Self {
        DeployFailure::Rejected(errors)
    }
}

#[async_trait]
pub trait Deployer: Send + Sync {
    async fn deploy_entity(
        &self,
        files: Vec<Bytes>,
        entity_id: &str,
        auth_chain: Value,
        context: &str,
    ) -> Result<i64, DeployFailure>;

    async fn retry_failed_deployment(&self, _entity_id: &str) -> Result<String, Vec<String>> {
        Err(vec![
            "retry_failed_deployment not supported by this deployer".to_string(),
        ])
    }
}

pub trait Denylist: Send + Sync {
    fn is_denylisted(&self, id: &str) -> bool;

    fn add(&self, _id: &str) -> Result<bool, String> {
        Err("denylist add not supported by this backend".to_string())
    }

    fn remove(&self, _id: &str) -> Result<bool, String> {
        Err("denylist remove not supported by this backend".to_string())
    }

    fn list(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Fail closed: an entity carrying no string `id` cannot be checked against the
/// denylist, so it is dropped rather than served. Retaining it would let a
/// malformed or id-less entity walk past a list whose whole job is to withhold
/// specific ids.
pub fn retain_non_denylisted(entities: &mut Vec<Value>, denylist: &dyn Denylist) {
    entities.retain(|entity| {
        entity
            .get("id")
            .and_then(|id| id.as_str())
            .is_some_and(|id| !denylist.is_denylisted(id))
    });
}

pub fn retain_non_denylisted_docs(docs: &mut Vec<EntityDoc>, denylist: &dyn Denylist) {
    docs.retain(|doc| !doc.id.is_empty() && !denylist.is_denylisted(&doc.id));
}

pub trait ChallengeSupervisor: Send + Sync {
    fn get_challenge_text(&self) -> String;

    fn refresh(&self) -> String {
        self.get_challenge_text()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncControl {
    Run,
    Paused,
}

pub trait SynchronizationState: Send + Sync {
    fn get_state(&self) -> String;

    fn is_type_ready(&self, _entity_type: &str) -> bool {
        true
    }

    fn ready_types(&self) -> Option<Vec<String>> {
        None
    }

    fn sync_frontier_ms(&self) -> Option<i64> {
        None
    }

    fn sync_heartbeat_ms(&self) -> Option<i64> {
        None
    }

    fn control(&self) -> SyncControl {
        SyncControl::Run
    }

    fn pause(&self) -> Result<(), String> {
        Err("sync pause not supported by this backend".to_string())
    }

    fn resume(&self) -> Result<(), String> {
        Err("sync resume not supported by this backend".to_string())
    }

    fn force(&self) -> Result<(), String> {
        Err("sync force not supported by this backend".to_string())
    }
}

pub trait SnapshotGenerator: Send + Sync {
    fn get_current_snapshots(&self) -> Option<Vec<SnapshotMetadata>>;

    fn trigger_regeneration(&self) -> Result<String, String> {
        Err("snapshot regeneration not supported by this backend".to_string())
    }
}

pub trait AcceptingUsers: Send + Sync {
    fn is_accepting(&self) -> bool;

    fn set_accepting(&self, _accepting: bool) -> Result<(), String> {
        Err("accepting-users toggle not supported by this backend".to_string())
    }
}

#[async_trait]
pub trait ContentCluster: Send + Sync {
    fn get_status(&self) -> Value;
}

pub const DEPLOYMENTS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

pub const DEPLOYMENTS_CACHE_MAX_ENTRIES: usize = 1000;

pub struct AppState {
    pub storage: Arc<dyn ContentStorage>,
    pub database: Arc<dyn Database>,
    pub deployer: Arc<dyn Deployer>,
    pub denylist: Arc<dyn Denylist>,

    pub challenge_supervisor: Arc<dyn ChallengeSupervisor>,
    pub synchronization_state: Arc<dyn SynchronizationState>,
    pub snapshot_generator: Arc<dyn SnapshotGenerator>,
    pub content_cluster: Arc<dyn ContentCluster>,
    pub accepting_users: Arc<dyn AcceptingUsers>,

    pub deployments_cache: TtlMap<String, Bytes>,

    pub content_version: String,
    pub lambdas_version: String,
    pub commit_hash: String,
    pub eth_network: String,
    pub content_server_address: String,

    pub read_only: AtomicBool,

    pub audit_pool: Option<sqlx::PgPool>,

    pub content_pool: Option<sqlx::PgPool>,

    pub entities_cache_control_max_age: u64,

    pub content_public_url: String,
    pub lambdas_public_url: String,
    pub realm_name: Option<String>,

    pub squid_pool: Option<sqlx::PgPool>,
    pub profile_cdn_base_url: String,

    pub land_image_base_url: String,
}

impl AppState {
    pub fn is_read_only(&self) -> bool {
        self.read_only.load(Ordering::Relaxed)
    }

    pub fn set_read_only(&self, read_only: bool) -> bool {
        self.read_only.store(read_only, Ordering::Relaxed);
        read_only
    }
}
