use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};

static PRESENT: AtomicBool = AtomicBool::new(false);

pub(crate) async fn usage_grants_present(pool: &PgPool) -> bool {
    if PRESENT.load(Ordering::Relaxed) {
        return true;
    }

    let present: bool = sqlx::query_scalar(
        "SELECT to_regclass('marketplace.usage_grants') IS NOT NULL \
         AND has_table_privilege(current_user, 'marketplace.usage_grants', 'SELECT')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if present {
        PRESENT.store(true, Ordering::Relaxed);
    }
    present
}
