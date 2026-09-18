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
                "catalyrst_social=info,catalyrst_social_service=info,catalyrst_comms=info,\
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
    let mut comms_runtime = None;
    match build_comms().await {
        Ok((router, runtime)) => {
            members.push(("comms", true));
            app = app.merge(router);
            comms_runtime = Some(runtime);
        }
        Err(err) => {
            if protected_comms_requested() {
                return Err(err.context("protected embedded comms startup failed"));
            }
            tracing::warn!(member = "comms", %err, "member unavailable, serving without it");
            members.push(("comms", false));
        }
    }
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
    if let Some(runtime) = comms_runtime.as_mut() {
        runtime.start();
    }
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    if let Some(runtime) = comms_runtime {
        runtime.shutdown().await;
    }
    served?;
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
    let cfg = catalyrst_social_service::rest::config::Config::from_env()?;
    let state = catalyrst_social_service::rest::build_state(&cfg).await?;
    Ok(catalyrst_social_service::rest::api_router().with_state(state))
}

async fn build_comms() -> Result<(Router, catalyrst_comms::CommsRuntime)> {
    let cfg = catalyrst_comms::config::Config::from_env()?;
    let state = catalyrst_comms::build_state(&cfg).await?;
    let runtime = catalyrst_comms::CommsRuntime::embedded(state.clone(), &cfg.cluster)?;
    let connections = catalyrst_comms::connection_router(
        state.clone(),
        runtime.assignment_reader(),
        cfg.pulse_room_authority_key,
    )?;
    let routes = catalyrst_comms::api_router(state.clone())
        .merge(connections)
        .with_state(state);
    Ok((routes, runtime))
}

fn protected_comms_requested() -> bool {
    env::var_os("COMMS_CONTROL_PG_CONNECTION_STRING").is_some()
        || env::var_os("COMMS_CONTROL_V4_AUDIENCE").is_some()
}

async fn shutdown_signal() {
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()).ok() {
            Some(mut signal) => {
                signal.recv().await;
            }
            None => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate => {}
    }
}

async fn build_notifications() -> Result<Router> {
    let cfg = catalyrst_notifications::config::Config::from_env()?;
    let state = catalyrst_notifications::build_state(&cfg).await?;
    Ok(catalyrst_notifications::api_router().with_state(state))
}

async fn build_badges() -> Result<Router> {
    let cfg = catalyrst_badges::config::Config::from_env()?;
    let state = catalyrst_badges::build_state(&cfg).await?;
    Ok(catalyrst_badges::api_router(&cfg).with_state(state))
}

async fn build_media() -> Result<Router> {
    let cfg = catalyrst_media::config::Config::from_env()?;
    let state = catalyrst_media::build_state(&cfg).await?;
    Ok(catalyrst_media::api_router().with_state(state))
}
