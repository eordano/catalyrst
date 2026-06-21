use std::sync::Arc;

use axum::extract::{OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use catalyrst_validator::squid_checker::check_parcel_access;

use crate::errors::{AppError, AppResult, InvalidRequestError, NotFoundError};
use crate::land_operators::SubgraphLandOperatorResolver;
use crate::land_publish::{tombstone_and_repoint, UnpublishError};
use crate::signed_fetch::{require_verified, AuthChainError};
use crate::state::AppState;

fn parse_parcel(coord: &str) -> Option<(i32, i32)> {
    let (x, y) = coord.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn map_auth_error(e: AuthChainError) -> AppError {
    match e {
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => InvalidRequestError::new(e.to_string()).into(),
        _ => AppError::Unauthorized(e.to_string()),
    }
}

pub async fn unpublish_scene(
    State(state): State<Arc<AppState>>,
    Path(coord): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let auth = require_verified(&headers, "delete", uri.path()).map_err(map_auth_error)?;
    let signer = auth.signer.to_lowercase();

    let (x, y) = parse_parcel(&coord).ok_or_else(|| {
        InvalidRequestError::new(format!(
            "Scene pointers should only contain two integers separated by a comma, \
             for example (10,10) or (120,-45). Invalid pointer: {coord}"
        ))
    })?;

    let squid_pool = state.squid_pool.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "LAND ownership checks are unavailable on this node".to_string(),
        )
    })?;
    let content_pool = state.content_pool.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable("content database is unavailable".to_string())
    })?;

    let resolver = SubgraphLandOperatorResolver::new(state.eth_network.clone());
    let allowed = check_parcel_access(squid_pool, Some(&resolver), &signer, x, y)
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("parcel access check failed: {e}")))?;
    if !allowed {
        return Err(AppError::Forbidden(format!(
            "The provided Eth Address does not have access to the following parcel: ({x},{y})"
        )));
    }

    let pointer = format!("{x},{y}");
    match tombstone_and_repoint(content_pool, &pointer).await {
        Ok(outcome) => {
            state.deployments_cache.clear();
            tracing::info!(
                entity_id = %outcome.entity_id,
                signer = %signer,
                pointer = %pointer,
                repointed = outcome.repointed.len(),
                "DELETE /scenes - local publish tombstoned"
            );
            let parcels: Vec<&str> = outcome.repointed.iter().map(|(p, _)| p.as_str()).collect();
            Ok(Json(json!({
                "entityId": outcome.entity_id,
                "unpublished": true,
                "parcels": parcels,
            })))
        }
        Err(UnpublishError::NotLocal(_)) => {
            Err(NotFoundError::new(format!("No locally published scene at {x},{y}.")).into())
        }
        Err(UnpublishError::Conflict) => Err(AppError::Conflict(
            "A concurrent deployment changed the parcel; please retry.".to_string(),
        )),
        Err(UnpublishError::Db(e)) => Err(AppError::Internal(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_parcel_accepts_signed_coords() {
        assert_eq!(parse_parcel("52,-52"), Some((52, -52)));
        assert_eq!(parse_parcel(" -10 , 20 "), Some((-10, 20)));
    }

    #[test]
    fn parse_parcel_rejects_garbage() {
        assert_eq!(parse_parcel("52"), None);
        assert_eq!(parse_parcel("a,b"), None);
        assert_eq!(parse_parcel("1,2,3"), None);
        assert_eq!(parse_parcel(""), None);
    }
}
