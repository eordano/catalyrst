use axum::extract::State;
use axum::Json;

use crate::http::errors::ApiResult;
use crate::types::QueuesStatus;
use crate::AppState;

const QUEUE_SCAN_LIMIT: i64 = 2000;

pub async fn get_queues_status(
    State(state): State<AppState>,
) -> ApiResult<Json<QueuesStatus>> {
    let ids = state
        .content
        .active_entity_ids_of_types(&["scene", "wearable", "emote"], QUEUE_SCAN_LIMIT)
        .await?;

    let mut q = QueuesStatus::default();
    for id in ids {
        let m = state.manifests.get(&id).await;
        if !matches!(m.windows_status(), crate::types::BuildStatus::Complete) {
            q.windows_pending_jobs.push(id.clone());
        }
        if !matches!(m.mac_status(), crate::types::BuildStatus::Complete) {
            q.mac_pending_jobs.push(id.clone());
        }
        if !matches!(m.webgl_status(), crate::types::BuildStatus::Complete) {
            q.webgl_pending_jobs.push(id);
        }
    }
    Ok(Json(q))
}
