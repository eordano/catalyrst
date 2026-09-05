//! The five `WORLDS_FED_*` environment keys, parsed once at boot.
//!
//! Every one of these is validated here rather than at the point of use, because the
//! point of use is a background poller: a bad value discovered there degrades a
//! running process instead of refusing to start one. `get_u64` already returns
//! `Result`, so an unparseable value is a boot failure for free; zero is rejected
//! explicitly, since a zero poll interval is a busy loop against a peer and a zero
//! byte or row cap is a mirror that silently stores nothing.

use anyhow::{anyhow, Result};
use catalyrst_envcfg::{env_bool, get_u64};
use std::path::PathBuf;

/// 5 minutes. Slow enough that a peer never sees us as load, fast enough that a
/// world published on a peer shows up within one coffee.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 300;
/// 4 MiB. The production worlds corpus (`worlds-content-server/index`, 1552 worlds)
/// is ~2.1 MB, so this is roughly 2x head-room over the largest listing known to
/// exist. Enforced by a streaming byte counter *before* parse, never after.
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// 10000 rows per peer. Same corpus is 1552.
pub const DEFAULT_MAX_WORLDS_PER_PEER: u64 = 10_000;

/// Resolved worlds-federation configuration.
///
/// `peers_file` being `None` is the *only* representation of "federation was never
/// requested". It is never conflated with "the file loaded and yielded nothing" --
/// that distinction is carried by
/// [`crate::fed::peers::WorldsFederationPeers`], not by this struct.
#[derive(Debug, Clone)]
pub struct WorldsFedConfig {
    /// `WORLDS_FED_PEERS_FILE`. `None` when unset or empty.
    pub peers_file: Option<PathBuf>,
    /// `WORLDS_FED_POLL_INTERVAL_SECS`, always >= 1.
    pub poll_interval_secs: u64,
    /// `WORLDS_FED_MAX_RESPONSE_BYTES`, always >= 1.
    pub max_response_bytes: u64,
    /// `WORLDS_FED_MAX_WORLDS_PER_PEER`, always >= 1.
    pub max_worlds_per_peer: u64,
    /// `WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS`. Permits a `http://127.0.0.1:*`
    /// peer with no pinned root, and nothing else. Follows the existing
    /// `LIVEKIT_ALLOW_DEV_CREDS` precedent in [`crate::config`]: a dev escape hatch
    /// that is named in its own warning line every time it fires.
    pub allow_insecure_loopback_peers: bool,
}

impl Default for WorldsFedConfig {
    fn default() -> Self {
        Self {
            peers_file: None,
            poll_interval_secs: DEFAULT_POLL_INTERVAL_SECS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_worlds_per_peer: DEFAULT_MAX_WORLDS_PER_PEER,
            allow_insecure_loopback_peers: false,
        }
    }
}

fn positive(name: &str, value: u64) -> Result<u64> {
    if value == 0 {
        return Err(anyhow!("{name} must be a positive integer, got {value}"));
    }
    Ok(value)
}

impl WorldsFedConfig {
    pub fn from_env() -> Result<Self> {
        let peers_file = std::env::var("WORLDS_FED_PEERS_FILE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        Ok(Self {
            peers_file,
            poll_interval_secs: positive(
                "WORLDS_FED_POLL_INTERVAL_SECS",
                get_u64("WORLDS_FED_POLL_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS)?,
            )?,
            max_response_bytes: positive(
                "WORLDS_FED_MAX_RESPONSE_BYTES",
                get_u64("WORLDS_FED_MAX_RESPONSE_BYTES", DEFAULT_MAX_RESPONSE_BYTES)?,
            )?,
            max_worlds_per_peer: positive(
                "WORLDS_FED_MAX_WORLDS_PER_PEER",
                get_u64(
                    "WORLDS_FED_MAX_WORLDS_PER_PEER",
                    DEFAULT_MAX_WORLDS_PER_PEER,
                )?,
            )?,
            allow_insecure_loopback_peers: env_bool(
                "WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS",
                false,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_refused_for_every_cap_with_the_crate_message_shape() {
        // Same wording as `config::positive_limit`, so the two families of knobs
        // read identically in a boot log.
        for key in [
            "WORLDS_FED_POLL_INTERVAL_SECS",
            "WORLDS_FED_MAX_RESPONSE_BYTES",
            "WORLDS_FED_MAX_WORLDS_PER_PEER",
        ] {
            let err = positive(key, 0).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("{key} must be a positive integer, got 0")
            );
            assert_eq!(positive(key, 1).unwrap(), 1);
        }
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let d = WorldsFedConfig::default();
        assert!(d.peers_file.is_none(), "federation is off unless asked for");
        assert!(!d.allow_insecure_loopback_peers);
        assert_eq!(d.poll_interval_secs, 300);
        assert_eq!(d.max_response_bytes, 4 * 1024 * 1024);
        assert_eq!(d.max_worlds_per_peer, 10_000);
    }
}
