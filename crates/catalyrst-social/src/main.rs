use anyhow::Result;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

const DEFAULT_PORT: u16 = 5145;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "catalyrst_social=info,catalyrst_communities=info,catalyrst_comms=info,\
                 catalyrst_notifications=info,catalyrst_badges=info,catalyrst_media=info,\
                 tower_http=info"
                    .into()
            }),
        )
        .with_target(false)
        .init();

    let mut members: Vec<(&'static str, bool)> = Vec::new();
    let mut app = Router::new();

    app = mount(app, &mut members, "communities", build_communities().await);
    app = mount(app, &mut members, "comms", build_comms().await);
    app = mount(
        app,
        &mut members,
        "notifications",
        build_notifications().await,
    );
    app = mount(app, &mut members, "badges", build_badges().await);
    app = mount(app, &mut members, "media", build_media().await);

    let health_body = health_body(&members);
    let app: Router = app
        .route("/status", get(catalyrst_comms::handlers::status::status))
        .route(
            "/health",
            get(move || {
                let body = health_body.clone();
                async move { ([(CONTENT_TYPE, "application/json")], body).into_response() }
            }),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let port: u16 = match env::var("BUNDLE_HTTP_PORT") {
        Ok(s) => s
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid BUNDLE_HTTP_PORT: {s:?}"))?,
        Err(_) => DEFAULT_PORT,
    };
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    tracing::info!(%addr, "catalyrst-social bundle listening");

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

async fn build_communities() -> Result<Router> {
    let cfg = catalyrst_communities::config::Config::from_env()?;
    let state = catalyrst_communities::build_state(&cfg).await?;
    Ok(catalyrst_communities::api_router().with_state(state))
}

async fn build_comms() -> Result<Router> {
    let cfg = catalyrst_comms::config::Config::from_env()?;
    let state = catalyrst_comms::build_state(&cfg).await?;
    Ok(catalyrst_comms::api_router(state.clone()).with_state(state))
}

async fn build_notifications() -> Result<Router> {
    let cfg = catalyrst_notifications::config::Config::from_env()?;
    let state = catalyrst_notifications::build_state(&cfg).await?;
    Ok(catalyrst_notifications::api_router().with_state(state))
}

async fn build_badges() -> Result<Router> {
    let cfg = catalyrst_badges::config::Config::from_env()?;
    let state = catalyrst_badges::build_state(&cfg).await?;
    Ok(catalyrst_badges::api_router().with_state(state))
}

async fn build_media() -> Result<Router> {
    let cfg = catalyrst_media::config::Config::from_env()?;
    let state = catalyrst_media::build_state(&cfg).await?;
    Ok(catalyrst_media::api_router().with_state(state))
}
