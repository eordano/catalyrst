use std::collections::HashSet;

use crate::ports::content::ActiveEntity;
use crate::types::{DbEntity, EntityVersions};
use crate::AppState;

pub async fn resolve_db_entities(
    state: &AppState,
    pointers: &[String],
    world_name: Option<&str>,
) -> Result<Vec<DbEntity>, sqlx::Error> {
    let (entities, denylist) = gather(state, pointers, world_name, true).await?;

    let mut out = Vec::with_capacity(entities.len());
    for ent in entities {
        if denylist.contains(&ent.entity_id) {
            continue;
        }
        let m = state.manifests.get(&ent.entity_id).await;
        let status = m.registry_status(true);
        if !status.is_servable() {
            continue;
        }
        let is_world = ent.is_world();
        out.push(DbEntity {
            id: ent.entity_id,
            entity_type: ent.entity_type,
            timestamp: ent.timestamp,
            pointers: ent.pointers,
            content: ent.content,
            metadata: ent.metadata,
            deployer: ent
                .deployer_address
                .map(|d| d.to_lowercase())
                .unwrap_or_default(),
            status,
            bundles: m.bundles(is_world),
            versions: m.versions(),
        });
    }
    Ok(out)
}

pub async fn resolve_versions(
    state: &AppState,
    pointers: &[String],
    world_name: Option<&str>,
) -> Result<Vec<EntityVersions>, sqlx::Error> {
    let (entities, _denylist) = gather(state, pointers, world_name, false).await?;

    let mut out = Vec::with_capacity(entities.len());
    for ent in entities {
        let m = state.manifests.get(&ent.entity_id).await;
        let status = m.registry_status(true);
        if !status.is_servable() {
            continue;
        }
        let is_world = ent.is_world();
        out.push(EntityVersions {
            pointers: ent.pointers,
            versions: m.versions(),
            bundles: m.bundles(is_world),
            status,
        });
    }
    Ok(out)
}

async fn gather(
    state: &AppState,
    pointers: &[String],
    world_name: Option<&str>,
    exclude_denylisted: bool,
) -> Result<(Vec<ActiveEntity>, HashSet<String>), sqlx::Error> {
    let mut entities = state.content.resolve_pointers(pointers).await?;

    if let Some(world) = world_name {
        entities.retain(|e| {
            e.world_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(world))
        });
    }

    let denylist = if exclude_denylisted {
        state.registry.denylist_set().await?
    } else {
        HashSet::new()
    };
    Ok((entities, denylist))
}
