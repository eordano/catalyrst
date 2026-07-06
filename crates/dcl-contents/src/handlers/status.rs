use axum::extract::{Path, Query, State};
use axum::Json;

use crate::errors::{ApiError, ApiResult};
use crate::manifest_store::AbManifests;
use crate::registry::RegistryAppState;
use crate::types::{BuildStatus, EntityStatus, PlatformStatuses, WorldNameQuery};

pub async fn get_entity_status(
    State(state): State<RegistryAppState>,
    Path(id): Path<String>,
    Query(q): Query<WorldNameQuery>,
) -> ApiResult<Json<EntityStatus>> {
    let resolved = if let Some(world) = q.world_name.as_deref() {
        let ents = state
            .content
            .resolve_pointers(std::slice::from_ref(&id))
            .await?;
        ents.into_iter().find(|e| {
            e.world_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(world))
        })
    } else {
        state.content.resolve_one(&id).await?
    };

    let ent = resolved.ok_or_else(|| ApiError::not_found("entity not found"))?;
    let m = state.manifests.get(&ent.entity_id).await;
    Ok(Json(entity_status_from(&ent.entity_id, &m, ent.is_world())))
}

pub fn entity_status_from(entity_id: &str, m: &AbManifests, is_world: bool) -> EntityStatus {
    let asset_bundles = PlatformStatuses {
        mac: m.mac_status(),
        windows: m.windows_status(),
        linux: m.linux_status(),
    };
    let complete = matches!(asset_bundles.mac, BuildStatus::Complete)
        && matches!(asset_bundles.windows, BuildStatus::Complete)
        && matches!(asset_bundles.linux, BuildStatus::Complete);
    let lods = if is_world {
        None
    } else {
        Some(PlatformStatuses {
            mac: m.lods.mac.unwrap_or(BuildStatus::Pending),
            windows: m.lods.windows.unwrap_or(BuildStatus::Pending),
            linux: m
                .lods
                .windows
                .or(m.lods.mac)
                .unwrap_or(BuildStatus::Pending),
        })
    };
    EntityStatus {
        entity_id: entity_id.to_string(),
        catalyst: BuildStatus::Complete,
        complete,
        asset_bundles,
        lods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use axum::response::IntoResponse;
    use serde_json::json;

    use crate::registry::testutil::{entity, open_state};

    fn write_manifest(root: &std::path::Path, id: &str, platform: &str, exit: i32) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{platform}.manifest.json")),
            serde_json::to_string(&json!({
                "version": "v41",
                "exitCode": exit,
                "date": "2024-03-15T12:34:56.789Z",
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dclc_status_{tag}_{}", std::process::id()))
    }

    #[tokio::test]
    async fn status_by_id_reads_out_root_manifests() {
        let tmp = tmp_root("byid");
        let id = "bafkstatusentity";
        for platform in ["windows", "mac", "linux"] {
            write_manifest(&tmp, id, platform, 0);
        }
        let state = open_state(vec![entity(id, "scene", &["0,0"], json!({}))], &tmp);

        let Json(got) = get_entity_status(
            State(state.clone()),
            Path(id.to_string()),
            Query(WorldNameQuery { world_name: None }),
        )
        .await
        .unwrap();
        assert_eq!(got.entity_id, id);
        assert!(got.complete);
        assert!(matches!(got.asset_bundles.windows, BuildStatus::Complete));
        assert!(matches!(got.asset_bundles.mac, BuildStatus::Complete));
        assert!(matches!(got.asset_bundles.linux, BuildStatus::Complete));
        assert!(got.lods.is_some());

        let missing = get_entity_status(
            State(state),
            Path("bafkunknownentity".to_string()),
            Query(WorldNameQuery { world_name: None }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(missing.into_response().status().as_u16(), 404);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn status_world_name_filters_pointer_matches() {
        let tmp = tmp_root("world");
        let pointer = "myworld.dcl.eth";
        let hit = entity(
            "bafkworldhit12",
            "scene",
            &[pointer],
            json!({"worldConfiguration": {"name": "myworld.dcl.eth"}}),
        );
        let other = entity(
            "bafkworldother",
            "scene",
            &[pointer],
            json!({"worldConfiguration": {"name": "other.dcl.eth"}}),
        );
        let state = open_state(vec![other, hit], &tmp);

        let Json(got) = get_entity_status(
            State(state.clone()),
            Path(pointer.to_string()),
            Query(WorldNameQuery {
                world_name: Some("MYWORLD.dcl.eth".to_string()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(got.entity_id, "bafkworldhit12");
        assert!(got.lods.is_none());

        let miss = get_entity_status(
            State(state),
            Path(pointer.to_string()),
            Query(WorldNameQuery {
                world_name: Some("nosuch.dcl.eth".to_string()),
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(miss.into_response().status().as_u16(), 404);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
