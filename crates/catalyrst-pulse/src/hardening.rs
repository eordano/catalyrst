#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DisconnectReason {
    Graceful = 1,

    AuthTimeout = 2,

    AuthFailed = 3,

    DuplicateSession = 4,

    Banned = 5,

    ServerFull = 6,

    PreAuthIpLimitExhausted = 7,

    PreAuthBudgetExhausted = 8,

    InputRateExceeded = 9,

    DiscreteEventRateExceeded = 10,

    InvalidInputField = 11,

    InvalidEmoteField = 12,

    InvalidTeleportField = 13,

    HandshakeReplayRejected = 14,

    InvalidHandshakeField = 15,

    PacketCorrupted = 16,
}

impl DisconnectReason {
    pub fn code(self) -> u32 {
        self as u32
    }
}

#[derive(Default)]
pub struct BanList {
    banned: std::collections::HashSet<String>,
}

impl BanList {
    pub fn new() -> Self {
        Self::default()
    }

    fn normalize(wallet: &str) -> String {
        let trimmed = wallet.trim();
        let with_prefix = if trimmed.len() >= 2 && trimmed[..2].eq_ignore_ascii_case("0x") {
            trimmed.to_string()
        } else {
            format!("0x{trimmed}")
        };
        with_prefix.to_lowercase()
    }

    pub fn is_banned(&self, wallet: &str) -> bool {
        self.banned.contains(&Self::normalize(wallet))
    }

    pub fn replace<I, S>(&mut self, addresses: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let next: std::collections::HashSet<String> = addresses
            .into_iter()
            .map(|a| Self::normalize(a.as_ref()))
            .collect();
        let newly_banned: Vec<String> = next
            .iter()
            .filter(|w| !self.banned.contains(*w))
            .cloned()
            .collect();
        self.banned = next;
        newly_banned
    }
}

pub struct HandshakeReplayPolicy {
    enabled: bool,
    ttl_ms: u32,
    max_entries: usize,
    seen: std::collections::HashMap<(String, String), u32>,
}

impl HandshakeReplayPolicy {
    pub fn new(enabled: bool, ttl_ms: u32, max_entries: usize) -> Self {
        Self {
            enabled,
            ttl_ms,
            max_entries,
            seen: std::collections::HashMap::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.ttl_ms > 0
    }

    pub fn try_admit(&mut self, now: u32, wallet: &str, timestamp: &str) -> bool {
        if !self.is_enabled() {
            return true;
        }
        let key = (wallet.to_lowercase(), timestamp.to_string());

        if let Some(&inserted_at) = self.seen.get(&key) {
            if now.wrapping_sub(inserted_at) < self.ttl_ms {
                return false;
            }
        }

        if self.seen.len() >= self.max_entries / 2 {
            self.sweep_expired(now);
        }

        if self.seen.len() >= self.max_entries {
            self.evict_oldest();
        }

        self.seen.insert(key, now);
        true
    }

    fn sweep_expired(&mut self, now: u32) {
        let ttl = self.ttl_ms;
        self.seen
            .retain(|_, &mut inserted_at| now.wrapping_sub(inserted_at) < ttl);
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest) = self
            .seen
            .iter()
            .min_by_key(|(_, &t)| t)
            .map(|(k, _)| k.clone())
        {
            self.seen.remove(&oldest);
        }
    }
}

pub struct HandshakeAttemptPolicy {
    max_attempts: u8,
}

impl HandshakeAttemptPolicy {
    pub fn new(max_attempts: u8) -> Self {
        Self { max_attempts }
    }

    pub fn is_enabled(&self) -> bool {
        self.max_attempts > 0
    }

    pub fn try_record_attempt(&self, attempts: u8) -> Option<u8> {
        if !self.is_enabled() {
            return Some(attempts);
        }
        if attempts >= self.max_attempts {
            return None;
        }
        Some(attempts + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmitResult {
    Ok,
    IpLimitExhausted,
    BudgetExhausted,
}

#[derive(Default)]
pub struct PreAuthAdmission {
    per_ip_cap: i64,
    global_budget: i64,
    per_ip_counts: std::collections::HashMap<String, i64>,
    ip_by_pending_peer: std::collections::HashMap<u32, String>,
    in_flight: i64,
}

impl PreAuthAdmission {
    pub fn new(per_ip_cap: i64, global_budget: i64) -> Self {
        Self {
            per_ip_cap,
            global_budget,
            ..Default::default()
        }
    }

    pub fn in_flight(&self) -> i64 {
        self.in_flight
    }

    pub fn try_admit(&mut self, peer_index: u32, ip: &str) -> AdmitResult {
        let per_ip = *self.per_ip_counts.get(ip).unwrap_or(&0);

        if self.per_ip_cap > 0 && per_ip >= self.per_ip_cap {
            return AdmitResult::IpLimitExhausted;
        }
        if self.global_budget > 0 && self.in_flight >= self.global_budget {
            return AdmitResult::BudgetExhausted;
        }

        self.per_ip_counts.insert(ip.to_string(), per_ip + 1);
        self.ip_by_pending_peer.insert(peer_index, ip.to_string());
        self.in_flight += 1;
        AdmitResult::Ok
    }

    pub fn release_on_promotion(&mut self, peer_index: u32) {
        self.release_internal(peer_index);
    }

    pub fn release_on_disconnect(&mut self, peer_index: u32) {
        self.release_internal(peer_index);
    }

    fn release_internal(&mut self, peer_index: u32) {
        let Some(ip) = self.ip_by_pending_peer.remove(&peer_index) else {
            return;
        };
        if let Some(c) = self.per_ip_counts.get_mut(&ip) {
            if *c <= 1 {
                self.per_ip_counts.remove(&ip);
            } else {
                *c -= 1;
            }
        }
        self.in_flight -= 1;
    }
}

pub fn pre_auth_refusal_reason(result: AdmitResult) -> Option<DisconnectReason> {
    match result {
        AdmitResult::Ok => None,
        AdmitResult::IpLimitExhausted => Some(DisconnectReason::PreAuthIpLimitExhausted),
        AdmitResult::BudgetExhausted => Some(DisconnectReason::PreAuthBudgetExhausted),
    }
}

pub const DEFAULT_PRE_AUTH_BUDGET: i64 = 512;

pub const DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP: i64 = 32;

pub const DEFAULT_MAX_HANDSHAKE_ATTEMPTS: u8 = 2;

pub const DEFAULT_MAX_EMOTE_ID_LENGTH: usize = 512;

pub const DEFAULT_MAX_EMOTE_DURATION_MS: u32 = 60_000;

pub const DEFAULT_CORRUPT_MAX_PER_MINUTE: u32 = 5;

pub const DEFAULT_CORRUPT_BURST: u32 = 5;

pub struct CorruptedPacketLimiter {
    peer_buckets: std::collections::HashMap<u32, CorruptBucket>,
    burst_capacity: u8,
    refill_interval_ms: u32,
}

#[derive(Clone, Copy)]
struct CorruptBucket {
    tokens: u8,
    last_refill_ms: u32,
}

impl CorruptedPacketLimiter {
    pub fn new(max_per_minute: u32, burst_capacity: u32) -> Self {
        Self {
            peer_buckets: std::collections::HashMap::new(),
            burst_capacity: burst_capacity.min(u8::MAX as u32) as u8,
            refill_interval_ms: 60_000u32.checked_div(max_per_minute).unwrap_or(0),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.refill_interval_ms > 0 && self.burst_capacity > 0
    }

    pub fn register_and_check_exhausted(&mut self, peer: u32, now_ms: u32) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let cap = self.burst_capacity;
        let interval = self.refill_interval_ms;
        let mut bucket = match self.peer_buckets.get(&peer) {
            Some(b) if b.last_refill_ms != 0 => {
                let mut b = *b;
                let refills = now_ms.wrapping_sub(b.last_refill_ms) / interval;
                if refills > 0 {
                    b.tokens = (b.tokens as u32 + refills).min(cap as u32) as u8;
                    b.last_refill_ms = b.last_refill_ms.wrapping_add(refills * interval);
                }
                b
            }

            _ => CorruptBucket {
                tokens: cap,
                last_refill_ms: if now_ms == 0 { 1 } else { now_ms },
            },
        };
        if bucket.tokens == 0 {
            self.peer_buckets.insert(peer, bucket);
            return true;
        }
        bucket.tokens -= 1;
        self.peer_buckets.insert(peer, bucket);
        false
    }

    pub fn release(&mut self, peer: u32) {
        self.peer_buckets.remove(&peer);
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::CorruptedPacketLimiter;

    #[test]
    fn tolerates_burst_then_exhausts() {
        let mut l = CorruptedPacketLimiter::new(5, 5);
        assert!(l.is_enabled());
        for _ in 0..5 {
            assert!(!l.register_and_check_exhausted(1, 1000));
        }
        assert!(l.register_and_check_exhausted(1, 1000));
    }

    #[test]
    fn refills_over_time() {
        let mut l = CorruptedPacketLimiter::new(5, 5);
        for _ in 0..6 {
            l.register_and_check_exhausted(1, 1000);
        }

        assert!(!l.register_and_check_exhausted(1, 13_000));
        assert!(l.register_and_check_exhausted(1, 13_000));
    }

    #[test]
    fn disabled_passes_everything() {
        let mut l = CorruptedPacketLimiter::new(0, 5);
        assert!(!l.is_enabled());
        for _ in 0..100 {
            assert!(!l.register_and_check_exhausted(1, 0));
        }
    }

    #[test]
    fn release_resets_the_bucket() {
        let mut l = CorruptedPacketLimiter::new(5, 5);
        for _ in 0..6 {
            l.register_and_check_exhausted(1, 1000);
        }
        l.release(1);
        assert!(!l.register_and_check_exhausted(1, 1000));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ban_list_normalizes_prefix_and_case() {
        let mut bans = BanList::new();
        bans.replace(["0xABC", "DEF"]);
        assert!(bans.is_banned("0xabc"));
        assert!(bans.is_banned("0xABC"), "case-insensitive");
        assert!(
            bans.is_banned("0xdef"),
            "missing-prefix entry gets 0x prefix"
        );
        assert!(bans.is_banned(" 0xabc \n"), "whitespace trimmed on query");
        assert!(!bans.is_banned("0x999"));
    }

    #[test]
    fn ban_list_replace_returns_newly_banned() {
        let mut bans = BanList::new();
        assert_eq!(bans.replace(["0x1"]), vec!["0x1".to_string()]);

        let newly = bans.replace(["0x1", "0x2"]);
        assert_eq!(newly, vec!["0x2".to_string()]);

        assert!(bans.is_banned("0x1"));
        assert!(bans.is_banned("0x2"));

        let newly = bans.replace(["0x2"]);
        assert!(
            newly.is_empty(),
            "0x2 was already banned, nothing newly added"
        );
        assert!(!bans.is_banned("0x1"));
        assert!(bans.is_banned("0x2"));
    }

    #[test]
    fn replay_first_admit_succeeds_then_dup_rejected() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 4096);
        assert!(p.try_admit(1000, "0xabc", "1700000000000"));

        assert!(!p.try_admit(1000, "0xabc", "1700000000000"));

        assert!(!p.try_admit(1000, "0xABC", "1700000000000"));
    }

    #[test]
    fn replay_same_pair_after_ttl_accepted() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 4096);
        assert!(p.try_admit(1000, "0xabc", "ts"));

        assert!(p.try_admit(1000 + 30_001, "0xabc", "ts"));
    }

    #[test]
    fn replay_different_timestamp_or_wallet_accepted() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 4096);
        assert!(p.try_admit(1000, "0xabc", "t1"));
        assert!(
            p.try_admit(1000, "0xabc", "t2"),
            "fresh timestamp = legit reconnect"
        );
        assert!(
            p.try_admit(1000, "0xdef", "t1"),
            "different wallet, same ts"
        );
    }

    #[test]
    fn replay_disabled_admits_everything() {
        let mut p = HandshakeReplayPolicy::new(false, 30_000, 4096);
        for _ in 0..100 {
            assert!(p.try_admit(1000, "0xabc", "ts"));
        }
    }

    #[test]
    fn replay_zero_ttl_disables_cache() {
        let mut p = HandshakeReplayPolicy::new(true, 0, 4096);
        for _ in 0..100 {
            assert!(p.try_admit(1000, "0xabc", "ts"));
        }
    }

    #[test]
    fn replay_overflow_does_not_reject_fresh_handshakes() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 2);
        assert!(p.try_admit(1000, "0x1", "t1"));
        assert!(p.try_admit(1000, "0x2", "t2"));

        assert!(p.try_admit(1000, "0x3", "t3"));
    }

    #[test]
    fn attempt_throttle_rejects_after_max() {
        let p = HandshakeAttemptPolicy::new(2);

        assert_eq!(p.try_record_attempt(0), Some(1));
        assert_eq!(p.try_record_attempt(1), Some(2));
        assert_eq!(p.try_record_attempt(2), None);
    }

    #[test]
    fn attempt_throttle_disabled_when_max_zero() {
        let p = HandshakeAttemptPolicy::new(0);
        assert_eq!(p.try_record_attempt(255), Some(255));
    }

    #[test]
    fn pre_auth_budget_caps_global_in_flight() {
        let mut a = PreAuthAdmission::new(0, 2);
        assert_eq!(a.try_admit(1, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(2, "2.2.2.2"), AdmitResult::Ok);
        assert_eq!(a.try_admit(3, "3.3.3.3"), AdmitResult::BudgetExhausted);
        assert_eq!(a.in_flight(), 2);

        a.release_on_promotion(1);
        assert_eq!(a.in_flight(), 1);
        assert_eq!(a.try_admit(3, "3.3.3.3"), AdmitResult::Ok);
    }

    #[test]
    fn pre_auth_per_ip_cap_isolates_one_ip() {
        let mut a = PreAuthAdmission::new(2, 0);
        assert_eq!(a.try_admit(1, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(2, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(3, "1.1.1.1"), AdmitResult::IpLimitExhausted);

        assert_eq!(a.try_admit(4, "2.2.2.2"), AdmitResult::Ok);

        a.release_on_disconnect(1);
        assert_eq!(a.try_admit(3, "1.1.1.1"), AdmitResult::Ok);
    }

    #[test]
    fn pre_auth_release_is_idempotent() {
        let mut a = PreAuthAdmission::new(0, 4);
        a.try_admit(1, "1.1.1.1");
        a.release_on_promotion(1);

        a.release_on_disconnect(1);
        assert_eq!(a.in_flight(), 0);
    }

    #[test]
    fn pre_auth_refusal_reason_maps_codes() {
        assert_eq!(pre_auth_refusal_reason(AdmitResult::Ok), None);
        assert_eq!(
            pre_auth_refusal_reason(AdmitResult::IpLimitExhausted),
            Some(DisconnectReason::PreAuthIpLimitExhausted)
        );
        assert_eq!(
            pre_auth_refusal_reason(AdmitResult::BudgetExhausted),
            Some(DisconnectReason::PreAuthBudgetExhausted)
        );
    }

    #[test]
    fn disconnect_reason_codes_match_upstream() {
        assert_eq!(DisconnectReason::AuthTimeout.code(), 2);
        assert_eq!(DisconnectReason::DuplicateSession.code(), 4);
        assert_eq!(DisconnectReason::Banned.code(), 5);
        assert_eq!(DisconnectReason::HandshakeReplayRejected.code(), 14);
        assert_eq!(DisconnectReason::InvalidEmoteField.code(), 12);
    }
}
