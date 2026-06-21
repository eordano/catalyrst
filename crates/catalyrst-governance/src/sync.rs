use anyhow::Result;

use crate::client::GovernanceClient;
use crate::parse;
use crate::ports::store::Store;

pub async fn backfill(client: &GovernanceClient, store: &Store) -> Result<()> {
    tracing::info!("backfill: fetching all proposals");
    let proposals = client.fetch_all_proposals().await?;
    let n = store.upsert_proposals(&proposals).await?;
    tracing::info!(proposals = n, "backfill: upserted proposals");

    client.throttle_pub().await;
    let projects = client.fetch_projects().await?;
    let np = store.upsert_projects(&projects).await?;
    tracing::info!(projects = np, "backfill: upserted projects");

    let mut total_updates = 0u64;
    for (i, proj) in projects.iter().enumerate() {
        let Some(pid) = parse::opt_str(proj, "id") else {
            continue;
        };
        client.throttle_pub().await;
        let updates = client.fetch_project_updates(&pid).await?;
        total_updates += store.upsert_project_updates(&updates).await?;
        if (i + 1) % 20 == 0 {
            tracing::info!(
                processed = i + 1,
                of = projects.len(),
                "backfill: project updates"
            );
        }
    }
    tracing::info!(
        updates = total_updates,
        "backfill: upserted project updates"
    );

    client.throttle_pub().await;
    let budgets = client.fetch_budgets().await?;
    let nb = store.upsert_budgets(&budgets).await?;

    client.throttle_pub().await;
    let vestings = client.fetch_vestings().await?;
    let nv = store.upsert_vestings(&vestings).await?;

    let (nc, nd) = sync_members(client, store).await?;

    store
        .set_sync_state("last_backfill", &chrono::Utc::now().to_rfc3339())
        .await?;

    tracing::info!(
        proposals = n,
        projects = np,
        updates = total_updates,
        budgets = nb,
        vestings = nv,
        members = nc + nd,
        "backfill complete"
    );
    Ok(())
}

pub async fn sync(client: &GovernanceClient, store: &Store, window: u32) -> Result<()> {
    tracing::info!(window, "sync: fetching recently-updated proposals");
    let proposals = client.fetch_recent_proposals(window).await?;
    let n = store.upsert_proposals(&proposals).await?;
    tracing::info!(proposals = n, "sync: upserted proposals");

    client.throttle_pub().await;
    let projects = client.fetch_projects().await?;
    let np = store.upsert_projects(&projects).await?;

    let mut total_updates = 0u64;
    let active: Vec<_> = projects
        .iter()
        .filter(|p| parse::project_is_active(p))
        .collect();
    for proj in &active {
        let Some(pid) = parse::opt_str(proj, "id") else {
            continue;
        };
        client.throttle_pub().await;
        let updates = client.fetch_project_updates(&pid).await?;
        total_updates += store.upsert_project_updates(&updates).await?;
    }
    tracing::info!(
        updates = total_updates,
        active = active.len(),
        "sync: upserted active updates"
    );

    client.throttle_pub().await;
    let budgets = client.fetch_budgets().await?;
    let nb = store.upsert_budgets(&budgets).await?;

    client.throttle_pub().await;
    let vestings = client.fetch_vestings().await?;
    let nv = store.upsert_vestings(&vestings).await?;

    let (nc, nd) = sync_members(client, store).await?;

    store
        .set_sync_state("last_sync", &chrono::Utc::now().to_rfc3339())
        .await?;

    tracing::info!(
        proposals = n,
        projects = np,
        updates = total_updates,
        budgets = nb,
        vestings = nv,
        members = nc + nd,
        "sync complete"
    );
    Ok(())
}

async fn sync_members(client: &GovernanceClient, store: &Store) -> Result<(u64, u64)> {
    client.throttle_pub().await;
    let committee = client.fetch_members("committee").await?;
    let nc = store.replace_members("committee", &committee).await?;

    client.throttle_pub().await;
    let council = client.fetch_members("dao-council").await?;
    let nd = store.replace_members("council", &council).await?;
    tracing::info!(committee = nc, council = nd, "synced members");
    Ok((nc, nd))
}
