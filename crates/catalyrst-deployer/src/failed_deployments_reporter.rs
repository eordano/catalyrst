use std::sync::Arc;

use crate::{BackendError, DatabaseBackend, FailedDeployment};

#[async_trait::async_trait]
pub trait IFailedDeploymentsReporter: Send + Sync {
    async fn report_failure(&self, deployment: &FailedDeployment) -> Result<(), BackendError>;
}

pub struct FailedDeploymentsReporter {
    database: Arc<dyn DatabaseBackend>,
}

impl FailedDeploymentsReporter {
    pub fn new(database: Arc<dyn DatabaseBackend>) -> Self {
        Self { database }
    }
}

#[async_trait::async_trait]
impl IFailedDeploymentsReporter for FailedDeploymentsReporter {
    async fn report_failure(&self, deployment: &FailedDeployment) -> Result<(), BackendError> {
        if deployment.from_snapshot {
            let existing = self
                .database
                .find_failed_deployment(&deployment.entity_id)
                .await?;

            if existing.is_some() {
                self.database
                    .delete_failed_deployment(&deployment.entity_id)
                    .await?;
            }

            self.database.save_failed_deployment(deployment).await?;
        }

        Ok(())
    }
}
