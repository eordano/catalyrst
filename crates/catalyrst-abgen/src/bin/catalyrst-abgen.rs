#![cfg_attr(target_arch = "wasm32", no_main)]
#![cfg(not(target_arch = "wasm32"))]

use std::net::SocketAddr;

use anyhow::Result;
use tower_http::trace::TraceLayer;

use abgen::abcdn::config::Config;
use abgen::abcdn::{build_app, build_registry_state, build_state};

const BIN_NAME: &str = "catalyrst-abgen";

const USAGE: &str = "\
catalyrst-abgen: ab-cdn-compatible asset-bundle JIT server (configured by env, no flags)

USAGE:
  catalyrst-abgen                 boot the server
  catalyrst-abgen --help | -h     print this help (does not boot or bind)
  catalyrst-abgen --version | -V  print the version

ENV:
  HTTP_SERVER_HOST          bind host (default 127.0.0.1)
  HTTP_SERVER_PORT          bind port (default 5147)
  ABGEN_OUT_ROOT            bundle corpus/output root (default ./data/ab-generator/out)
  ABGEN_CATALYST_URL        upstream catalyst content URL (default http://127.0.0.1:5141/content)
  ABGEN_CACHE_DIR           in-process JIT cache dir (default ./abgen-serve-cache)
  ABGEN_ROOT                dir containing template/ + shader assets (default: crate dir)
  ABGEN_VERSION             served bundle version prefix (default v41)
  ABGEN_WORLDS_CONTENT_URL  worlds-content-server fallback for by-hash content misses
                            and the /entities/active?world_name= lane
                            (default https://worlds-content-server.decentraland.org; 0/off/empty disables)
  ABGEN_SHADER_JIT          serve-time materialization of the vendored shared shader
                            bundle on shader-path misses (default on; 0/false/no/off disables)
  ABGEN_HASH_RESOLVE_FAIL_TTL_S  negative-cache TTL for unresolvable flat {hash}_{platform}
                            requests (default 3600)
  ABGEN_LOG_FORMAT          json for JSON logs (default plain text)
  RUST_LOG                  tracing filter (default abgen=info,catalyrst_abgen=info,catalyrst_registry=info,tower_http=info)
";

fn handle_argv() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {}
        Some("--help") | Some("-h") => abgen::clihelp::print_help(USAGE),
        Some("--version") | Some("-V") => abgen::clihelp::print_version(BIN_NAME),
        Some(other) => {
            eprintln!("catalyrst-abgen: unrecognized argument: {other}");
            abgen::clihelp::usage_error(USAGE);
        }
    }
}

fn env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "abgen=info,catalyrst_abgen=info,catalyrst_registry=info,tower_http=info".into()
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    handle_argv();
    abgen::maybe_enable_gpu_from_env();
    catalyrst_server::metrics::init();
    let json_logs = std::env::var("ABGEN_LOG_FORMAT")
        .map(|v| v.trim().eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter())
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter())
            .with_target(false)
            .init();
    }

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let registry = build_registry_state().await;

    let app = build_app(state, registry).layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    tracing::info!(%addr, out_root = %cfg.abgen_out_root, "catalyrst-abgen listening");
    axum::serve(listener, app).await?;
    Ok(())
}
