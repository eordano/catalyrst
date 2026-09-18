use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::Notify;

#[derive(Default)]
pub(super) struct RecoveryOwners(Mutex<HashMap<String, HashMap<String, Weak<Attempt>>>>);

struct Attempt {
    session: String,
    cancelled: AtomicBool,
    changed: Notify,
}

impl Attempt {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.changed.notify_waiters();
    }
}

impl RecoveryOwners {
    pub(super) fn begin(self: &Arc<Self>, wallet: &str, session: &str) -> Option<RecoveryTicket> {
        let mut owners = self.0.lock().unwrap();
        let sessions = owners.entry(wallet.into()).or_default();
        if sessions
            .get(session)
            .and_then(Weak::upgrade)
            .is_some_and(|attempt| !attempt.cancelled.load(Ordering::SeqCst))
        {
            return None;
        }
        let attempt = Arc::new(Attempt {
            session: session.into(),
            cancelled: AtomicBool::new(false),
            changed: Notify::new(),
        });
        // An announcement identifies a socket, not Pulse's current owner. Another
        // device must not cancel a valid mint merely by reannouncing itself.
        sessions.insert(session.into(), Arc::downgrade(&attempt));
        Some(RecoveryTicket {
            wallet: wallet.into(),
            attempt,
            owners: self.clone(),
        })
    }

    pub(super) fn invalidate(&self, wallet: &str) {
        if let Some(sessions) = self.0.lock().unwrap().remove(wallet) {
            for attempt in sessions.into_values().filter_map(|a| a.upgrade()) {
                attempt.cancel();
            }
        }
    }

    pub(super) fn clear(&self) {
        for (_, sessions) in self.0.lock().unwrap().drain() {
            for attempt in sessions.into_values().filter_map(|a| a.upgrade()) {
                attempt.cancel();
            }
        }
    }
}

pub(super) struct RecoveryTicket {
    wallet: String,
    attempt: Arc<Attempt>,
    owners: Arc<RecoveryOwners>,
}

impl RecoveryTicket {
    pub(super) async fn cancelled(&self) {
        loop {
            let changed = self.attempt.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.attempt.cancelled.load(Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }
}

impl Drop for RecoveryTicket {
    fn drop(&mut self) {
        let mut owners = self.owners.0.lock().unwrap();
        let Some(sessions) = owners.get_mut(&self.wallet) else {
            return;
        };
        if sessions
            .get(&self.attempt.session)
            .is_some_and(|owner| owner.ptr_eq(&Arc::downgrade(&self.attempt)))
        {
            sessions.remove(&self.attempt.session);
        }
        if sessions.is_empty() {
            owners.remove(&self.wallet);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_sessions_do_not_cancel_each_other_and_each_coalesces() {
        let owners = Arc::new(RecoveryOwners::default());
        let first = owners.begin("wallet", "first").unwrap();
        let second = owners.begin("wallet", "second").unwrap();
        assert!(!first.attempt.cancelled.load(Ordering::SeqCst));
        assert!(!second.attempt.cancelled.load(Ordering::SeqCst));
        assert!(owners.begin("wallet", "first").is_none());
        assert!(owners.begin("wallet", "second").is_none());
        drop(first);
        assert_eq!(owners.0.lock().unwrap()["wallet"].len(), 1);
        drop(second);
        assert!(owners.0.lock().unwrap().is_empty());
    }

    #[test]
    fn invalidation_cancels_every_session_of_only_the_named_wallet() {
        let owners = Arc::new(RecoveryOwners::default());
        let first = owners.begin("wallet", "first").unwrap();
        let second = owners.begin("wallet", "second").unwrap();
        let other = owners.begin("other", "first").unwrap();
        owners.invalidate("wallet");
        assert!(first.attempt.cancelled.load(Ordering::SeqCst));
        assert!(second.attempt.cancelled.load(Ordering::SeqCst));
        assert!(!other.attempt.cancelled.load(Ordering::SeqCst));
        assert_eq!(owners.0.lock().unwrap().len(), 1);
        owners.clear();
        assert!(other.attempt.cancelled.load(Ordering::SeqCst));
        assert!(owners.0.lock().unwrap().is_empty());
    }

    #[test]
    fn stale_ticket_drop_cannot_release_the_replacement_attempt() {
        let owners = Arc::new(RecoveryOwners::default());
        let first = owners.begin("wallet", "session").unwrap();
        owners.invalidate("wallet");
        let second = owners.begin("wallet", "session").unwrap();
        drop(first);
        assert!(owners.begin("wallet", "session").is_none());
        drop(second);
        assert!(owners.0.lock().unwrap().is_empty());
    }
}
