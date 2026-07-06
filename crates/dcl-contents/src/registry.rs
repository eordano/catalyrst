use std::collections::HashSet;
use std::sync::Arc;

pub use async_trait::async_trait;
use axum::routing::{get, post};
use axum::Router;

use crate::errors::ApiError;
use crate::handlers;
use crate::manifest_store::AbManifestStore;
use crate::types::ActiveEntity;

#[async_trait]
pub trait EntitySource: Send + Sync {
    async fn resolve_pointers(&self, pointers: &[String]) -> Result<Vec<ActiveEntity>, ApiError>;

    async fn resolve_profiles(&self, addresses: &[String]) -> Result<Vec<ActiveEntity>, ApiError>;

    async fn resolve_world(&self, world_name: &str) -> Result<Vec<ActiveEntity>, ApiError>;

    async fn resolve_one(&self, id: &str) -> Result<Option<ActiveEntity>, ApiError> {
        let mut all = self
            .resolve_pointers(std::slice::from_ref(&id.to_string()))
            .await?;

        if let Some(pos) = all.iter().position(|e| e.entity_id == id) {
            return Ok(Some(all.swap_remove(pos)));
        }
        Ok(all.into_iter().next())
    }
}

#[async_trait]
pub trait WorldPolicy: Send + Sync {
    async fn denylist(&self) -> Result<HashSet<String>, ApiError>;
    async fn spawn_override(&self, world_name: &str) -> Result<Option<(i64, i64)>, ApiError>;
}

pub struct OpenWorldPolicy;

#[async_trait]
impl WorldPolicy for OpenWorldPolicy {
    async fn denylist(&self) -> Result<HashSet<String>, ApiError> {
        Ok(HashSet::new())
    }

    async fn spawn_override(&self, _world_name: &str) -> Result<Option<(i64, i64)>, ApiError> {
        Ok(None)
    }
}

pub struct RegistryStateInner {
    pub content: Arc<dyn EntitySource>,
    pub manifests: AbManifestStore,
    pub profile_images_url: String,
    pub world_policy: Arc<dyn WorldPolicy>,
}

pub type RegistryAppState = Arc<RegistryStateInner>;

pub fn router() -> Router<RegistryAppState> {
    Router::new()
        .route("/profiles", post(handlers::profiles::post_profiles))
        .route(
            "/profiles/metadata",
            post(handlers::profiles::post_profiles_metadata),
        )
        .route(
            "/entities/status/{id}",
            get(handlers::status::get_entity_status),
        )
        .route(
            "/worlds/{world_name}/manifest",
            get(handlers::worlds::get_world_manifest),
        )
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::{async_trait, EntitySource, RegistryAppState, RegistryStateInner, WorldPolicy};
    use crate::errors::ApiError;
    use crate::manifest_store::AbManifestStore;
    use crate::types::ActiveEntity;

    pub struct StubSource(pub Vec<ActiveEntity>);

    #[async_trait]
    impl EntitySource for StubSource {
        async fn resolve_pointers(
            &self,
            pointers: &[String],
        ) -> Result<Vec<ActiveEntity>, ApiError> {
            let lowered: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();
            Ok(self
                .0
                .iter()
                .filter(|e| {
                    e.pointers.iter().any(|p| lowered.contains(p))
                        || pointers.contains(&e.entity_id)
                })
                .cloned()
                .collect())
        }

        async fn resolve_profiles(
            &self,
            addresses: &[String],
        ) -> Result<Vec<ActiveEntity>, ApiError> {
            let mut ents = self.resolve_pointers(addresses).await?;
            ents.retain(|e| e.entity_type == "profile");
            Ok(ents)
        }

        async fn resolve_world(&self, world_name: &str) -> Result<Vec<ActiveEntity>, ApiError> {
            self.resolve_pointers(std::slice::from_ref(&world_name.to_string()))
                .await
        }
    }

    pub struct StubPolicy {
        pub denylist: HashSet<String>,
        pub spawn: Option<(i64, i64)>,
    }

    #[async_trait]
    impl WorldPolicy for StubPolicy {
        async fn denylist(&self) -> Result<HashSet<String>, ApiError> {
            Ok(self.denylist.clone())
        }

        async fn spawn_override(&self, _world_name: &str) -> Result<Option<(i64, i64)>, ApiError> {
            Ok(self.spawn)
        }
    }

    pub fn entity(
        entity_id: &str,
        entity_type: &str,
        pointers: &[&str],
        metadata: serde_json::Value,
    ) -> ActiveEntity {
        ActiveEntity {
            deployment_id: 0,
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            timestamp: 1_700_000_000_000,
            pointers: pointers.iter().map(|p| p.to_string()).collect(),
            metadata,
            deployer_address: None,
            content: Vec::new(),
        }
    }

    pub fn state_with(
        entities: Vec<ActiveEntity>,
        world_policy: Arc<dyn WorldPolicy>,
        out_root: &std::path::Path,
    ) -> RegistryAppState {
        Arc::new(RegistryStateInner {
            content: Arc::new(StubSource(entities)),
            manifests: AbManifestStore::new(out_root),
            profile_images_url: "https://profile-images.example".to_string(),
            world_policy,
        })
    }

    pub fn open_state(entities: Vec<ActiveEntity>, out_root: &std::path::Path) -> RegistryAppState {
        state_with(entities, Arc::new(super::OpenWorldPolicy), out_root)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn router_constructs_without_route_conflicts() {
        let _ = super::router();
    }
}
