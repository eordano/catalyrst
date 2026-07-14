use std::net::SocketAddr;

use anyhow::Result;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use tower::Layer;
use tower_http::cors::CorsLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "catalyrst_explore=info,catalyrst_places=info,catalyrst_events=info,\
                 catalyrst_archipelago=info,catalyrst_worlds=info,catalyrst_map=info,\
                 tower_http=info"
                    .into()
            }),
        )
        .with_target(false)
        .init();

    let mut members: Vec<(&'static str, bool)> = Vec::new();
    let mut app = Router::new();

    let (places, lists) = build_places().await;
    app = mount(app, &mut members, "places", places);
    app = mount(app, &mut members, "events", build_events().await);
    app = mount(app, &mut members, "archipelago", build_archipelago().await);
    app = mount(app, &mut members, "worlds", build_worlds().await);
    app = mount(app, &mut members, "map", build_map().await);
    app = mount(app, &mut members, "lists", lists);

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
        .unwrap_or(5143);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!(%addr, "catalyrst-explore bundle listening");
    let app = NormalizePathLayer::trim_trailing_slash().layer(app);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        axum::ServiceExt::<axum::extract::Request>::into_make_service(app),
    )
    .await?;
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

async fn build_places() -> (Result<Router>, Result<Router>) {
    match build_places_state().await {
        Ok(state) => (
            Ok(catalyrst_places::api_router().with_state(state.clone())),
            Ok(catalyrst_places::lists_router().with_state(state)),
        ),
        Err(err) => {
            let msg = format!("{err:#}");
            (Err(anyhow::anyhow!(msg.clone())), Err(anyhow::anyhow!(msg)))
        }
    }
}

async fn build_places_state() -> Result<catalyrst_places::AppState> {
    let cfg = catalyrst_places::config::Config::from_env()?;
    catalyrst_places::build_state(&cfg).await
}

async fn build_events() -> Result<Router> {
    let cfg = catalyrst_events::config::Config::from_env()?;
    let state = catalyrst_events::build_state(&cfg).await?;
    Ok(catalyrst_events::api_router().with_state(state))
}

async fn build_archipelago() -> Result<Router> {
    let cfg = catalyrst_archipelago::Config::from_env()?;
    let state = catalyrst_archipelago::build_state(&cfg).await?;
    Ok(catalyrst_archipelago::api_router().with_state(state))
}

async fn build_worlds() -> Result<Router> {
    let cfg = catalyrst_worlds::config::Config::from_env()?;
    let state = catalyrst_worlds::build_state(cfg).await?;
    Ok(catalyrst_worlds::api_router().with_state(state))
}

async fn build_map() -> Result<Router> {
    let cfg = catalyrst_map::config::Config::from_env()?;
    let state = catalyrst_map::build_state(&cfg).await?;
    Ok(catalyrst_map::api_router().with_state(state))
}
