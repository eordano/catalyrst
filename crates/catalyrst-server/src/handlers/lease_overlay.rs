use sqlx::PgPool;
use std::sync::atomic::{AtomicU8, Ordering};

const UNKNOWN: u8 = 0;
const PRESENT: u8 = 1;
const ABSENT: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

pub(crate) async fn usage_grants_present(pool: &PgPool) -> bool {
    match STATE.load(Ordering::Relaxed) {
        PRESENT => return true,
        ABSENT => return false,
        _ => {}
    }

    let probe: Result<bool, sqlx::Error> = sqlx::query_scalar(
        "SELECT to_regclass('marketplace.usage_grants') IS NOT NULL \
         AND has_table_privilege(current_user, 'marketplace.usage_grants', 'SELECT')",
    )
    .fetch_one(pool)
    .await;

    match probe {
        Ok(present) => {
            STATE.store(if present { PRESENT } else { ABSENT }, Ordering::Relaxed);
            present
        }
        Err(_) => false,
    }
}
