use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::AppState;

/// `GET /world/{world_name}/preview-wearables` -- the smart wearables this realm
/// is previewing.
///
/// The stock client fetches this on entry, takes `data[0]`, resolves the
/// `scene.json` inside its first representation and runs it as a live scene
/// (`LoadSmartWearablePreviewSceneSystem`). That makes the answer executable, so
/// it is never inferred: it is exactly the URNs an operator selected for this
/// world, or node-wide via `PREVIEW_WEARABLE_URNS`. A normally deployed world
/// selects none and correctly answers with an empty list -- the client treats
/// that as "not previewing anything" and moves on.
#[utoipa::path(
    get,
    path = "/world/{world_name}/preview-wearables",
    tag = "worlds",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_preview_wearables(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let selected = state
        .worlds
        .get_world(&world_name)
        .await?
        .and_then(|w| w.preview_wearable_urns)
        .filter(|urns| !urns.is_empty())
        .unwrap_or_else(|| state.cfg.preview_wearable_urns.clone());

    if selected.is_empty() {
        return Ok(Json(json!({ "ok": true, "data": [] })));
    }

    let entities = fetch_active_entities(&state, &selected).await;
    let content_base = state.cfg.content_public_url.trim_end_matches('/');
    let data: Vec<Value> = entities
        .iter()
        .filter_map(|e| preview_wearable(e, content_base))
        .collect();

    Ok(Json(json!({ "ok": true, "data": data })))
}

/// A failure here is not the visitor's problem: the realm is still enterable and
/// the client's own contract is that an empty list means "nothing previewed", so
/// an unreachable content server degrades to that rather than to a 500 in the
/// middle of someone's entry.
async fn fetch_active_entities(state: &AppState, pointers: &[String]) -> Vec<Value> {
    let url = format!(
        "{}/entities/active",
        state.cfg.content_public_url.trim_end_matches('/')
    );
    let resp = state
        .http
        .post(&url)
        .json(&json!({ "pointers": pointers }))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => r.json::<Vec<Value>>().await.unwrap_or_default(),
        Ok(r) => {
            tracing::warn!(
                url,
                status = r.status().as_u16(),
                "preview-wearables: content server rejected the pointer lookup"
            );
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(url, error = %e, "preview-wearables: content server unreachable");
            Vec::new()
        }
    }
}

/// Reshapes a wearable entity into what the client parses. The `hash` alongside
/// each `url` is what makes this usable rather than decorative: the client turns
/// it into the previewed scene's content mapping, so without it the scene loads
/// with no files.
fn preview_wearable(entity: &Value, content_base: &str) -> Option<Value> {
    let id = entity
        .get("pointers")
        .and_then(|p| p.as_array())
        .and_then(|p| p.first())
        .and_then(|p| p.as_str())
        .or_else(|| entity.get("id").and_then(|i| i.as_str()))?;

    let hash_of = |file: &str| -> Option<&str> {
        entity
            .get("content")?
            .as_array()?
            .iter()
            .find(|c| c.get("file").and_then(|f| f.as_str()) == Some(file))?
            .get("hash")?
            .as_str()
    };

    let representations: Vec<Value> = entity
        .get("metadata")
        .and_then(|m| m.get("data"))
        .and_then(|d| d.get("representations"))
        .and_then(|r| r.as_array())
        .map(|reps| {
            reps.iter()
                .map(|rep| {
                    let contents: Vec<Value> = rep
                        .get("contents")
                        .and_then(|c| c.as_array())
                        .map(|files| {
                            files
                                .iter()
                                .filter_map(|f| f.as_str())
                                .filter_map(|file| {
                                    let hash = hash_of(file)?;
                                    Some(json!({
                                        "key": file,
                                        "url": format!("{content_base}/contents/{hash}"),
                                        "hash": hash,
                                    }))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    json!({ "contents": contents })
                })
                .collect()
        })
        .unwrap_or_default();

    // A representation with no scene.json is a plain wearable, not a smart one;
    // the client logs an error and gives up on it, so it is dropped here instead.
    let runnable = representations.iter().any(|rep| {
        rep["contents"]
            .as_array()
            .map(|c| {
                c.iter().any(|item| {
                    item["key"]
                        .as_str()
                        .map(|k| k.to_ascii_lowercase().ends_with("scene.json"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });
    if !runnable {
        return None;
    }

    Some(json!({
        "id": id,
        "data": { "representations": representations },
    }))
}

#[cfg(test)]
mod tests {
    use super::preview_wearable;
    use serde_json::json;

    fn smart_wearable() -> serde_json::Value {
        json!({
            "id": "bafyEntity",
            "pointers": ["urn:decentraland:off-chain:base-avatars:smart-hat"],
            "content": [
                { "file": "male/scene.json", "hash": "QmScene" },
                { "file": "male/hat.glb", "hash": "QmHat" }
            ],
            "metadata": {
                "data": {
                    "representations": [
                        { "contents": ["male/scene.json", "male/hat.glb"] }
                    ]
                }
            }
        })
    }

    #[test]
    fn maps_contents_to_key_url_and_hash() {
        let out = preview_wearable(&smart_wearable(), "https://node/content").expect("smart");
        assert_eq!(
            out["id"],
            json!("urn:decentraland:off-chain:base-avatars:smart-hat")
        );
        let contents = &out["data"]["representations"][0]["contents"];
        assert_eq!(contents[0]["key"], json!("male/scene.json"));
        assert_eq!(
            contents[0]["url"],
            json!("https://node/content/contents/QmScene")
        );
        // The client builds the previewed scene's content mapping out of these
        // hashes; a url alone would load a scene that can reach none of its files.
        assert_eq!(contents[0]["hash"], json!("QmScene"));
        assert_eq!(contents[1]["hash"], json!("QmHat"));
    }

    #[test]
    fn drops_a_wearable_with_no_scene_json() {
        let mut plain = smart_wearable();
        plain["content"] = json!([{ "file": "male/hat.glb", "hash": "QmHat" }]);
        plain["metadata"]["data"]["representations"] = json!([{ "contents": ["male/hat.glb"] }]);
        assert!(preview_wearable(&plain, "https://node/content").is_none());
    }

    #[test]
    fn drops_a_content_entry_the_entity_does_not_carry() {
        let mut missing = smart_wearable();
        missing["content"] = json!([{ "file": "male/scene.json", "hash": "QmScene" }]);
        let out = preview_wearable(&missing, "https://node/content").expect("smart");
        let contents = out["data"]["representations"][0]["contents"]
            .as_array()
            .expect("array")
            .len();
        assert_eq!(contents, 1);
    }
}
