//! Runtime-mutable relayer controls (admin surface).
//!
//! Upstream `transactions-server` (and the NOW tranche of this crate) decide the
//! broadcast provider **once, at startup**: whichever of the OZ HTTP relayer or
//! the direct JSON-RPC signer is provisioned is wired into
//! [`crate::ports::transaction::TransactionComponent`] and never changes for the
//! life of the process. The admin-console roadmap (docs/admin-console.md §4,
//! "Relayer on/off, signer switch") asks for that to become runtime-mutable so an
//! operator can pause broadcasting (e.g. relayer key is drained / paused for
//! maintenance) or flip which provisioned provider is preferred, all without a
//! restart.
//!
//! This module holds that mutable state in lock-free atomics. It is **purely
//! additive**: the default state (`enabled = true`, `signer = Auto`) reproduces
//! the existing startup-only behaviour exactly — relayer first, then direct
//! signer, then 503. Only an authenticated admin call changes it.
//!
//! State is process-local (no DB / schema): the toggle is an operational switch,
//! not durable domain data, so it deliberately resets to the provisioned default
//! on restart (matching "startup decides the default; admin overrides at runtime").

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Which provisioned broadcast provider to prefer when more than one is wired up.
///
/// `Auto` is the historical behaviour: prefer the OZ relayer, fall back to the
/// direct JSON-RPC signer. `Oz` / `Direct` pin a specific provider; if that
/// provider is not provisioned, broadcast falls through to the 503 path (the
/// switch never *invents* a provider, it only selects among the configured ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignerPreference {
    Auto,
    Oz,
    Direct,
}

impl SignerPreference {
    fn to_u8(self) -> u8 {
        match self {
            SignerPreference::Auto => 0,
            SignerPreference::Oz => 1,
            SignerPreference::Direct => 2,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => SignerPreference::Oz,
            2 => SignerPreference::Direct,
            _ => SignerPreference::Auto,
        }
    }
}

/// Lock-free runtime controls shared (via `Arc`) into the transaction component
/// and read on every broadcast.
#[derive(Debug)]
pub struct RuntimeConfig {
    /// Master broadcast switch. When `false`, [`crate::ports::transaction`]
    /// short-circuits to the existing "broadcast unavailable" 503 even if a
    /// provider is provisioned. Defaults to `true` (broadcast on).
    enabled: AtomicBool,
    /// Provider preference, encoded as a [`SignerPreference`] discriminant.
    signer: AtomicU8,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            signer: AtomicU8::new(SignerPreference::Auto.to_u8()),
        }
    }
}

impl RuntimeConfig {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn relayer_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_relayer_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn signer_preference(&self) -> SignerPreference {
        SignerPreference::from_u8(self.signer.load(Ordering::Relaxed))
    }

    pub fn set_signer_preference(&self, pref: SignerPreference) {
        self.signer.store(pref.to_u8(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_preserve_startup_behaviour() {
        let rc = RuntimeConfig::default();
        assert!(rc.relayer_enabled());
        assert_eq!(rc.signer_preference(), SignerPreference::Auto);
    }

    #[test]
    fn toggle_and_switch_round_trip() {
        let rc = RuntimeConfig::default();
        rc.set_relayer_enabled(false);
        assert!(!rc.relayer_enabled());
        rc.set_relayer_enabled(true);
        assert!(rc.relayer_enabled());

        rc.set_signer_preference(SignerPreference::Direct);
        assert_eq!(rc.signer_preference(), SignerPreference::Direct);
        rc.set_signer_preference(SignerPreference::Oz);
        assert_eq!(rc.signer_preference(), SignerPreference::Oz);
    }
}
