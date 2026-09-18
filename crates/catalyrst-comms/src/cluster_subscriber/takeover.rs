use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::Notify;

#[derive(Default)]
pub(super) struct TakeoverOwners(Mutex<HashMap<String, HashMap<String, Weak<Attempt>>>>);

struct Attempt {
    cluster: String,
    session: String,
    realm: String,
    predecessor: Mutex<Option<Weak<Attempt>>>,
    superseded_by: Mutex<Option<TakeoverTarget>>,
    finished: AtomicBool,
    changed: Notify,
}

impl Attempt {
    fn target(&self) -> TakeoverTarget {
        TakeoverTarget {
            cluster: self.cluster.clone(),
            session: self.session.clone(),
            realm: self.realm.clone(),
        }
    }

    fn supersede(&self, target: TakeoverTarget) {
        *self.superseded_by.lock().unwrap() = Some(target.clone());
        self.changed.notify_waiters();
        if let Some(predecessor) = self
            .predecessor
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
        {
            predecessor.supersede(target);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TakeoverTarget {
    pub(super) cluster: String,
    pub(super) session: String,
    pub(super) realm: String,
}

impl TakeoverOwners {
    pub(super) fn prepare(
        self: &Arc<Self>,
        wallet: &str,
        cluster: &str,
        session: &str,
        realm: &str,
    ) -> (TakeoverRegistration, TakeoverTicket) {
        let attempt = Arc::new(Attempt {
            cluster: cluster.into(),
            session: session.into(),
            realm: realm.into(),
            predecessor: Mutex::new(None),
            superseded_by: Mutex::new(None),
            finished: AtomicBool::new(false),
            changed: Notify::new(),
        });
        let registration = TakeoverRegistration {
            wallet: wallet.into(),
            cluster: cluster.into(),
            attempt: attempt.clone(),
            owners: self.clone(),
        };
        let ticket = TakeoverTicket {
            wallet: wallet.into(),
            cluster: cluster.into(),
            attempt,
            owners: self.clone(),
        };
        (registration, ticket)
    }
}

pub(super) struct TakeoverRegistration {
    wallet: String,
    cluster: String,
    attempt: Arc<Attempt>,
    owners: Arc<TakeoverOwners>,
}

impl TakeoverRegistration {
    /// Makes an accepted same-room takeover current and wakes its predecessor. A rejected queue
    /// item never calls this, so it cannot cancel work that still has a chance to complete.
    pub(super) fn commit(self) {
        let previous = {
            let mut owners = self.owners.0.lock().unwrap();
            if self.attempt.finished.load(Ordering::SeqCst) {
                return;
            }
            owners
                .entry(self.wallet)
                .or_default()
                .insert(self.cluster, Arc::downgrade(&self.attempt))
                .and_then(|attempt| attempt.upgrade())
        };
        if let Some(previous) = previous {
            *self.attempt.predecessor.lock().unwrap() = Some(Arc::downgrade(&previous));
            previous.supersede(self.attempt.target());
        }
    }
}

pub(super) struct TakeoverTicket {
    wallet: String,
    cluster: String,
    attempt: Arc<Attempt>,
    owners: Arc<TakeoverOwners>,
}

impl TakeoverTicket {
    pub(super) async fn superseded(&self) -> TakeoverTarget {
        loop {
            let changed = self.attempt.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(target) = self.attempt.superseded_by.lock().unwrap().take() {
                return target;
            }
            changed.await;
        }
    }
}

impl Drop for TakeoverTicket {
    fn drop(&mut self) {
        self.attempt.finished.store(true, Ordering::SeqCst);
        let mut owners = self.owners.0.lock().unwrap();
        let Some(clusters) = owners.get_mut(&self.wallet) else {
            return;
        };
        if clusters
            .get(&self.cluster)
            .is_some_and(|owner| owner.ptr_eq(&Arc::downgrade(&self.attempt)))
        {
            clusters.remove(&self.cluster);
        }
        if clusters.is_empty() {
            owners.remove(&self.wallet);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_committed_same_room_successor_cancels_its_predecessor() {
        let owners = Arc::new(TakeoverOwners::default());
        let (first_registration, first) = owners.prepare("wallet", "room", "first", "realm");
        first_registration.commit();
        let (rejected_registration, rejected) =
            owners.prepare("wallet", "room", "rejected", "realm");
        drop(rejected_registration);
        drop(rejected);
        assert!(first.attempt.superseded_by.lock().unwrap().is_none());

        let (second_registration, second) = owners.prepare("wallet", "room", "second", "realm");
        second_registration.commit();
        assert_eq!(
            first.attempt.superseded_by.lock().unwrap().as_ref(),
            Some(&TakeoverTarget {
                cluster: "room".into(),
                session: "second".into(),
                realm: "realm".into(),
            })
        );
        assert!(second.attempt.superseded_by.lock().unwrap().is_none());
    }

    #[test]
    fn another_room_or_wallet_does_not_cancel_the_active_takeover() {
        let owners = Arc::new(TakeoverOwners::default());
        let (first_registration, first) = owners.prepare("wallet", "room", "first", "realm");
        first_registration.commit();
        for (wallet, room) in [("wallet", "other-room"), ("other-wallet", "room")] {
            let (registration, _ticket) = owners.prepare(wallet, room, "next", "realm");
            registration.commit();
        }
        assert!(first.attempt.superseded_by.lock().unwrap().is_none());
    }

    #[test]
    fn a_finished_ticket_cannot_leave_a_dead_owner_or_release_its_successor() {
        let owners = Arc::new(TakeoverOwners::default());
        let (late_registration, late) = owners.prepare("wallet", "room", "late", "realm");
        drop(late);
        late_registration.commit();
        assert!(owners.0.lock().unwrap().is_empty());

        let (first_registration, first) = owners.prepare("wallet", "room", "first", "realm");
        first_registration.commit();
        let (second_registration, second) = owners.prepare("wallet", "room", "second", "realm");
        second_registration.commit();
        drop(first);
        assert!(owners.0.lock().unwrap()["wallet"].contains_key("room"));
        drop(second);
        assert!(owners.0.lock().unwrap().is_empty());
    }

    #[test]
    fn the_latest_successor_propagates_through_a_queued_chain() {
        let owners = Arc::new(TakeoverOwners::default());
        let (first_registration, first) = owners.prepare("wallet", "room", "first", "realm");
        first_registration.commit();
        let (second_registration, _second) = owners.prepare("wallet", "room", "second", "realm");
        second_registration.commit();
        let (third_registration, _third) = owners.prepare("wallet", "room", "third", "realm");
        third_registration.commit();
        assert_eq!(
            first.attempt.superseded_by.lock().unwrap().as_ref(),
            Some(&TakeoverTarget {
                cluster: "room".into(),
                session: "third".into(),
                realm: "realm".into(),
            })
        );
    }
}
