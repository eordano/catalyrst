use std::time::Duration;

use sqlx::PgPool;
use tracing::{info, warn};

use catalyrst_validator::tp_subgraph::TpSubgraph;

pub fn spawn(
    squid_pool: PgPool,
    tp: TpSubgraph,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match refresh(&squid_pool, &tp).await {
                Ok(n) => info!(
                    count = n,
                    "third-party roots refreshed from registry subgraph"
                ),
                Err(e) => warn!(error = %e, "third-party root refresh failed (will retry)"),
            }
            tokio::time::sleep(interval).await;
        }
    })
}

async fn refresh(pool: &PgPool, tp: &TpSubgraph) -> Result<usize, String> {
    let third_parties = tp
        .fetch_all_third_parties()
        .await
        .ok_or("registry subgraph fetch failed")?;
    if third_parties.is_empty() {
        return Ok(0);
    }

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS squid_marketplace.third_party (
            "id" character varying NOT NULL,
            "root" text,
            "is_approved" boolean NOT NULL,
            CONSTRAINT "PK_third_party" PRIMARY KEY ("id")
        )"#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("create third_party table: {e}"))?;

    let ids: Vec<String> = third_parties.iter().map(|(id, _, _)| id.clone()).collect();
    let roots: Vec<Option<String>> = third_parties.iter().map(|(_, r, _)| r.clone()).collect();
    let approved: Vec<bool> = third_parties.iter().map(|(_, _, a)| *a).collect();

    sqlx::query(
        r#"
        INSERT INTO squid_marketplace.third_party (id, root, is_approved)
        SELECT * FROM unnest($1::text[], $2::text[], $3::bool[])
        ON CONFLICT (id) DO UPDATE
            SET root = EXCLUDED.root, is_approved = EXCLUDED.is_approved
        "#,
    )
    .bind(&ids)
    .bind(&roots)
    .bind(&approved)
    .execute(pool)
    .await
    .map_err(|e| format!("upsert third_party: {e}"))?;

    Ok(third_parties.len())
}
