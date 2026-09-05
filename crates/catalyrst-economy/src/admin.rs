use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

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

#[derive(Debug)]
pub struct RuntimeConfig {
    enabled: AtomicBool,

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
