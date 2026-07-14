use axum::http::HeaderValue;

pub(crate) async fn spawn_content_server(
    body: Vec<u8>,
    encoding: Option<&'static str>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/contents/{hash}",
        axum::routing::get(move || async move {
            let mut resp = axum::response::Response::new(axum::body::Body::from(body));
            if let Some(enc) = encoding {
                resp.headers_mut().insert(
                    axum::http::header::CONTENT_ENCODING,
                    HeaderValue::from_static(enc),
                );
            }
            resp
        }),
    );
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), handle)
}

pub(crate) async fn temp_content_storage(
    tag: &str,
) -> (catalyrst_storage::ContentStorage, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "catalyrst-sync-test-{tag}-{}-{}",
        std::process::id(),
        rand::random::<u32>()
    ));
    let storage = catalyrst_storage::ContentStorage::new(&tmp).await.unwrap();
    (storage, tmp)
}
