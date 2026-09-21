use std::io;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const QUEUED_PEER_BYTES: usize = 64 * 1024;
const QUEUED_GLOBAL_BYTES: usize = 8 * 1024 * 1024;
const QUEUED_CONTROL_BYTES: usize = 8 * 1024;
const QUEUED_GLOBAL_CONTROL_BYTES: usize = 2 * 1024 * 1024;
const PACKET_COST: usize = 64;

#[derive(Clone)]
pub(super) struct ApplicationGlobal {
    data: Arc<Semaphore>,
    control: Arc<Semaphore>,
}

#[derive(Clone)]
pub(super) struct ApplicationBudget {
    global: Arc<Semaphore>,
    global_control: Arc<Semaphore>,
    peer: Arc<Semaphore>,
    control: Arc<Semaphore>,
}

impl ApplicationBudget {
    pub(super) fn global() -> ApplicationGlobal {
        ApplicationGlobal {
            data: Arc::new(Semaphore::new(QUEUED_GLOBAL_BYTES)),
            control: Arc::new(Semaphore::new(QUEUED_GLOBAL_CONTROL_BYTES)),
        }
    }

    pub(super) fn new(global: ApplicationGlobal) -> Self {
        Self {
            global: global.data,
            global_control: global.control,
            peer: Arc::new(Semaphore::new(QUEUED_PEER_BYTES)),
            control: Arc::new(Semaphore::new(QUEUED_CONTROL_BYTES)),
        }
    }

    pub(super) fn reserve(&self, bytes: usize, control: bool) -> io::Result<ApplicationPermit> {
        let cost = bytes
            .checked_add(PACKET_COST)
            .and_then(|cost| u32::try_from(cost).ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "application packet too large")
            })?;
        let acquire = |semaphore: &Arc<Semaphore>| {
            semaphore
                .clone()
                .try_acquire_many_owned(cost)
                .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "application queue full"))
        };
        let data = acquire(&self.global).and_then(|global| {
            Ok(ApplicationPermit {
                _global: Some(global),
                _peer: acquire(&self.peer)?,
                _global_control: None,
            })
        });
        match data {
            Err(_) if control => Ok(ApplicationPermit {
                _global: None,
                _peer: acquire(&self.control)?,
                _global_control: Some(acquire(&self.global_control)?),
            }),
            data => data,
        }
    }
}

pub(super) struct ApplicationPermit {
    _global: Option<OwnedSemaphorePermit>,
    _peer: OwnedSemaphorePermit,
    _global_control: Option<OwnedSemaphorePermit>,
}

pub(super) struct ApplicationPayload {
    pub(super) bytes: Bytes,
    pub(super) _permit: ApplicationPermit,
}

impl AsRef<[u8]> for ApplicationPayload {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_owner_retains_budget_through_clones_and_releases_on_last_drop() {
        let budget = ApplicationBudget::new(ApplicationBudget::global());
        let permit = budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .unwrap();
        let bytes = Bytes::from_owner(ApplicationPayload {
            bytes: Bytes::from_static(b"payload"),
            _permit: permit,
        });
        let clone = bytes.clone();
        drop(bytes);
        assert_eq!(
            budget.reserve(1, false).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(clone);
        assert!(budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .is_ok());
    }

    #[test]
    fn peer_pressure_does_not_leak_global_permits_or_block_other_peers() {
        let global = ApplicationBudget::global();
        let budget = ApplicationBudget::new(global.clone());
        let other = ApplicationBudget::new(global.clone());
        let _held = budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .unwrap();
        for _ in 0..1000 {
            assert!(budget.reserve(1, false).is_err());
        }
        assert_eq!(
            global.data.available_permits(),
            QUEUED_GLOBAL_BYTES - QUEUED_PEER_BYTES
        );
        assert!(other
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .is_ok());
    }

    const SELF_LEFT_NOTICE_BYTES: usize = 14 + 4;
    const LARGEST_FRAME_BYTES: usize = 4096 + 4;

    #[test]
    fn control_notices_queue_past_a_full_peer_budget_within_their_own_bound() {
        let budget = ApplicationBudget::new(ApplicationBudget::global());
        let held = budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .unwrap();
        assert_eq!(
            budget
                .reserve(SELF_LEFT_NOTICE_BYTES, false)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        let mut notices: Vec<_> = (0..16)
            .map(|_| budget.reserve(SELF_LEFT_NOTICE_BYTES, true))
            .collect();
        notices.push(budget.reserve(LARGEST_FRAME_BYTES, true));
        assert!(notices.iter().all(Result::is_ok));
        notices.extend(
            (0..QUEUED_CONTROL_BYTES / PACKET_COST)
                .map(|_| budget.reserve(SELF_LEFT_NOTICE_BYTES, true)),
        );
        assert_eq!(
            notices.last().unwrap().as_ref().err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(notices);
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
        drop(held);
        assert_eq!(budget.peer.available_permits(), QUEUED_PEER_BYTES);
    }

    #[test]
    fn control_notices_queue_past_a_full_global_budget_without_touching_it() {
        let global = ApplicationBudget::global();
        let _held: Vec<_> = (0..QUEUED_GLOBAL_BYTES / QUEUED_PEER_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
                    .unwrap()
            })
            .collect();
        let budget = ApplicationBudget::new(global.clone());
        assert!(budget.reserve(SELF_LEFT_NOTICE_BYTES, false).is_err());
        let notice = budget.reserve(SELF_LEFT_NOTICE_BYTES, true);
        assert!(notice.is_ok());
        assert_eq!(global.data.available_permits(), 0);
        assert_eq!(budget.peer.available_permits(), QUEUED_PEER_BYTES);
    }

    #[test]
    fn overall_control_cap_refuses_a_notice_across_connections_and_releases_on_drop() {
        let global = ApplicationBudget::global();
        let _data: Vec<_> = (0..QUEUED_GLOBAL_BYTES / QUEUED_PEER_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
                    .unwrap()
            })
            .collect();
        let mut held: Vec<_> = (0..QUEUED_GLOBAL_CONTROL_BYTES / QUEUED_CONTROL_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_CONTROL_BYTES - PACKET_COST, true)
                    .unwrap()
            })
            .collect();
        let budget = ApplicationBudget::new(global.clone());
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
        let refused = budget.reserve(SELF_LEFT_NOTICE_BYTES, true);
        assert_eq!(
            refused.err().map(|error| error.kind()),
            Some(io::ErrorKind::WouldBlock)
        );
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
        held.pop();
        let notice = budget.reserve(SELF_LEFT_NOTICE_BYTES, true);
        assert!(notice.is_ok());
        assert_eq!(
            global.control.available_permits(),
            QUEUED_CONTROL_BYTES - SELF_LEFT_NOTICE_BYTES - PACKET_COST
        );
        drop(notice);
        drop(held);
        assert_eq!(
            global.control.available_permits(),
            QUEUED_GLOBAL_CONTROL_BYTES
        );
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
    }

    #[test]
    fn a_full_connection_reserve_takes_nothing_from_the_overall_control_cap() {
        let global = ApplicationBudget::global();
        let budget = ApplicationBudget::new(global.clone());
        let _data = budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .unwrap();
        let _held = budget
            .reserve(QUEUED_CONTROL_BYTES - PACKET_COST, true)
            .unwrap();
        for _ in 0..1000 {
            assert!(budget.reserve(SELF_LEFT_NOTICE_BYTES, true).is_err());
        }
        assert_eq!(
            global.control.available_permits(),
            QUEUED_GLOBAL_CONTROL_BYTES - QUEUED_CONTROL_BYTES
        );
    }

    #[test]
    fn a_payload_never_draws_on_either_control_budget() {
        let global = ApplicationBudget::global();
        let budget = ApplicationBudget::new(global.clone());
        let _held = budget
            .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
            .unwrap();
        let refused = budget.reserve(SELF_LEFT_NOTICE_BYTES, false);
        assert_eq!(
            refused.err().map(|error| error.kind()),
            Some(io::ErrorKind::WouldBlock)
        );
        let _data: Vec<_> = (1..QUEUED_GLOBAL_BYTES / QUEUED_PEER_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
                    .unwrap()
            })
            .collect();
        let other = ApplicationBudget::new(global.clone());
        let refused = other.reserve(SELF_LEFT_NOTICE_BYTES, false);
        assert_eq!(
            refused.err().map(|error| error.kind()),
            Some(io::ErrorKind::WouldBlock)
        );
        assert_eq!(global.data.available_permits(), 0);
        assert_eq!(
            global.control.available_permits(),
            QUEUED_GLOBAL_CONTROL_BYTES
        );
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
        assert_eq!(other.control.available_permits(), QUEUED_CONTROL_BYTES);
    }

    #[test]
    fn control_notices_use_the_data_budget_while_it_has_room() {
        let global = ApplicationBudget::global();
        let budget = ApplicationBudget::new(global.clone());
        let notice = budget.reserve(SELF_LEFT_NOTICE_BYTES, true);
        assert!(notice.is_ok());
        assert_eq!(budget.control.available_permits(), QUEUED_CONTROL_BYTES);
        assert_eq!(
            global.data.available_permits(),
            QUEUED_GLOBAL_BYTES - SELF_LEFT_NOTICE_BYTES - PACKET_COST
        );
    }

    #[test]
    fn global_pressure_bounds_many_individually_healthy_peers() {
        let global = ApplicationBudget::global();
        let permits: Vec<_> = (0..QUEUED_GLOBAL_BYTES / QUEUED_PEER_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_PEER_BYTES - PACKET_COST, false)
                    .unwrap()
            })
            .collect();
        assert!(ApplicationBudget::new(global.clone())
            .reserve(1, false)
            .is_err());
        drop(permits);
        assert_eq!(global.data.available_permits(), QUEUED_GLOBAL_BYTES);
    }
}
