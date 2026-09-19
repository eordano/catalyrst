use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::catalog_store::{CatalogStore, ContentError, UpstreamError};
use crate::http::errors::ApiError;
use crate::AppState;

const UNCONFIGURED: &str = "BUILDER_CATALOG_DIR is unset: point it at the directory written by \
    `catalyrst-builder catalog <packs> <dir>`";

fn store(state: &AppState) -> Result<&CatalogStore, ApiError> {
    state
        .catalog
        .as_deref()
        .ok_or_else(|| ApiError::http(503, UNCONFIGURED))
}

pub async fn get_asset_packs(State(state): State<AppState>) -> Response {
    let store = match store(&state) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    let body = match store.asset_packs().await {
        Ok(Some(b)) => b,
        Ok(None) => {
            return ApiError::http(
                503,
                format!(
                    "no catalog.json under {}: run `catalyrst-builder catalog <packs> <dir>`",
                    store.dir().display()
                ),
            )
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "builder catalog load failed");
            return ApiError::internal("builder catalog load failed").into_response();
        }
    };
    let mut resp = (StatusCode::OK, body).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    resp
}

const RETRY_AFTER_SECS: &str = "30";

fn upstream_failure(hash: &str, e: &UpstreamError) -> Response {
    let (status, message) = match e {
        UpstreamError::Status(s) => (
            502,
            format!("builder content bucket answered {s} for {hash}"),
        ),
        UpstreamError::Unreachable(_) => (
            503,
            format!("builder content bucket unreachable while pulling {hash}"),
        ),
        UpstreamError::Rejected(_) => (
            502,
            format!("builder content bucket served unusable bytes for {hash}"),
        ),
    };
    let mut resp = ApiError::http(status, message).into_response();
    resp.headers_mut().insert(
        header::RETRY_AFTER,
        HeaderValue::from_static(RETRY_AFTER_SECS),
    );
    resp
}

pub async fn get_content(State(state): State<AppState>, Path(hash): Path<String>) -> Response {
    let store = match store(&state) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    let content = match store.content(&hash).await {
        Ok(c) => c,
        Err(ContentError::InvalidHash) => {
            return ApiError::bad_request("hash is not a canonical CID").into_response()
        }
        Err(ContentError::NotFound) => {
            return ApiError::not_found(format!("no builder item content for {hash}"))
                .into_response()
        }
        Err(ContentError::Upstream(e)) => {
            tracing::warn!(error = %e, hash, "builder content pull-through failed");
            return upstream_failure(&hash, &e);
        }
        Err(ContentError::Io(e)) => {
            tracing::error!(error = %e, hash, "builder content store read failed");
            return ApiError::internal("builder content store read failed").into_response();
        }
    };
    let mut resp = (StatusCode::OK, content.bytes).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content.content_type),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    if let Ok(etag) = HeaderValue::from_str(&format!("\"{hash}\"")) {
        headers.insert(header::ETAG, etag);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_store::{PullThrough, CATALOG_FILE};
    use crate::ports::items::{ItemsComponent, NewsletterComponent};
    use crate::AppStateInner;
    use axum::body::to_bytes;
    use serde_json::{json, Value};
    use std::sync::Arc;

    fn dead_pool() -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/nowhere")
            .expect("lazy pool from static url")
    }

    fn state(catalog: Option<Arc<CatalogStore>>) -> AppState {
        Arc::new(AppStateInner {
            polygon_rpc_url: None,
            items: ItemsComponent::new(dead_pool()),
            newsletter: NewsletterComponent::new(dead_pool()),
            marketplace: None,
            content_bucket_url: "https://example.test".into(),
            catalog,
            admin_addresses: Vec::new(),
            newsletter_service_url: None,
            newsletter_publication_id: None,
            newsletter_api_key: None,
            admin_token: None,
            http: reqwest::Client::new(),
        })
    }

    fn temp_dir() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("builder-catalog-handler-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn body_json(resp: Response) -> Value {
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn asset_packs_is_503_without_a_catalog_dir() {
        let resp = get_asset_packs(State(state(None))).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let v = body_json(resp).await;
        assert!(v["message"]
            .as_str()
            .unwrap()
            .contains("BUILDER_CATALOG_DIR"));
    }

    #[tokio::test]
    async fn asset_packs_is_503_until_the_catalog_is_built() {
        let dir = temp_dir();
        let store = Arc::new(CatalogStore::new(&dir, None).unwrap());
        let resp = get_asset_packs(State(state(Some(store)))).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let v = body_json(resp).await;
        assert!(v["message"]
            .as_str()
            .unwrap()
            .contains("catalyrst-builder catalog"));
    }

    #[tokio::test]
    async fn asset_packs_serves_the_builder_server_envelope_and_content_by_cid() {
        let dir = temp_dir();
        let model = b"glTF\x02\x00\x00\x00fake".to_vec();
        let hash = catalyrst_hashing::hash_bytes_v1(&model);
        std::fs::write(dir.join(&hash), &model).unwrap();
        let catalog = json!({ "data": [{
            "id": "pack-1", "title": "Pack", "assets": [{
                "id": "a1", "name": "Cube", "model": "cube.glb",
                "contents": { "cube.glb": hash }
            }]
        }]});
        std::fs::write(
            dir.join(CATALOG_FILE),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        let store = Arc::new(CatalogStore::new(&dir, None).unwrap());
        let st = state(Some(store));

        let resp = get_asset_packs(State(st.clone())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()[header::CACHE_CONTROL],
            "public, max-age=3600"
        );
        let v = body_json(resp).await;
        assert_eq!(v["ok"], json!(true));
        assert_eq!(
            v["data"][0]["assets"][0]["contents"]["cube.glb"],
            json!(hash)
        );

        let resp = get_content(State(st.clone()), Path(hash.clone())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()[header::CONTENT_TYPE], "model/gltf-binary");
        assert_eq!(resp.headers()[header::ETAG], format!("\"{hash}\""));
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(bytes.as_ref(), model.as_slice());

        let resp = get_content(State(st.clone()), Path("not-a-cid".into())).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let missing = catalyrst_hashing::hash_bytes_v1(b"absent");
        let resp = get_content(State(st), Path(missing)).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    fn pulling(dir: &std::path::Path, bucket: String) -> Arc<CatalogStore> {
        Arc::new(
            CatalogStore::new(
                dir,
                Some(PullThrough {
                    bucket,
                    budget: 1 << 20,
                    http: reqwest::Client::new(),
                }),
            )
            .unwrap(),
        )
    }

    async fn failing_bucket(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route(
            "/contents/{hash}",
            axum::routing::get(move || async move { (status, "upstream down") }),
        );
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn upstream_5xx_is_a_502_with_retry_after_not_a_404() {
        let dir = temp_dir();
        let store = pulling(&dir, failing_bucket(StatusCode::BAD_GATEWAY).await);
        let hash = catalyrst_hashing::hash_bytes_v1(b"not local");
        let resp = get_content(State(state(Some(store))), Path(hash.clone())).await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(resp.headers()[header::RETRY_AFTER], RETRY_AFTER_SECS);
        let v = body_json(resp).await;
        assert!(v["message"].as_str().unwrap().contains("answered 502"));
    }

    #[tokio::test]
    async fn unreachable_bucket_is_a_503_with_retry_after() {
        let dir = temp_dir();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let store = pulling(&dir, dead);
        let hash = catalyrst_hashing::hash_bytes_v1(b"not local");
        let resp = get_content(State(state(Some(store))), Path(hash)).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(resp.headers()[header::RETRY_AFTER], RETRY_AFTER_SECS);
    }

    #[tokio::test]
    async fn upstream_404_is_a_404() {
        let dir = temp_dir();
        let store = pulling(&dir, failing_bucket(StatusCode::NOT_FOUND).await);
        let hash = catalyrst_hashing::hash_bytes_v1(b"nowhere");
        let resp = get_content(State(state(Some(store))), Path(hash)).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert!(resp.headers().get(header::RETRY_AFTER).is_none());
    }
}
