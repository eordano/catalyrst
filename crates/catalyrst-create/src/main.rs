use std::net::SocketAddr;

use anyhow::Result;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "catalyrst_create=info,catalyrst_builder=info,catalyrst_camera_reel=info,\
                 tower_http=info"
                    .into()
            }),
        )
        .with_target(false)
        .init();

    let mut members: Vec<(&'static str, bool)> = Vec::new();
    let mut app = Router::new();

    app = mount(app, &mut members, "builder", build_builder().await);
    app = mount(app, &mut members, "camera-reel", build_camera_reel().await);

    let health_body = health_body(&members);
    let app = app
        .route(
            "/health",
            get(move || {
                let body = health_body.clone();
                async move { ([(CONTENT_TYPE, "application/json")], body).into_response() }
            }),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let port: u16 = std::env::var("BUNDLE_HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5144);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    tracing::info!(%addr, "catalyrst-create bundle listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn mount(
    app: Router,
    members: &mut Vec<(&'static str, bool)>,
    name: &'static str,
    built: Result<Router>,
) -> Router {
    match built {
        Ok(router) => {
            members.push((name, true));
            app.merge(router)
        }
        Err(err) => {
            tracing::warn!(member = name, %err, "member unavailable, serving without it");
            members.push((name, false));
            app
        }
    }
}

fn health_body(members: &[(&'static str, bool)]) -> String {
    let all_up = members.iter().all(|(_, up)| *up);
    let members_obj: serde_json::Map<String, serde_json::Value> = members
        .iter()
        .map(|(name, up)| (name.to_string(), json!(if *up { "up" } else { "down" })))
        .collect();
    json!({
        "status": if all_up { "ok" } else { "degraded" },
        "members": members_obj,
    })
    .to_string()
}

async fn build_builder() -> Result<Router> {
    let cfg = catalyrst_builder::config::Config::from_env()?;
    let state = catalyrst_builder::build_state(&cfg).await?;
    Ok(catalyrst_builder::api_router().with_state(state))
}

async fn build_camera_reel() -> Result<Router> {
    let cfg = catalyrst_camera_reel::config::Config::from_env()?;
    let state = catalyrst_camera_reel::build_state(cfg).await?;
    Ok(catalyrst_camera_reel::api_router().with_state(state))
}
