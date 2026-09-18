use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const UNKNOWN: u8 = 0;
const PRESENT: u8 = 1;
const ABSENT: u8 = 2;
const FAILED: u8 = 3;
const RETRY_AFTER_SECS: u64 = 60;

/// A yes/no fact about the database, probed once per process; a failed probe answers `false`
/// and runs again after `RETRY_AFTER_SECS`.
pub(crate) struct LatchedProbe {
    state: AtomicU8,
    retry_at: AtomicU64,
}

impl LatchedProbe {
    pub(crate) const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNKNOWN),
            retry_at: AtomicU64::new(0),
        }
    }

    pub(crate) fn get(&self) -> Option<bool> {
        latched(
            self.state.load(Ordering::Relaxed),
            self.retry_at.load(Ordering::Relaxed),
            now_secs(),
        )
    }

    /// A failure never overwrites an answer a concurrent probe already settled.
    pub(crate) fn settle(&self, probe: Result<bool, sqlx::Error>) -> bool {
        match probe {
            Ok(yes) => {
                self.state
                    .store(if yes { PRESENT } else { ABSENT }, Ordering::Relaxed);
                yes
            }
            Err(_) => match self.state.load(Ordering::Relaxed) {
                PRESENT => true,
                ABSENT => false,
                _ => {
                    self.retry_at
                        .store(now_secs() + RETRY_AFTER_SECS, Ordering::Relaxed);
                    self.state.store(FAILED, Ordering::Relaxed);
                    false
                }
            },
        }
    }
}

static USAGE_GRANTS: LatchedProbe = LatchedProbe::new();

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
    if let Some(present) = USAGE_GRANTS.get() {
        return present;
    }

    let probe: Result<bool, sqlx::Error> = sqlx::query_scalar!(
        r#"SELECT to_regclass('marketplace.usage_grants') IS NOT NULL
         AND has_table_privilege(current_user, 'marketplace.usage_grants', 'SELECT') AS "present!""#
    )
    .fetch_one(pool)
    .await;

    USAGE_GRANTS.settle(probe)
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

    #[test]
    fn a_failed_probe_never_unsettles_an_answer() {
        let probe = LatchedProbe::new();
        assert_eq!(probe.get(), None);
        assert!(!probe.settle(Err(sqlx::Error::RowNotFound)));
        assert_eq!(probe.get(), Some(false), "failed, inside its retry window");
        assert!(probe.settle(Ok(true)));
        assert!(probe.settle(Err(sqlx::Error::RowNotFound)));
        assert_eq!(probe.get(), Some(true));
    }
}
