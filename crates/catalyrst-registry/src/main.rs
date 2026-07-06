use anyhow::Result;

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5143)"),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "catalyst content DB connection string (overrides the POSTGRES_* parts below)",
    ),
    (
        "POSTGRES_HOST",
        "content DB host or socket dir (default ./data/run)",
    ),
    ("POSTGRES_PORT", "content DB port (default 6432)"),
    (
        "POSTGRES_CONTENT_USER",
        "required unless CONTENT_PG_CONNECTION_STRING is set — content DB user",
    ),
    (
        "POSTGRES_CONTENT_PASSWORD",
        "content DB password (default empty)",
    ),
    ("POSTGRES_CONTENT_DB", "content DB name (default content)"),
    (
        "AB_REGISTRY_PG_CONNECTION_STRING",
        "optional — asset-bundle registry Postgres connection string",
    ),
    (
        "ABGEN_OUT_ROOT",
        "asset-bundle output root (default ./data/ab-generator/workdir/out)",
    ),
    (
        "API_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "PROFILE_CDN_BASE_URL",
        "profile-images base URL (falls back to PROFILE_IMAGES_URL, then https://profile-images.decentraland.org)",
    ),
    (
        "PROFILE_IMAGES_URL",
        "legacy alias for PROFILE_CDN_BASE_URL",
    ),
    (
        "DENYLIST_MODERATORS",
        "comma-separated addresses allowed to manage the denylist (default empty)",
    ),
    (
        "AB_REGISTRY_REQUIRED_PLATFORMS",
        "comma-separated platforms required for complete status (default windows,mac)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_registry=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-registry", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_registry=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = catalyrst_registry::config::Config::from_env()?;
    let addr: std::net::SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let state = catalyrst_registry::build_state(&cfg).await?;
    let app = catalyrst_registry::api_router().with_state(state);

    tracing::info!(%addr, "catalyrst-registry listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
