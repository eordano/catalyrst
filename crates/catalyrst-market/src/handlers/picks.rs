use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth_chain::{
    self, build_payload, AuthChainError, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
};
use crate::http::response::ApiError;
use crate::ports::lists::is_uuid;
use crate::AppState;

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn authenticate(
    headers: &HeaderMap,
    method: &str,
    fallback_path: &str,
) -> Result<String, ApiError> {
    let chain = auth_chain::extract_auth_chain(headers).map_err(auth_chain_error_to_api)?;

    let timestamp = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| auth_chain_error_to_api(AuthChainError::MissingTimestamp))?;
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("{}");

    let path = signed_fetch_path(headers, fallback_path);
    let payload = build_payload(method, path.as_ref(), timestamp, metadata);

    let now = Utc::now().timestamp();
    let recovered = auth_chain::validate_signature(&chain, &payload, timestamp, FIVE_MINUTES, now)
        .map_err(auth_chain_error_to_api)?;
    Ok(recovered.to_lowercase())
}

#[derive(Debug, Default, Deserialize)]
pub struct PickUnpickInBulkBody {
    #[serde(default, rename = "pickedFor")]
    pub picked_for: Option<Vec<String>>,
    #[serde(default, rename = "unpickedFrom")]
    pub unpicked_from: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PickUnpickResult {
    #[serde(rename = "pickedByUser")]
    #[cfg_attr(feature = "ts", ts(rename = "pickedByUser"))]
    pub picked_by_user: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PickUnpickEnvelope {
    pub ok: bool,
    pub data: PickUnpickResult,
}

fn validate_list_ids(ids: &[String]) -> Result<(), ApiError> {
    if ids.iter().any(|id| !is_uuid(id)) {
        return Err(ApiError::bad_request("list ids must be UUIDs"));
    }
    Ok(())
}

pub async fn pick_unpick_in_bulk(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<PickUnpickInBulkBody>>,
) -> Result<Json<PickUnpickEnvelope>, ApiError> {
    let user = authenticate(&headers, "post", &format!("/v1/picks/{item_id}"))?;

    let body = body.map(|Json(b)| b).unwrap_or_default();
    let picked_for = body.picked_for.unwrap_or_default();
    let unpicked_from = body.unpicked_from.unwrap_or_default();

    if picked_for.iter().any(|id| unpicked_from.contains(id)) {
        return Err(ApiError::bad_request(
            "The item cannot be be picked and unpicked from a list at the same time.",
        ));
    }
    validate_list_ids(&picked_for)?;
    validate_list_ids(&unpicked_from)?;

    if !state.lists.item_exists(&item_id).await? {
        return Err(ApiError::not_found(format!(
            "The item trying to get saved doesn't exist: {item_id}"
        )));
    }

    let (pick_ids, unpick_ids) = if picked_for.is_empty() && unpicked_from.is_empty() {
        (
            vec![state.lists.get_or_create_default_list(&user).await?],
            Vec::new(),
        )
    } else {
        let mut all = picked_for.clone();
        all.extend(unpicked_from.iter().cloned());
        all.sort();
        all.dedup();
        let owned = state.lists.count_owned_lists(&user, &all).await?;
        if owned != all.len() {
            return Err(ApiError::not_found("Some of the lists were not found."));
        }
        (picked_for, unpicked_from)
    };

    state
        .lists
        .pick_in_lists(&item_id, &user, &pick_ids)
        .await?;
    state
        .lists
        .unpick_from_lists(&item_id, &user, &unpick_ids)
        .await?;

    let picked_by_user = if !pick_ids.is_empty() {
        true
    } else {
        state.lists.is_picked_by_user(&item_id, &user).await?
    };

    Ok(Json(PickUnpickEnvelope {
        ok: true,
        data: PickUnpickResult { picked_by_user },
    }))
}

pub async fn unpick_everywhere(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PickUnpickEnvelope>, ApiError> {
    let user = authenticate(&headers, "delete", &format!("/v1/picks/{item_id}"))?;
    state.lists.unpick_everywhere(&item_id, &user).await?;
    Ok(Json(PickUnpickEnvelope {
        ok: true,
        data: PickUnpickResult {
            picked_by_user: false,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_serializes_camel_case() {
        let env = PickUnpickEnvelope {
            ok: true,
            data: PickUnpickResult {
                picked_by_user: true,
            },
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({ "ok": true, "data": { "pickedByUser": true } })
        );
    }

    #[test]
    fn body_parses_upstream_shape_and_defaults() {
        let b: PickUnpickInBulkBody =
            serde_json::from_value(json!({ "pickedFor": ["a"], "unpickedFrom": null })).unwrap();
        assert_eq!(b.picked_for.as_deref(), Some(&["a".to_string()][..]));
        assert!(b.unpicked_from.is_none());

        let empty: PickUnpickInBulkBody = serde_json::from_value(json!({})).unwrap();
        assert!(empty.picked_for.is_none() && empty.unpicked_from.is_none());
    }

    #[test]
    fn uuid_validation() {
        assert!(is_uuid("6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0001"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid(""));
        assert!(!is_uuid("6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a000g"));
        assert!(validate_list_ids(&["nope".to_string()]).is_err());
        assert!(validate_list_ids(&[]).is_ok());
    }
}
