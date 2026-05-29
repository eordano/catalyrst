use std::sync::atomic::{AtomicU8, Ordering};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Bootstrapping = 0,
    Syncing = 1,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Bootstrapping => write!(f, "Bootstrapping"),
            State::Syncing => write!(f, "Syncing"),
        }
    }
}

pub struct SynchronizationState {
    state: AtomicU8,
}

impl Default for SynchronizationState {
    fn default() -> Self {
        Self::new()
    }
}

impl SynchronizationState {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(State::Bootstrapping as u8),
        }
    }

    pub fn get_state(&self) -> State {
        match self.state.load(Ordering::Relaxed) {
            0 => State::Bootstrapping,
            _ => State::Syncing,
        }
    }

    pub fn to_syncing(&self) {
        info!("Switching to syncing state...");
        self.state.store(State::Syncing as u8, Ordering::Relaxed);
    }
}
