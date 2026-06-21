use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_bvimposters::config::Config;
use catalyrst_bvimposters::store::Store;
use catalyrst_bvimposters::{api_router, build_state, handlers, quarantine_list, seed};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5154)"),
    (
        "BVIMPOSTERS_STORE_ROOT",
        "store root (default /var/lib/bvimposters)",
    ),
    (
        "BVIMPOSTERS_STORE_MAX_BYTES",
        "LRU byte budget for the store (default 21474836480 = 20 GiB)",
    ),
    (
        "BVIMPOSTERS_CDN_BASE",
        "read-through upstream base (default https://bevy-imposters.dclregenesislabs.xyz)",
    ),
    (
        "BVIMPOSTERS_CDN_REALM_SEGMENT",
        "literal wire realm segment for upstream URLs, never re-encoded (default the double-encoded official realm about_url)",
    ),
    (
        "BVIMPOSTERS_READTHROUGH_TIMEOUT_SECS",
        "upstream fetch timeout (default 30)",
    ),
    (
        "BVIMPOSTERS_QUARANTINE_LIST",
        "file of store keys ({level}/{x},{y}.{crc}[.zip] per line) that skip CDN read-through (default {store_root}/readthrough-quarantine.txt, missing file = empty)",
    ),
    (
        "BVIMPOSTERS_BAKE_ENABLED",
        "enable bake-on-miss via impost (default 0)",
    ),
    (
        "BVIMPOSTERS_BAKE_WRAPPER",
        "optional command prefix providing the display env for impost (default empty)",
    ),
    (
        "BVIMPOSTERS_IMPOST_BIN",
        "impost binary (default impost)",
    ),
    (
        "BVIMPOSTERS_IMPOST_SERVER",
        "realm server passed to impost (default https://catalyst.example.com)",
    ),
    (
        "BVIMPOSTERS_IMPOST_CONTENT_SERVER",
        "content server passed to impost (default http://localhost:5141)",
    ),
    (
        "BVIMPOSTERS_BAKE_QUEUE_DEPTH",
        "bounded bake queue depth, clamped to 1..=2 (default 1)",
    ),
    (
        "BVIMPOSTERS_BAKE_TIMEOUT_SECS",
        "bake subprocess timeout, killpg on expiry (default 1800)",
    ),
    (
        "BVIMPOSTERS_BAKE_MAX_FAILURES",
        "consecutive failures before quarantine (default 3)",
    ),
    (
        "BVIMPOSTERS_BAKE_QUARANTINE_SECS",
        "quarantine duration for poison keys (default 86400)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_bvimposters=info,tower_http=info)",
    ),
];

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_bvimposters=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("seed") => {
            let Some(dir) = args.next() else {
                eprintln!("usage: catalyrst-bvimposters seed <realm-cache-dir>");
                std::process::exit(2);
            };
            init_tracing();
            let cfg = Config::from_env()?;
            let counts = seed::run(&cfg, std::path::Path::new(&dir))?;
            println!(
                "imported {} skipped {} crc0 {} incomplete {}",
                counts.imported, counts.skipped, counts.crc0, counts.incomplete
            );
            return Ok(());
        }
        Some("quarantine") => {
            init_tracing();
            let cfg = Config::from_env()?;
            let path = args
                .next()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| cfg.quarantine_list.clone());
            let store = Store::new(cfg.store_root.clone(), cfg.store_max_bytes);
            store.init()?;
            let list = quarantine_list::QuarantineList::load(path.clone());
            let counts = quarantine_list::apply(&store, &list);
            println!(
                "list {} keys {} renamed {} absent {} errors {}",
                path.display(),
                list.len(),
                counts.renamed,
                counts.absent,
                counts.errors
            );
            return Ok(());
        }
        _ => {}
    }

    catalyrst_envcfg::handle_standard_args("catalyrst-bvimposters", ENV_DOCS);
    init_tracing();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-bvimposters listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
