use sqlx::{Executor, PgPool};

use super::constants::{
    ACQUISITION_SCAN_DEADLINE_MS, NEIGHBORS_JOB_STATEMENT_TIMEOUT_MS,
    NEIGHBORS_REBUILD_INTERVAL_MS, NEIGHBORS_REBUILD_STARTUP_DELAY_MS,
};
use super::neighbours::{
    begin_swap, finish_swap, produce_neighbor_rows, BuildOptions, RebuildOutcome,
    REBUILD_SESSION_LOCK_KEY,
};

/// One rebuild.
///
/// Two connections, deliberately: the producer streams a multi-million-row scan while the swap
/// holds an open transaction around the staging table, and sharing one connection would serialise
/// the two into a deadlock the first time the stream needed a round trip.
///
/// The session-scoped advisory lock is what keeps several replicas -- all running the same
/// schedule -- from recomputing the same table at once. It is taken for the WHOLE run, not just
/// the swap: the expensive part is the scan, and two replicas scanning in parallel to have one of
/// them discard the result is the waste worth avoiding.
pub async fn rebuild_neighbors(pool: &PgPool) -> Result<RebuildOutcome, sqlx::Error> {
    let mut guard = pool.acquire().await?;
    let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(REBUILD_SESSION_LOCK_KEY)
        .fetch_one(&mut *guard)
        .await?;
    if !held {
        tracing::info!("item-neighbours rebuild already running elsewhere: skipped");
        return Ok(RebuildOutcome::Skipped);
    }

    let outcome = run_rebuild(pool).await;

    let released = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(REBUILD_SESSION_LOCK_KEY)
        .execute(&mut *guard)
        .await;
    if let Err(e) = released {
        tracing::warn!(error = %e, "item-neighbours advisory lock not released cleanly");
    }

    outcome
}

async fn run_rebuild(pool: &PgPool) -> Result<RebuildOutcome, sqlx::Error> {
    let mut read = pool.acquire().await?;
    let mut write = pool.acquire().await?;

    let outcome = run_rebuild_on(&mut read, &mut write).await;

    // BOTH connections go back to the pool, and both carry the job's per-session ceiling until it
    // is reset -- a normal API request borrowing either afterwards would silently run under five
    // minutes instead of the pool's own limit. Reset on every exit path, including the one where
    // setting the timeout is itself what failed.
    for connection in [&mut read, &mut write] {
        let _ = connection
            .execute(sqlx::AssertSqlSafe("RESET statement_timeout".to_string()))
            .await;
    }

    outcome
}

async fn run_rebuild_on(
    read: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    write: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
) -> Result<RebuildOutcome, sqlx::Error> {
    read.execute(sqlx::AssertSqlSafe(format!(
        "SET statement_timeout = {NEIGHBORS_JOB_STATEMENT_TIMEOUT_MS}"
    )))
    .await?;
    write
        .execute(sqlx::AssertSqlSafe(format!(
            "SET statement_timeout = {NEIGHBORS_JOB_STATEMENT_TIMEOUT_MS}"
        )))
        .await?;

    let started = std::time::Instant::now();
    let options = BuildOptions {
        block_width: None,
        acquisition_deadline: Some(std::time::Duration::from_millis(
            ACQUISITION_SCAN_DEADLINE_MS,
        )),
    };

    let outcome = async {
        let Some(mut tx) = begin_swap(write).await? else {
            return Ok(RebuildOutcome::Skipped);
        };
        let meta = produce_neighbor_rows(read, &mut tx, options).await?;
        finish_swap(tx, &meta).await
    }
    .await;

    match &outcome {
        Ok(RebuildOutcome::Rebuilt) => tracing::info!(
            duration_ms = started.elapsed().as_millis() as u64,
            "item-neighbours rebuilt"
        ),
        Ok(RebuildOutcome::Skipped) => {}
        Err(e) => {
            tracing::warn!(error = %e, "item-neighbours rebuild failed: previous table still serving")
        }
    }

    outcome
}

pub fn startup_delay() -> std::time::Duration {
    std::time::Duration::from_millis(NEIGHBORS_REBUILD_STARTUP_DELAY_MS)
}

pub fn rebuild_interval() -> std::time::Duration {
    std::time::Duration::from_millis(NEIGHBORS_REBUILD_INTERVAL_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A multi-minute scan starting the instant a process comes up competes with the warm-up it
    /// is supposed to follow.
    #[test]
    fn the_job_waits_before_its_first_run_and_then_runs_on_a_long_cycle() {
        assert!(startup_delay().as_secs() >= 60);
        assert!(rebuild_interval() > startup_delay());
        assert!(rebuild_interval().as_secs() >= 3600);
    }
}
