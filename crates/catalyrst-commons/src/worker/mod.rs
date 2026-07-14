//! Background loops: [`spawn_periodic`] for anything on a timer, and (behind the `pg`
//! feature) [`spawn_invalidation_listener`] for postgres LISTEN/NOTIFY fanout.

mod periodic;

#[cfg(feature = "pg")]
mod listener;

pub use periodic::{spawn_periodic, Pacing, PeriodicCfg};

#[cfg(feature = "pg")]
pub use listener::spawn_invalidation_listener;
