use std::process::ExitCode;

use catalyrst_land_authz::{cursor, fold, migrate, Indexer, DEFAULT_RPC_URL};

const ENV_DOCS: &[(&str, &str)] = &[
    (
        "LAND_AUTHZ_PG_CONNECTION_STRING",
        "required \u{2014} land-authz Postgres connection string",
    ),
    (
        "LAND_AUTHZ_RPC_URL",
        "Ethereum RPC endpoint (default https://rpc.decentraland.org/mainnet)",
    ),
    (
        "LAND_AUTHZ_STOP_BLOCK",
        "optional \u{2014} stop indexing at this block (default: chain head)",
    ),
    ("RUST_LOG", "tracing filter (default info)"),
];

#[tokio::main]
async fn main() -> ExitCode {
    catalyrst_envcfg::handle_standard_args("catalyrst-land-authz-index", ENV_DOCS);

    catalyrst_envcfg::init_tracing("info");

    let database_url = match std::env::var("LAND_AUTHZ_PG_CONNECTION_STRING") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("LAND_AUTHZ_PG_CONNECTION_STRING is required");
            return ExitCode::FAILURE;
        }
    };
    let rpc_url =
        std::env::var("LAND_AUTHZ_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());

    let pool = match sqlx::PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot connect: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = migrate(&pool).await {
        eprintln!("migration failed: {e}");
        return ExitCode::FAILURE;
    }

    let indexer = Indexer::new(rpc_url);
    let head = match indexer.head_block().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("cannot read chain head: {e}");
            return ExitCode::FAILURE;
        }
    };
    let from = match cursor(&pool).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read cursor: {e}");
            return ExitCode::FAILURE;
        }
    };
    let stop_at: u64 = std::env::var("LAND_AUTHZ_STOP_BLOCK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(head);
    let to = stop_at.min(head);

    if from <= to {
        match indexer.sync(&pool, from, to).await {
            Ok(n) => tracing::info!(events = n, from, to, "sync complete"),
            Err(e) => {
                eprintln!("sync failed: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    match fold(&pool).await {
        Ok((tokens, accounts)) => {
            tracing::info!(tokens, accounts, "fold complete");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fold failed: {e}");
            ExitCode::FAILURE
        }
    }
}
