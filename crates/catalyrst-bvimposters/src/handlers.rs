use axum::body::Bytes;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::key::{parse_spec_request, parse_zip_request, TileKey};
use crate::supply::{Served, Source};
use crate::AppState;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}

pub async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let usage = state.store.usage_snapshot();
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
    headers: HeaderMap,
) -> Response {
    if file.ends_with("-spec.json") {
        return spec(&state, &level, &file).await;
    }
    let Some(key) = parse_zip_request(&level, &file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if etag_matches(&headers, key.crc) {
        return not_modified(key.crc);
    }
    let result = if state.quarantine_list.contains(&key) {
        tracing::debug!(%level, %file, "read-through quarantined");
        Ok(state.supply.get_stored(&key).await)
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
    match state.supply.spec(&key).await {
        Ok(Some(spec)) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            spec,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(%level, %file, error = %e, "stored zip missing spec member");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

fn etag_value(crc: u32) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!("\"{crc}\"")).ok()
}

// The crc in the key is the ETag; a matching If-None-Match needs no store work.
fn etag_matches(headers: &HeaderMap, crc: u32) -> bool {
    let Some(raw) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let wanted = format!("\"{crc}\"");
    raw.split(',').map(str::trim).any(|tag| {
        tag == "*" || tag == wanted || tag.strip_prefix("W/").is_some_and(|t| t == wanted)
    })
}

fn not_modified(crc: u32) -> Response {
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    if let Some(etag) = etag_value(crc) {
        headers.insert(header::ETAG, etag);
    }
    response
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

fn zip_response(bytes: Bytes, crc: u32, source: Source) -> Response {
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
    if let Some(etag) = etag_value(crc) {
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
        let supply = Supply::new(store.clone(), 64 << 20);
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
        get_zip_with(state, key, HeaderMap::new()).await
    }

    async fn get_zip_with(state: &AppState, key: &ImposterKey, headers: HeaderMap) -> Response {
        imposter(
            State(state.clone()),
            Path((
                "content".to_string(),
                key.tile.level.to_string(),
                key.zip_name(),
            )),
            headers,
        )
        .await
    }

    #[tokio::test]
    async fn if_none_match_on_the_crc_etag_short_circuits() {
        let dir = tempfile::tempdir().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = mock_cdn(hits.clone()).await;
        let state = state_with(dir.path(), base, &[]);
        let key = ImposterKey::new(0, 2, 100, 777).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("\"777\""));
        let response = get_zip_with(&state, &key, headers).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()[header::ETAG], "\"777\"");
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(!state.store.zip_path(&key).exists());

        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("W/\"1\", \"777\""),
        );
        let response = get_zip_with(&state, &key, headers).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("\"778\""));
        let response = get_zip_with(&state, &key, headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::ETAG], "\"777\"");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn spec_is_served_from_the_stored_zip() {
        let dir = tempfile::tempdir().unwrap();
        let base = mock_cdn(Arc::new(AtomicUsize::new(0))).await;
        let state = state_with(dir.path(), base, &[]);
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let path = |state: &AppState| {
            imposter(
                State(state.clone()),
                Path((
                    "content".to_string(),
                    "0".to_string(),
                    "0,100.3504527830-spec.json".to_string(),
                )),
                HeaderMap::new(),
            )
        };
        assert_eq!(path(&state).await.status(), StatusCode::NOT_FOUND);
        std::fs::create_dir_all(state.store.level_dir(0)).unwrap();
        std::fs::write(
            state.store.zip_path(&key),
            crate::zips::test_zip_bytes(0, 100, 3504527830),
        )
        .unwrap();
        let response = path(&state).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
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
