use std::net::SocketAddr;

use anyhow::Result;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use catalyrst_credits as credits;
use catalyrst_economy as economy;
use catalyrst_market as market;
use catalyrst_price as price;
use catalyrst_rpc as rpc;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "catalyrst_data=info,catalyrst_market=info,catalyrst_economy=info,\
                 catalyrst_price=info,catalyrst_credits=info,catalyrst_rpc=info,tower_http=info"
                    .into()
            }),
        )
        .with_target(false)
        .init();

    let mut members: Vec<(&'static str, bool)> = Vec::new();
    let mut app = Router::new();

    app = mount(app, &mut members, "market", build_market().await);
    app = mount(app, &mut members, "economy", build_economy().await);
    app = mount(app, &mut members, "price", build_price().await);
    app = mount(app, &mut members, "credits", build_credits().await);
    app = mount(app, &mut members, "rpc", build_rpc().await);

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
        .unwrap_or(5146);
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    tracing::info!(%addr, "catalyrst-data listening");
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

async fn build_market() -> Result<Router> {
    let cfg = market::config::Config::from_env()?;
    let state = market::build_state(&cfg).await?;
    Ok(market::api_router().with_state(state))
}

async fn build_economy() -> Result<Router> {
    let cfg = economy::config::Config::from_env()?;
    let api_version = cfg.api_version.clone();
    let state = economy::build_state(cfg).await?;
    Ok(economy::api_router(&api_version).with_state(state))
}

async fn build_price() -> Result<Router> {
    let cfg = price::config::Config::from_env()?;
    let state = price::build_state(&cfg).await?;
    Ok(price::api_router().with_state(state))
}

async fn build_credits() -> Result<Router> {
    let cfg = credits::config::Config::from_env()?;
    let state = credits::build_state(&cfg).await?;
    Ok(credits::api_router().with_state(state))
}

async fn build_rpc() -> Result<Router> {
    let cfg = rpc::config::Config::from_env()?;
    let state = rpc::build_state(cfg).await?;
    Ok(rpc::api_router().with_state(state))
}
