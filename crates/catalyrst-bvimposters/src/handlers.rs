use axum::extract::{OriginalUri, Path, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::key::{parse_spec_request, parse_zip_request, TileKey};
use crate::supply::{Served, Source};
use crate::AppState;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}

pub async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.store.clone();
    let usage = tokio::task::spawn_blocking(move || store.usage())
        .await
        .unwrap_or_default();
    let (bake_queue, bake_inflight) = state
        .bake
        .as_ref()
        .map(|bake| bake.snapshot())
        .unwrap_or_default();
    let quarantine: Vec<_> = state
        .quarantine
        .entries()
        .into_iter()
        .map(|(key, entry)| json!({"key": key, "until": entry.until, "failures": entry.failures}))
        .collect();
    axum::Json(json!({
        "store_bytes": usage.bytes,
        "store_entries": usage.entries,
        "budget_bytes": state.store.max_bytes(),
        "bake_enabled": state.bake.is_some(),
        "bake_queue": bake_queue.iter().map(TileKey::label).collect::<Vec<_>>(),
        "bake_inflight": bake_inflight.map(|tile| tile.label()),
        "quarantine": quarantine,
        "readthrough_quarantine": json!({
            "path": state.quarantine_list.path().display().to_string(),
            "keys": state.quarantine_list.len(),
        }),
    }))
}

pub async fn imposter(
    State(state): State<AppState>,
    Path((_realm, level, file)): Path<(String, String, String)>,
) -> Response {
    if file.ends_with("-spec.json") {
        return spec(&state, &level, &file).await;
    }
    let Some(key) = parse_zip_request(&level, &file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let result = if state.quarantine_list.contains(&key) {
        tracing::debug!(%level, %file, "read-through quarantined");
        state.supply.get(&key, || async { Ok(None) }).await
    } else {
        state.supply.get(&key, || state.cdn.fetch(&key)).await
    };
    let served = match result {
        Ok(served) => served,
        Err(e) => {
            tracing::warn!(%level, %file, error = %e, "read-through failed");
            Served::Miss
        }
    };
    match served {
        Served::Hit(bytes, source) => zip_response(bytes, key.crc, source),
        Served::Miss => {
            maybe_enqueue_bake(&state, key.tile);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn spec(state: &AppState, level: &str, file: &str) -> Response {
    let Some(key) = parse_spec_request(level, file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(bytes) = state.store.read_hit(&key).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match crate::zips::extract_spec(&bytes, &key) {
        Ok(spec) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            spec,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(%level, %file, error = %e, "stored zip missing spec member");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

fn maybe_enqueue_bake(state: &AppState, tile: TileKey) {
    let Some(bake) = state.bake.as_ref() else {
        return;
    };
    if state.quarantine.is_quarantined(&tile) {
        tracing::info!(tile = %tile.label(), "bake suppressed, tile quarantined");
        return;
    }
    if bake.enqueue(tile) {
        tracing::info!(tile = %tile.label(), "bake enqueued");
    }
}

fn zip_response(bytes: Vec<u8>, crc: u32, source: Source) -> Response {
    let source_value = match source {
        Source::Store => HeaderValue::from_static("store"),
        Source::Cdn => HeaderValue::from_static("cdn"),
    };
    let mut response = (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        bytes,
    )
        .into_response();
    if let Ok(etag) = HeaderValue::from_str(&format!("\"{crc}\"")) {
        response.headers_mut().insert(header::ETAG, etag);
    }
    response
        .headers_mut()
        .insert(HeaderName::from_static("x-bvi-source"), source_value);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdn::CdnClient;
    use crate::key::ImposterKey;
    use crate::quarantine::Quarantine;
    use crate::quarantine_list::QuarantineList;
    use crate::store::Store;
    use crate::supply::Supply;
    use crate::AppStateInner;
    use axum::Router;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    async fn mock_cdn(hits: Arc<AtomicUsize>) -> String {
        let app = Router::new().route(
            "/imposters/realms/{realm}/{level}/{file}",
            axum::routing::get(
                move |Path((_realm, level, file)): Path<(String, String, String)>| {
                    let hits = hits.clone();
                    async move {
                        hits.fetch_add(1, Ordering::SeqCst);
                        let key = crate::key::parse_zip_request(&level, &file).unwrap();
                        crate::zips::test_zip_bytes(key.tile.x, key.tile.y, key.crc)
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn state_with(dir: &std::path::Path, cdn_base: String, listed: &[ImposterKey]) -> AppState {
        let store = Arc::new(Store::new(dir.join("root"), u64::MAX));
        store.init().unwrap();
        let list_path = dir.join("list.txt");
        let lines: Vec<String> = listed
            .iter()
            .map(|key| format!("{}/{}", key.tile.level, key.zip_name()))
            .collect();
        std::fs::write(&list_path, lines.join("\n")).unwrap();
        let quarantine = Arc::new(Quarantine::load(store.quarantine_path(), 3, 86400));
        let supply = Supply::new(store.clone());
        let cdn = CdnClient::new(cdn_base, "content".to_string(), 5).unwrap();
        Arc::new(AppStateInner {
            store,
            supply,
            cdn,
            quarantine,
            quarantine_list: QuarantineList::load(list_path),
            bake: None,
        })
    }

    async fn get_zip(state: &AppState, key: &ImposterKey) -> Response {
        imposter(
            State(state.clone()),
            Path((
                "content".to_string(),
                key.tile.level.to_string(),
                key.zip_name(),
            )),
        )
        .await
    }

    #[tokio::test]
    async fn quarantined_key_skips_read_through() {
        let dir = tempfile::tempdir().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = mock_cdn(hits.clone()).await;
        let listed = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let state = state_with(dir.path(), base, &[listed]);

        std::fs::create_dir_all(state.store.level_dir(0)).unwrap();
        std::fs::write(
            state.store.zip_path(&listed),
            crate::zips::test_zip_bytes(0, 100, 3504527830),
        )
        .unwrap();
        let response = get_zip(&state, &listed).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-bvi-source"], "store");
        assert_eq!(hits.load(Ordering::SeqCst), 0);

        assert!(state.store.quarantine_entry(&listed).unwrap());
        let response = get_zip(&state, &listed).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(hits.load(Ordering::SeqCst), 0);

        let healthy = ImposterKey::new(0, 2, 100, 777).unwrap();
        let response = get_zip(&state, &healthy).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-bvi-source"], "cdn");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
