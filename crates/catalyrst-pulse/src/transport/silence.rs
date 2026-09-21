use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const RELAY_SILENCE: Duration = Duration::from_millis(super::host::RELAY_TIMEOUT_MS as u64);

#[derive(Default)]
pub struct RelaySilence {
    heard: HashMap<u32, Instant>,
}

impl RelaySilence {
    pub fn watch(&mut self, peer: u32, watched: bool, now: Instant) {
        if watched {
            self.heard.entry(peer).or_insert(now);
        } else {
            self.heard.remove(&peer);
        }
    }

    pub fn heard(&mut self, peer: u32, now: Instant) {
        if let Some(at) = self.heard.get_mut(&peer) {
            *at = now;
        }
    }

    pub fn forget(&mut self, peer: u32) {
        self.heard.remove(&peer);
    }

    pub fn silent(&self, now: Instant) -> Vec<u32> {
        let mut peers: Vec<u32> = self
            .heard
            .iter()
            .filter(|(_, at)| now.saturating_duration_since(**at) >= RELAY_SILENCE)
            .map(|(peer, _)| *peer)
            .collect();
        peers.sort_unstable();
        peers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(4999);

    #[test]
    fn a_watched_peer_is_silent_five_seconds_after_it_was_last_heard() {
        let start = Instant::now();
        let mut silence = RelaySilence::default();
        silence.watch(7, true, start);
        silence.watch(9, true, start);
        assert_eq!(silence.silent(start + SHORT), Vec::<u32>::new());
        assert_eq!(silence.silent(start + RELAY_SILENCE), vec![7, 9]);
        silence.heard(9, start + SHORT);
        assert_eq!(silence.silent(start + RELAY_SILENCE), vec![7]);
        assert_eq!(silence.silent(start + SHORT + SHORT), vec![7]);
        assert_eq!(silence.silent(start + SHORT + RELAY_SILENCE), vec![7, 9]);
    }

    #[test]
    fn watching_again_keeps_the_clock_and_hearing_an_unwatched_peer_starts_none() {
        let start = Instant::now();
        let mut silence = RelaySilence::default();
        silence.watch(7, true, start);
        silence.watch(7, true, start + SHORT);
        assert_eq!(silence.silent(start + RELAY_SILENCE), vec![7]);
        silence.heard(8, start);
        assert_eq!(silence.silent(start + RELAY_SILENCE), vec![7]);
    }

    #[test]
    fn a_peer_that_lost_its_last_scope_or_its_connection_is_no_longer_watched() {
        let start = Instant::now();
        let mut silence = RelaySilence::default();
        silence.watch(7, true, start);
        silence.watch(9, true, start);
        silence.watch(7, false, start);
        silence.forget(9);
        assert_eq!(silence.silent(start + RELAY_SILENCE), Vec::<u32>::new());
    }
}
