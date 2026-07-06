use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::errors::{ApiError, ApiResult};
use crate::registry::RegistryAppState;
use crate::types::{ActiveEntity, CompactProfile, IdsBody, MAX_POINTERS};

pub async fn post_profiles(
    State(state): State<RegistryAppState>,
    Json(body): Json<IdsBody>,
) -> ApiResult<Json<Vec<Value>>> {
    let ids = validate_ids(body.ids)?;
    let profiles = state.content.resolve_profiles(&ids).await?;
    let base = &state.profile_images_url;
    Ok(Json(
        profiles
            .into_iter()
            .map(|e| sanitized_profile(&e, base))
            .collect(),
    ))
}

pub async fn post_profiles_metadata(
    State(state): State<RegistryAppState>,
    Json(body): Json<IdsBody>,
) -> ApiResult<Json<Vec<CompactProfile>>> {
    let ids = validate_ids(body.ids)?;
    let profiles = state.content.resolve_profiles(&ids).await?;
    let base = &state.profile_images_url;
    Ok(Json(
        profiles.iter().map(|e| compact_profile(e, base)).collect(),
    ))
}

fn validate_ids(ids: Vec<String>) -> Result<Vec<String>, ApiError> {
    if ids.is_empty() {
        return Err(ApiError::bad_request("ids must be a non-empty array"));
    }
    if ids.len() > MAX_POINTERS {
        return Err(ApiError::bad_request(format!(
            "too many ids: {} (max {})",
            ids.len(),
            MAX_POINTERS
        )));
    }
    Ok(ids)
}

fn face_url(base: &str, entity_id: &str) -> String {
    format!("{base}/entities/{entity_id}/face.png")
}

fn body_url(base: &str, entity_id: &str) -> String {
    format!("{base}/entities/{entity_id}/body.png")
}

fn first_avatar(metadata: &Value) -> Option<&Value> {
    metadata
        .get("avatars")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
}

fn sanitized_profile(ent: &ActiveEntity, base: &str) -> Value {
    let face = face_url(base, &ent.entity_id);
    let body = body_url(base, &ent.entity_id);

    let avatars: Vec<Value> = ent
        .metadata
        .get("avatars")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .map(|avatar| rewrite_avatar(avatar, &face, &body))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "timestamp": ent.timestamp,
        "avatars": avatars,
    })
}

fn rewrite_avatar(avatar: &Value, face: &str, body: &str) -> Value {
    let mut avatar = avatar.clone();
    if let Some(obj) = avatar.as_object_mut() {
        if let Some(inner) = obj.get_mut("avatar").and_then(|a| a.as_object_mut()) {
            inner.insert(
                "snapshots".to_string(),
                json!({ "face256": face, "body": body }),
            );
        }
    }
    avatar
}

fn compact_profile(ent: &ActiveEntity, base: &str) -> CompactProfile {
    let pointer = ent.pointers.first().cloned().unwrap_or_default();
    let avatar = first_avatar(&ent.metadata);

    let name = avatar
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();

    let has_claimed_name = avatar
        .and_then(|a| a.get("hasClaimedName"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let name_color = avatar
        .and_then(|a| a.get("nameColor"))
        .filter(|v| !v.is_null())
        .cloned();

    CompactProfile {
        pointer,
        has_claimed_name,
        name,
        name_color,

        thumbnail_url: face_url(base, &ent.entity_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::response::IntoResponse;
    use serde_json::json;

    use crate::registry::testutil::{entity, open_state};
    use crate::types::ActiveEntity;

    fn profile_entity(addr: &str, id: &str) -> ActiveEntity {
        entity(
            id,
            "profile",
            &[addr],
            json!({
                "avatars": [{
                    "name": "Tester",
                    "hasClaimedName": true,
                    "nameColor": "#ff0000",
                    "avatar": {
                        "bodyShape": "urn:decentraland:off-chain:base-avatars:BaseFemale",
                        "snapshots": {
                            "face256": "https://leaky.example/face",
                            "body": "https://leaky.example/body"
                        }
                    }
                }]
            }),
        )
    }

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dclc_profiles_{tag}_{}", std::process::id()))
    }

    #[tokio::test]
    async fn profiles_rewrite_snapshots_to_profile_images_base() {
        let tmp = tmp_root("rewrite");
        let state = open_state(vec![profile_entity("0xabc123", "bafkprof1")], &tmp);

        let Json(out) = post_profiles(
            State(state),
            Json(IdsBody {
                ids: vec!["0xABC123".to_string()],
            }),
        )
        .await
        .unwrap();

        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["timestamp"], json!(1_700_000_000_000i64));
        let avatar = &out[0]["avatars"][0];
        assert_eq!(avatar["name"], "Tester");
        assert_eq!(
            avatar["avatar"]["snapshots"],
            json!({
                "face256": "https://profile-images.example/entities/bafkprof1/face.png",
                "body": "https://profile-images.example/entities/bafkprof1/body.png"
            })
        );
    }

    #[tokio::test]
    async fn profiles_metadata_compact_shape() {
        let tmp = tmp_root("compact");
        let state = open_state(vec![profile_entity("0xabc123", "bafkprof2")], &tmp);

        let Json(out) = post_profiles_metadata(
            State(state),
            Json(IdsBody {
                ids: vec!["0xabc123".to_string()],
            }),
        )
        .await
        .unwrap();

        let got = serde_json::to_value(&out).unwrap();
        assert_eq!(
            got,
            json!([{
                "pointer": "0xabc123",
                "hasClaimedName": true,
                "name": "Tester",
                "nameColor": "#ff0000",
                "thumbnailUrl": "https://profile-images.example/entities/bafkprof2/face.png"
            }])
        );
    }

    #[tokio::test]
    async fn profiles_validate_ids_bounds() {
        let tmp = tmp_root("validate");
        let state = open_state(Vec::new(), &tmp);

        let empty = post_profiles(State(state.clone()), Json(IdsBody { ids: Vec::new() }))
            .await
            .err()
            .unwrap();
        assert_eq!(empty.into_response().status().as_u16(), 400);

        let too_many = post_profiles(
            State(state.clone()),
            Json(IdsBody {
                ids: vec!["0xdead".to_string(); MAX_POINTERS + 1],
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(too_many.into_response().status().as_u16(), 400);

        let Json(unknown) = post_profiles(
            State(state),
            Json(IdsBody {
                ids: vec!["0xunknown".to_string()],
            }),
        )
        .await
        .unwrap();
        assert!(unknown.is_empty());
    }
}
