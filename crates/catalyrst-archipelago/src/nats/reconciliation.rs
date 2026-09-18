use crate::registry::{PeersRegistry, ReconciliationPeer};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::Instant;

const FULL_INTERVAL: Duration = Duration::from_secs(30);
const RESTORED_FOLLOWUPS: u8 = 2;

pub(super) struct Reconciliation {
    pending: VecDeque<ReconciliationPeer>,
    unassigned_only: bool,
    next_full: Instant,
    restored_followups: u8,
}

impl Reconciliation {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            pending: VecDeque::new(),
            unassigned_only: false,
            next_full: now,
            restored_followups: 0,
        }
    }

    pub(super) fn reset(&mut self, now: Instant) {
        self.pending.clear();
        self.next_full = now;
        self.restored_followups = RESTORED_FOLLOWUPS;
    }

    pub(super) fn refill(&mut self, registry: &PeersRegistry, now: Instant) {
        // Finish a sweep before taking another snapshot: replacing a slow pass
        // at each timer tick can indefinitely starve sockets near its tail.
        if !self.pending.is_empty() {
            return;
        }
        self.unassigned_only = now < self.next_full;
        if !self.unassigned_only {
            let interval = if self.restored_followups > 0 {
                self.restored_followups -= 1;
                super::RECONCILE_INTERVAL
            } else {
                FULL_INTERVAL
            };
            self.next_full = now + interval;
        }
        self.pending
            .extend(registry.reconciliation_snapshot(self.unassigned_only));
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn next(&mut self, registry: &PeersRegistry) -> Option<ReconciliationPeer> {
        while let Some(target) = self.pending.pop_front() {
            if registry.reconciliation_peer_is_current(&target)
                && (!self.unassigned_only || registry.reconciliation_peer_needs_assignment(&target))
            {
                return Some(target);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(target: Option<ReconciliationPeer>) -> Option<(String, String)> {
        target.map(|target| (target.address, target.session))
    }

    #[test]
    fn assignment_stops_fast_retries_but_not_periodic_current_state_checks() {
        let registry = PeersRegistry::new();
        let (peer, _rx, _) = registry.on_peer_connected("wallet", "session");
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        assert_eq!(
            names(sweep.next(&registry)),
            Some(("wallet".into(), "session".into()))
        );
        peer.assignment_sent();
        sweep.refill(&registry, now + Duration::from_secs(5));
        assert!(sweep.next(&registry).is_none());
        sweep.refill(&registry, now + FULL_INTERVAL - Duration::from_nanos(1));
        assert!(sweep.next(&registry).is_none());
        sweep.refill(&registry, now + FULL_INTERVAL);
        assert_eq!(
            names(sweep.next(&registry)),
            Some(("wallet".into(), "session".into()))
        );
        assert!(sweep.next(&registry).is_none());
    }

    #[test]
    fn new_sockets_retry_while_a_healthy_socket_waits_for_the_full_sweep() {
        let registry = PeersRegistry::new();
        let (assigned, _rx_a, _) = registry.on_peer_connected("assigned", "session");
        assigned.assignment_sent();
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        assert!(sweep.next(&registry).is_some());
        let (_, _rx_b, _) = registry.on_peer_connected("new", "session");
        for seconds in [5, 10, 15, 20, 25] {
            sweep.refill(&registry, now + Duration::from_secs(seconds));
            assert_eq!(
                names(sweep.next(&registry)),
                Some(("new".into(), "session".into()))
            );
            assert!(sweep.next(&registry).is_none());
        }
    }

    #[test]
    fn an_in_progress_sweep_keeps_its_tail_across_multiple_timer_ticks() {
        let registry = PeersRegistry::new();
        let mut receivers = Vec::new();
        for index in 0..1_031 {
            let (peer, rx, _) = registry.on_peer_connected(&format!("wallet-{index}"), "session");
            peer.assignment_sent();
            receivers.push(rx);
        }
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        let mut seen = std::collections::HashSet::new();
        for index in 0..1_031 {
            sweep.refill(&registry, now + Duration::from_secs(index * 5));
            let target = sweep.next(&registry).expect("the pass lost its tail");
            assert!(
                seen.insert(target.address),
                "a new pass replaced an unfinished one"
            );
        }
        assert_eq!(seen.len(), 1_031);
        assert!(sweep.is_empty());
    }

    #[test]
    fn a_restored_link_immediately_resnapshots_even_assigned_sockets() {
        let registry = PeersRegistry::new();
        let (peer, _rx, _) = registry.on_peer_connected("wallet", "session");
        peer.assignment_sent();
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        assert!(sweep.next(&registry).is_some());
        let recovered = now + Duration::from_secs(1);
        sweep.reset(recovered);
        sweep.refill(&registry, recovered);
        assert_eq!(
            names(sweep.next(&registry)),
            Some(("wallet".into(), "session".into()))
        );
    }

    #[test]
    fn restored_assigned_sockets_get_bounded_followups_before_normal_sweeps() {
        let registry = PeersRegistry::new();
        let (peer, _rx, _) = registry.on_peer_connected("wallet", "session");
        peer.assignment_sent();
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.reset(now);
        for seconds in [0, 5, 10] {
            sweep.refill(&registry, now + Duration::from_secs(seconds));
            assert_eq!(
                names(sweep.next(&registry)),
                Some(("wallet".into(), "session".into())),
                "restored broker progress does not establish consumer readiness"
            );
            assert!(sweep.next(&registry).is_none());
        }
        for seconds in [15, 20, 25, 30, 35] {
            sweep.refill(&registry, now + Duration::from_secs(seconds));
            assert!(sweep.next(&registry).is_none(), "recovery burst must end");
        }
        sweep.refill(&registry, now + Duration::from_secs(40));
        assert!(sweep.next(&registry).is_some());
    }

    #[test]
    fn recovery_followups_keep_the_pending_tail_and_revalidate_owners() {
        let registry = PeersRegistry::new();
        let mut receivers = Vec::new();
        for index in 0..3 {
            let (peer, rx, _) = registry.on_peer_connected(&format!("wallet-{index}"), "old");
            peer.assignment_sent();
            receivers.push(rx);
        }
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.reset(now);
        sweep.refill(&registry, now);
        let first = sweep.next(&registry).unwrap();
        sweep.refill(&registry, now + Duration::from_secs(5));
        let second = sweep.next(&registry).unwrap();
        assert_ne!(first.address, second.address);
        let remaining = (0..3)
            .map(|index| format!("wallet-{index}"))
            .find(|address| address != &first.address && address != &second.address)
            .unwrap();
        let retired = registry.peer_link(&remaining, "old").unwrap();
        retired.close();
        registry.on_peer_disconnected(&remaining, "old", retired.id);
        let (replacement, _replacement_rx, _) = registry.on_peer_connected(&remaining, "new");
        replacement.assignment_sent();
        assert!(
            sweep.next(&registry).is_none(),
            "retired tail must not publish"
        );
        sweep.refill(&registry, now + Duration::from_secs(5));
        let mut refreshed = Vec::new();
        while let Some(peer) = sweep.next(&registry) {
            refreshed.push((peer.address, peer.session));
        }
        assert_eq!(refreshed.len(), 3);
        assert!(refreshed.contains(&(remaining, "new".into())));
    }

    #[test]
    fn queued_assignment_acknowledgement_only_cancels_an_initial_retry() {
        let registry = PeersRegistry::new();
        let (peer, _rx, _) = registry.on_peer_connected("wallet", "session");
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        assert!(sweep.next(&registry).is_some());
        sweep.refill(&registry, now + Duration::from_secs(5));
        peer.assignment_sent();
        assert!(sweep.next(&registry).is_none());
        sweep.refill(&registry, now + FULL_INTERVAL);
        assert!(sweep.next(&registry).is_some());
    }

    #[test]
    fn a_queued_snapshot_cannot_announce_closed_or_replaced_incarnations() {
        let registry = PeersRegistry::new();
        let (closed, _rx_a, _) = registry.on_peer_connected("closed", "session");
        let (_, rx_b, _) = registry.on_peer_connected("dropped", "session");
        let (_, _rx_c, _) = registry.on_peer_connected("replaced", "session");
        let now = Instant::now();
        let mut sweep = Reconciliation::new(now);
        sweep.refill(&registry, now);
        closed.close();
        drop(rx_b);
        let (_, _rx_d, _) = registry.on_peer_connected("replaced", "session");
        assert!(sweep.next(&registry).is_none());
        sweep.refill(&registry, now + Duration::from_secs(5));
        assert_eq!(
            names(sweep.next(&registry)),
            Some(("replaced".into(), "session".into()))
        );
        assert!(sweep.next(&registry).is_none());
    }
}
