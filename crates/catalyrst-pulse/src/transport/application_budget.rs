use std::io;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const QUEUED_PEER_BYTES: usize = 64 * 1024;
const QUEUED_GLOBAL_BYTES: usize = 8 * 1024 * 1024;
const PACKET_COST: usize = 64;

#[derive(Clone)]
pub(super) struct ApplicationBudget {
    global: Arc<Semaphore>,
    peer: Arc<Semaphore>,
}

impl ApplicationBudget {
    pub(super) fn global() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(QUEUED_GLOBAL_BYTES))
    }

    pub(super) fn new(global: Arc<Semaphore>) -> Self {
        Self {
            global,
            peer: Arc::new(Semaphore::new(QUEUED_PEER_BYTES)),
        }
    }

    pub(super) fn reserve(&self, bytes: usize) -> io::Result<ApplicationPermit> {
        let cost = bytes
            .checked_add(PACKET_COST)
            .and_then(|cost| u32::try_from(cost).ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "application packet too large")
            })?;
        let full = || io::Error::new(io::ErrorKind::WouldBlock, "application queue full");
        let global = self
            .global
            .clone()
            .try_acquire_many_owned(cost)
            .map_err(|_| full())?;
        let peer = self
            .peer
            .clone()
            .try_acquire_many_owned(cost)
            .map_err(|_| full())?;
        Ok(ApplicationPermit {
            _global: global,
            _peer: peer,
        })
    }
}

pub(super) struct ApplicationPermit {
    _global: OwnedSemaphorePermit,
    _peer: OwnedSemaphorePermit,
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
        let permit = budget.reserve(QUEUED_PEER_BYTES - PACKET_COST).unwrap();
        let bytes = Bytes::from_owner(ApplicationPayload {
            bytes: Bytes::from_static(b"payload"),
            _permit: permit,
        });
        let clone = bytes.clone();
        drop(bytes);
        assert_eq!(
            budget.reserve(1).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(clone);
        assert!(budget.reserve(QUEUED_PEER_BYTES - PACKET_COST).is_ok());
    }

    #[test]
    fn peer_pressure_does_not_leak_global_permits_or_block_other_peers() {
        let global = ApplicationBudget::global();
        let budget = ApplicationBudget::new(global.clone());
        let other = ApplicationBudget::new(global.clone());
        let _held = budget.reserve(QUEUED_PEER_BYTES - PACKET_COST).unwrap();
        for _ in 0..1000 {
            assert!(budget.reserve(1).is_err());
        }
        assert_eq!(
            global.available_permits(),
            QUEUED_GLOBAL_BYTES - QUEUED_PEER_BYTES
        );
        assert!(other.reserve(QUEUED_PEER_BYTES - PACKET_COST).is_ok());
    }

    #[test]
    fn global_pressure_bounds_many_individually_healthy_peers() {
        let global = ApplicationBudget::global();
        let permits: Vec<_> = (0..QUEUED_GLOBAL_BYTES / QUEUED_PEER_BYTES)
            .map(|_| {
                ApplicationBudget::new(global.clone())
                    .reserve(QUEUED_PEER_BYTES - PACKET_COST)
                    .unwrap()
            })
            .collect();
        assert!(ApplicationBudget::new(global.clone()).reserve(1).is_err());
        drop(permits);
        assert_eq!(global.available_permits(), QUEUED_GLOBAL_BYTES);
    }
}
