use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::debug;

use crate::{
    BackendError, DatabaseBackend, Entity, Pointer,
    deployments::map_deployments_to_entities,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotActiveEntity;

#[derive(Debug, Clone)]
pub enum CachedEntity {
    Active(Entity),
    NotActive,
}

impl CachedEntity {
    pub fn as_entity(&self) -> Option<&Entity> {
        match self {
            CachedEntity::Active(e) => Some(e),
            CachedEntity::NotActive => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, CachedEntity::Active(_))
    }
}

#[derive(Debug, Clone)]
pub struct ActiveEntitiesConfig {
    pub cache_size: usize,
}

impl Default for ActiveEntitiesConfig {
    fn default() -> Self {
        Self { cache_size: 100_000 }
    }
}

pub struct ActiveEntities {
    database: Arc<dyn DatabaseBackend>,
    entity_cache: Mutex<HashMap<String, CachedEntity>>,
    pointer_cache: Mutex<HashMap<String, Option<String>>>,
    config: ActiveEntitiesConfig,
}

impl ActiveEntities {
    pub fn new(database: Arc<dyn DatabaseBackend>, config: ActiveEntitiesConfig) -> Self {
        Self {
            database,
            entity_cache: Mutex::new(HashMap::new()),
            pointer_cache: Mutex::new(HashMap::new()),
            config,
        }
    }

    pub async fn with_ids(&self, entity_ids: &[String]) -> Result<Vec<Entity>, BackendError> {
        let unique: HashSet<&String> = entity_ids.iter().collect();
        let mut on_cache = Vec::new();
        let mut remaining = Vec::new();

        {
            let cache = self.entity_cache.lock();
            for id in &unique {
                if let Some(cached) = cache.get(*id) {
                    if let Some(entity) = cached.as_entity() {
                        on_cache.push(entity.clone());
                    }
                } else {
                    remaining.push((*id).clone());
                }
            }
        }

        if !remaining.is_empty() {
            let found = self.find_entities_by_ids(&remaining).await?;
            on_cache.extend(found);
        }

        Ok(on_cache)
    }

    pub async fn with_pointers(&self, pointers: &[Pointer]) -> Result<Vec<Entity>, BackendError> {
        let unique: HashSet<String> = pointers.iter().map(|p| p.to_lowercase()).collect();
        let mut known_entity_ids = Vec::new();
        let mut remaining_pointers = Vec::new();

        {
            let pcache = self.pointer_cache.lock();
            for pointer in &unique {
                match pcache.get(pointer) {
                    Some(Some(eid)) => known_entity_ids.push(eid.clone()),
                    Some(None) => {  }
                    None => remaining_pointers.push(pointer.clone()),
                }
            }
        }

        let mut entities = Vec::new();

        if !known_entity_ids.is_empty() {
            entities.extend(self.with_ids(&known_entity_ids).await?);
        }

        if !remaining_pointers.is_empty() {
            let found = self.find_entities_by_pointers(&remaining_pointers).await?;
            entities.extend(found);
        }

        Ok(entities)
    }

    pub async fn update(
        &self,
        tx: &dyn crate::TransactionHandle,
        pointers: &[Pointer],
        entity: &Entity,
    ) -> Result<(), BackendError> {
        self.invalidate_previous_for_pointers(pointers);

        {
            let mut ecache = self.entity_cache.lock();
            let mut pcache = self.pointer_cache.lock();

            ecache.insert(entity.id.clone(), CachedEntity::Active(entity.clone()));
            for p in pointers {
                pcache.insert(p.to_lowercase(), Some(entity.id.clone()));
            }
        }

        self.database
            .update_active_deployments(tx, pointers, &entity.id)
            .await
    }

    pub async fn clear(
        &self,
        tx: &dyn crate::TransactionHandle,
        pointers: &[Pointer],
    ) -> Result<(), BackendError> {
        self.invalidate_previous_for_pointers(pointers);

        {
            let mut pcache = self.pointer_cache.lock();
            for p in pointers {
                pcache.insert(p.to_lowercase(), None);
            }
        }

        self.database.remove_active_deployments(tx, pointers).await
    }

    pub fn clear_pointers_cache(&self, pointers: &[Pointer]) {
        let mut ecache = self.entity_cache.lock();
        let mut pcache = self.pointer_cache.lock();

        for p in pointers {
            let key = p.to_lowercase();
            if let Some(Some(eid)) = pcache.get(&key) {
                ecache.insert(eid.clone(), CachedEntity::NotActive);
            }
            pcache.insert(key, None);
        }
    }

    pub fn get_cached(&self, id_or_pointer: &str) -> Option<Option<String>> {
        let ecache = self.entity_cache.lock();
        if let Some(cached) = ecache.get(id_or_pointer) {
            return match cached {
                CachedEntity::Active(e) => Some(Some(e.id.clone())),
                CachedEntity::NotActive => Some(None),
            };
        }
        drop(ecache);

        let pcache = self.pointer_cache.lock();
        pcache.get(&id_or_pointer.to_lowercase()).cloned()
    }

    pub fn reset(&self) {
        self.entity_cache.lock().clear();
        self.pointer_cache.lock().clear();
    }

    fn invalidate_previous_for_pointers(&self, pointers: &[Pointer]) {
        let mut ecache = self.entity_cache.lock();
        let pcache = self.pointer_cache.lock();

        for p in pointers {
            let key = p.to_lowercase();
            if let Some(Some(old_eid)) = pcache.get(&key) {
                if let Some(CachedEntity::Active(old_entity)) = ecache.get(old_eid) {
                    let _old_pointers: Vec<String> =
                        old_entity.pointers.iter().map(|p| p.to_lowercase()).collect();
                    let old_id = old_eid.clone();
                    ecache.insert(old_id, CachedEntity::NotActive);
                }
            }
        }
    }

    async fn find_entities_by_ids(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<Entity>, BackendError> {
        let deployments = self
            .database
            .get_active_deployments(Some(entity_ids), None)
            .await?;

        let entities = map_deployments_to_entities(&deployments);

        self.update_cache_from_entities(&entities, Some(entity_ids), None);

        Ok(entities)
    }

    async fn find_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Entity>, BackendError> {
        let deployments = self
            .database
            .get_active_deployments(None, Some(pointers))
            .await?;

        let entities = map_deployments_to_entities(&deployments);

        self.update_cache_from_entities(&entities, None, Some(pointers));

        Ok(entities)
    }

    fn update_cache_from_entities(
        &self,
        entities: &[Entity],
        queried_ids: Option<&[String]>,
        queried_pointers: Option<&[String]>,
    ) {
        let mut ecache = self.entity_cache.lock();
        let mut pcache = self.pointer_cache.lock();

        if ecache.len() + entities.len() > self.config.cache_size {
            ecache.clear();
            pcache.clear();
        }

        for entity in entities {
            ecache.insert(entity.id.clone(), CachedEntity::Active(entity.clone()));
            for p in &entity.pointers {
                pcache.insert(p.to_lowercase(), Some(entity.id.clone()));
            }
        }

        if let Some(ids) = queried_ids {
            let found_ids: HashSet<&str> = entities.iter().map(|e| e.id.as_str()).collect();
            for id in ids {
                if !found_ids.contains(id.as_str()) {
                    ecache.insert(id.clone(), CachedEntity::NotActive);
                    debug!(entity_id = id, "entity has no active deployment");
                }
            }
        }

        if let Some(ptrs) = queried_pointers {
            let found_ptrs: HashSet<String> = entities
                .iter()
                .flat_map(|e| e.pointers.iter().map(|p| p.to_lowercase()))
                .collect();
            for p in ptrs {
                let lp = p.to_lowercase();
                if !found_ptrs.contains(&lp) {
                    pcache.insert(lp.clone(), None);
                    debug!(pointer = p, "pointer has no active entity");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityType;

    #[test]
    fn cached_entity_helpers() {
        let active = CachedEntity::Active(Entity {
            id: "abc".into(),
            entity_type: EntityType::Scene,
            pointers: vec!["0,0".into()],
            timestamp: 0,
            content: None,
            metadata: None,
        });
        assert!(active.is_active());
        assert!(active.as_entity().is_some());

        let not_active = CachedEntity::NotActive;
        assert!(!not_active.is_active());
        assert!(not_active.as_entity().is_none());
    }
}
