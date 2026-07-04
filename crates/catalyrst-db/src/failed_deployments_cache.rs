use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::failed_deployments_repository::{self, SnapshotFailedDeployment};
use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    DeploymentError,
}

impl FailureReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeploymentError => "Deployment error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FailedDeployment {
    pub entity_type: String,
    pub entity_id: String,
    pub failure_timestamp: f64,
    pub reason: String,
    pub auth_chain: serde_json::Value,
    pub error_description: String,
    pub snapshot_hash: Option<String>,
}

impl From<SnapshotFailedDeployment> for FailedDeployment {
    fn from(sfd: SnapshotFailedDeployment) -> Self {
        Self {
            entity_type: sfd.entity_type,
            entity_id: sfd.entity_id,
            failure_timestamp: sfd.failure_timestamp,
            reason: sfd.reason,
            auth_chain: sfd.auth_chain,
            error_description: sfd.error_description,
            snapshot_hash: Some(sfd.snapshot_hash),
        }
    }
}

#[derive(Clone)]
pub struct FailedDeploymentsCache {
    pool: PgPool,
    inner: Arc<RwLock<HashMap<String, FailedDeployment>>>,
}

impl FailedDeploymentsCache {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<(), sqlx::Error> {
        let rows =
            failed_deployments_repository::get_snapshot_failed_deployments(&self.pool).await?;
        let mut map = self.inner.write().await;
        for row in rows {
            let fd = FailedDeployment::from(row);
            map.insert(fd.entity_id.clone(), fd);
        }
        Ok(())
    }

    pub async fn get_all(&self) -> Vec<FailedDeployment> {
        let map = self.inner.read().await;
        map.values().cloned().collect()
    }

    pub async fn find(&self, entity_id: &str) -> Option<FailedDeployment> {
        let map = self.inner.read().await;
        map.get(entity_id).cloned()
    }

    pub async fn remove(&self, entity_id: &str) -> Result<(), sqlx::Error> {
        let mut map = self.inner.write().await;
        if !map.contains_key(entity_id) {
            return Ok(());
        }
        map.remove(entity_id);
        drop(map);
        failed_deployments_repository::delete_failed_deployment(&self.pool, entity_id).await?;
        Ok(())
    }

    pub async fn cache(&self, deployment: FailedDeployment) {
        let mut map = self.inner.write().await;
        map.insert(deployment.entity_id.clone(), deployment);
    }

    pub async fn len(&self) -> usize {
        let map = self.inner.read().await;
        map.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}
