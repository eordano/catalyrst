use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const UNKNOWN: u8 = 0;
const PRESENT: u8 = 1;
const ABSENT: u8 = 2;
const FAILED: u8 = 3;
const RETRY_AFTER_SECS: u64 = 60;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
static RETRY_AT: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The latched answer; `None` means the probe runs (never probed, or a failure aged past its retry).
fn latched(state: u8, retry_at: u64, now: u64) -> Option<bool> {
    match state {
        PRESENT => Some(true),
        ABSENT => Some(false),
        FAILED if now < retry_at => Some(false),
        _ => None,
    }
}

pub(crate) async fn usage_grants_present(pool: &PgPool) -> bool {
    if let Some(present) = latched(
        STATE.load(Ordering::Relaxed),
        RETRY_AT.load(Ordering::Relaxed),
        now_secs(),
    ) {
        return present;
    }

    let probe: Result<bool, sqlx::Error> = sqlx::query_scalar!(
        r#"SELECT to_regclass('marketplace.usage_grants') IS NOT NULL
         AND has_table_privilege(current_user, 'marketplace.usage_grants', 'SELECT') AS "present!""#
    )
    .fetch_one(pool)
    .await;

    match probe {
        Ok(present) => {
            STATE.store(if present { PRESENT } else { ABSENT }, Ordering::Relaxed);
            present
        }
        Err(_) => {
            RETRY_AT.store(now_secs() + RETRY_AFTER_SECS, Ordering::Relaxed);
            STATE.store(FAILED, Ordering::Relaxed);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_probe_is_absent_until_its_retry_time() {
        assert_eq!(latched(UNKNOWN, 0, 100), None);
        assert_eq!(latched(PRESENT, 0, 100), Some(true));
        assert_eq!(latched(ABSENT, 0, 100), Some(false));
        assert_eq!(latched(FAILED, 160, 100), Some(false));
        assert_eq!(latched(FAILED, 160, 159), Some(false));
        assert_eq!(
            latched(FAILED, 160, 160),
            None,
            "retry once the timer lapses"
        );
    }
}
