use sqlx::PgPool;
use std::sync::atomic::{AtomicU8, Ordering};

// Tri-state cache for the `marketplace.usage_grants` overlay probe:
//   0 = unknown (not yet probed), 1 = present, 2 = absent.
//
// Caching the *negative* result (ABSENT), not just the positive one, means
// overlay-less nodes stop re-running the `to_regclass`/`has_table_privilege`
// probe on every profile. Behavioral caveat: a node that GAINS the
// `usage_grants` table after startup will not pick it up until the process is
// restarted. That is acceptable — the overlay table is provisioned before boot.
//
// A transient DB error during the probe is deliberately NOT cached (state stays
// UNKNOWN) so the next call retries rather than permanently disabling the overlay.
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
        // Transient failure: leave state UNKNOWN and retry on the next call.
        Err(_) => false,
    }
}
