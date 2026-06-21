//! Anti-abuse hardening layer — a faithful port of `decentraland/Pulse`
//! `Transport/Hardening/*` + `Messaging/Hardening/*` (minus the wire codec, which
//! lives in [`crate::messages`]). These are the non-wire defenses the prior pass
//! left out: anti-replay, ban enforcement, handshake-attempt throttling, pre-auth
//! admission budgeting, and the emote field caps.
//!
//! Upstream runs each defense across lock-guarded worker shards; catalyrst drives
//! a single-writer server loop ([`crate::server::PulseServer`]), so the same state
//! is held behind plain `&mut` access with identical admission semantics. Each
//! policy reports its decision (admit / reject + a [`DisconnectReason`]) and the
//! server emits the transport-level `disconnect(peer, reason)` so the client gets
//! a reason code rather than a silent slot reclaim.

/// Why the transport tore a peer down. Mirrors
/// `DCLPulse.Transport.Shared/Runtime/DisconnectReason.cs` exactly — the integer
/// values are carried verbatim in the 32-bit ENet disconnect-data field so the
/// Unity client reads the same code upstream emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DisconnectReason {
    /// Clean shutdown / server stopping.
    Graceful = 1,
    /// PENDING_AUTH deadline exceeded.
    AuthTimeout = 2,
    /// Handshake validation failed (or attempt throttle exceeded).
    AuthFailed = 3,
    /// Evicted by a newer connection with the same `player_id`.
    DuplicateSession = 4,
    /// Banned platform-wide.
    Banned = 5,
    /// Peer pool exhausted.
    ServerFull = 6,
    /// Per-source-IP pre-auth connection cap exceeded.
    PreAuthIpLimitExhausted = 7,
    /// Global pre-auth budget exhausted.
    PreAuthBudgetExhausted = 8,
    /// PlayerStateInput sent faster than the server's `MaxHz` cap.
    InputRateExceeded = 9,
    /// Discrete-event (emote/teleport) token bucket exhausted.
    DiscreteEventRateExceeded = 10,
    /// PlayerStateInput carried an invalid field.
    InvalidInputField = 11,
    /// EmoteStart carried an invalid field (oversized id, excessive duration, bad parcel).
    InvalidEmoteField = 12,
    /// TeleportRequest carried an invalid field.
    InvalidTeleportField = 13,
    /// `(wallet, timestamp)` already accepted within the anti-replay window.
    HandshakeReplayRejected = 14,
    /// HandshakeRequest carried a malformed `PlayerInitialState`.
    InvalidHandshakeField = 15,
    /// Sustained corrupted-packet rate over the transport budget.
    PacketCorrupted = 16,
}

impl DisconnectReason {
    /// The raw `u32` carried in the ENet disconnect-data field.
    pub fn code(self) -> u32 {
        self as u32
    }
}

// ── BanList (Messaging/Hardening/BanList.cs) ────────────────────────────────────

/// Shared wallet blocklist. Case-insensitive (gatekeeper may not return
/// checksum-matching addresses) and `0x`-prefix-normalized (so a raw-hex export
/// doesn't silently disable a ban entry). [`BanList::replace`] swaps the whole set
/// and returns the newly-banned wallets so the caller can evict matching peers
/// mid-session.
#[derive(Default)]
pub struct BanList {
    banned: std::collections::HashSet<String>,
}

impl BanList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bring a gatekeeper-supplied wallet onto the canonical shape every
    /// authenticated peer presents: trimmed, `0x`-prefixed, lower-cased (the
    /// lower-case stands in for upstream's `OrdinalIgnoreCase` comparer).
    fn normalize(wallet: &str) -> String {
        let trimmed = wallet.trim();
        let with_prefix = if trimmed.len() >= 2 && trimmed[..2].eq_ignore_ascii_case("0x") {
            trimmed.to_string()
        } else {
            format!("0x{trimmed}")
        };
        with_prefix.to_lowercase()
    }

    /// Is `wallet` currently banned?
    pub fn is_banned(&self, wallet: &str) -> bool {
        self.banned.contains(&Self::normalize(wallet))
    }

    /// Replace the blocklist with `addresses`; returns the wallets that were not in
    /// the previous set (the enforcer kicks peers whose wallet just became banned).
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

// ── HandshakeReplayPolicy (Messaging/Hardening/HandshakeReplayPolicy.cs) ─────────

/// Sliding-window cache of accepted `(wallet, timestamp)` handshake pairs. Rejects
/// a pair already admitted within the PENDING_AUTH window (`ttl_ms`), so a captured
/// handshake packet can't be replayed while the original is still in-flight.
///
/// Both knobs are derived, not duplicated: `ttl_ms` tracks
/// `PeerOptions.PendingAuthCleanTimeoutMs` (single source of truth for how long
/// PENDING_AUTH state lives) and `max_entries` tracks `ENetTransportOptions.MaxPeers`
/// (connects in flight can't exceed the peer pool). Stores the *insertion* time, not
/// the expiry, so `(now - inserted_at) < ttl_ms` is wrap-safe under `u32` monotonic
/// arithmetic.
pub struct HandshakeReplayPolicy {
    enabled: bool,
    ttl_ms: u32,
    max_entries: usize,
    seen: std::collections::HashMap<(String, String), u32>,
}

impl HandshakeReplayPolicy {
    /// `ttl_ms` = `PeerOptions.PendingAuthCleanTimeoutMs`, `max_entries` =
    /// `ENetTransportOptions.MaxPeers`.
    pub fn new(enabled: bool, ttl_ms: u32, max_entries: usize) -> Self {
        Self {
            enabled,
            ttl_ms,
            max_entries,
            seen: std::collections::HashMap::new(),
        }
    }

    /// `enabled && ttl_ms > 0` — a zero TTL has no meaningful window so the cache
    /// degrades to pass-through (`IsEnabled`).
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.ttl_ms > 0
    }

    /// Records the pair if it's fresh; returns `false` (reject) if the same pair was
    /// admitted earlier within the TTL window. Wallet is lower-cased so a case-flipped
    /// address can't bypass the cache (EIP-55 is only a checksum).
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

        // Opportunistic sweep when the cache starts getting large.
        if self.seen.len() >= self.max_entries / 2 {
            self.sweep_expired(now);
        }
        // Fail-closed: at the hard cap, force-evict the oldest to make room rather
        // than silently skipping the insert (which would let a flooded cache replay
        // any subsequent legitimate handshake).
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
        // Oldest by insertion time. `wrapping` math doesn't give a total order, so
        // (matching the small bounded cache) compare raw inserted_at like upstream.
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

// ── HandshakeAttemptPolicy (Messaging/Hardening/HandshakeAttemptPolicy.cs) ───────

/// Per-peer handshake-attempt throttle. Stops a malicious/buggy client from burning
/// server CPU on ECDSA recovery by replaying HandshakeRequests. The counter lives on
/// the peer's transport state ([`crate::simulation::PeerState::handshake_attempts`]);
/// on overflow the peer is rejected with [`DisconnectReason::AuthFailed`]. A reconnect
/// starts fresh (the counter is scoped to the slot lifetime).
pub struct HandshakeAttemptPolicy {
    max_attempts: u8,
}

impl HandshakeAttemptPolicy {
    /// `max_attempts` = `HandshakeAttemptPolicyOptions.MaxAttempts` (upstream default 2).
    pub fn new(max_attempts: u8) -> Self {
        Self { max_attempts }
    }

    pub fn is_enabled(&self) -> bool {
        self.max_attempts > 0
    }

    /// Records an attempt against `attempts` (the peer's current counter). Returns
    /// `Some(next)` with the incremented counter if the caller should keep validating,
    /// or `None` if the peer has exceeded the budget and must be disconnected.
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

// ── PreAuthAdmission (Transport/Hardening/PreAuthAdmission.cs) ───────────────────

/// The outcome of [`PreAuthAdmission::try_admit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmitResult {
    Ok,
    IpLimitExhausted,
    BudgetExhausted,
}

/// Admission control for peers entering PENDING_AUTH. Two simultaneous caps:
/// a global budget (`PreAuthBudget`) reserving the bulk of the peer pool for
/// authenticated peers, and a per-source-IP quota (`MaxConcurrentPreAuthPerIP`)
/// so one IP can't hold a disproportionate share. Authenticated peers don't count
/// — promotion releases the slot. Counters are released on promotion or disconnect,
/// idempotently.
#[derive(Default)]
pub struct PreAuthAdmission {
    per_ip_cap: i64,
    global_budget: i64,
    per_ip_counts: std::collections::HashMap<String, i64>,
    ip_by_pending_peer: std::collections::HashMap<u32, String>,
    in_flight: i64,
}

impl PreAuthAdmission {
    /// `per_ip_cap` = `MaxConcurrentPreAuthPerIP`, `global_budget` = `PreAuthBudget`.
    /// Zero on either disables that limit (dev / load tests).
    pub fn new(per_ip_cap: i64, global_budget: i64) -> Self {
        Self {
            per_ip_cap,
            global_budget,
            ..Default::default()
        }
    }

    /// Current count of connections in PENDING_AUTH (`InFlight`).
    pub fn in_flight(&self) -> i64 {
        self.in_flight
    }

    /// Admit `peer_index` from `ip` into PENDING_AUTH, or refuse with a specific cause.
    /// Both caps are checked and the commit is atomic (in the single-writer model the
    /// two counters can never disagree).
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

    /// Release on PENDING_AUTH → AUTHENTICATED (`ReleaseOnPromotion`).
    pub fn release_on_promotion(&mut self, peer_index: u32) {
        self.release_internal(peer_index);
    }

    /// Release on the peer's Disconnected event (`ReleaseOnDisconnect`). Idempotent
    /// w.r.t. promotion: if the peer already authenticated the lookup misses.
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

/// The reason a [`PreAuthAdmission`] refusal maps to (`TryAdmitOrRefuse`).
pub fn pre_auth_refusal_reason(result: AdmitResult) -> Option<DisconnectReason> {
    match result {
        AdmitResult::Ok => None,
        AdmitResult::IpLimitExhausted => Some(DisconnectReason::PreAuthIpLimitExhausted),
        AdmitResult::BudgetExhausted => Some(DisconnectReason::PreAuthBudgetExhausted),
    }
}

// ── Defaults (appsettings.json) ─────────────────────────────────────────────────

/// Upstream `Transport.Hardening.PreAuth.PreAuthBudget`.
pub const DEFAULT_PRE_AUTH_BUDGET: i64 = 512;
/// Upstream `Transport.Hardening.PreAuth.MaxConcurrentPreAuthPerIP`.
pub const DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP: i64 = 32;
/// Upstream `Messaging.Hardening.Handshake.MaxAttempts`.
pub const DEFAULT_MAX_HANDSHAKE_ATTEMPTS: u8 = 2;
/// Upstream `Messaging.Hardening.FieldValidator.MaxEmoteIdLength`.
pub const DEFAULT_MAX_EMOTE_ID_LENGTH: usize = 512;
/// Upstream `Messaging.Hardening.FieldValidator.MaxEmoteDurationMs`.
pub const DEFAULT_MAX_EMOTE_DURATION_MS: u32 = 60_000;
/// Upstream `Transport.Hardening.CorruptedPacket.MaxPerMinute`.
pub const DEFAULT_CORRUPT_MAX_PER_MINUTE: u32 = 5;
/// Upstream `Transport.Hardening.CorruptedPacket.BurstCapacity`.
pub const DEFAULT_CORRUPT_BURST: u32 = 5;

/// Per-peer token bucket tolerating a small rate of corrupt packets before
/// terminating the session — port of `Transport/Hardening/CorruptedPacketLimiter.cs`.
/// Counts oversized packets + protobuf parse failures; exhausting the budget signals a
/// [`DisconnectReason::PacketCorrupted`] disconnect. Driven on the single server loop,
/// so plain `&mut` (no per-shard lock).
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
    /// `max_per_minute` is the sustained rate (0 disables); `burst_capacity` clamped to a
    /// byte. `refill_interval_ms = 60000 / max_per_minute`.
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

    /// Debit one token for `peer`; returns `true` when the budget is exhausted and the caller
    /// should disconnect with `PacketCorrupted`.
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
            // First corruption for this peer — start full. Clamp 0 to 1 so the sentinel
            // doesn't collide with a real monotonic reading of 0.
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

    /// Drop the peer's bucket on disconnect (bounds the map). Idempotent.
    pub fn release(&mut self, peer: u32) {
        self.peer_buckets.remove(&peer);
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::CorruptedPacketLimiter;

    #[test]
    fn tolerates_burst_then_exhausts() {
        let mut l = CorruptedPacketLimiter::new(5, 5); // burst 5, refill every 12000ms
        assert!(l.is_enabled());
        for _ in 0..5 {
            assert!(!l.register_and_check_exhausted(1, 1000));
        }
        assert!(l.register_and_check_exhausted(1, 1000)); // 6th exhausts
    }

    #[test]
    fn refills_over_time() {
        let mut l = CorruptedPacketLimiter::new(5, 5);
        for _ in 0..6 {
            l.register_and_check_exhausted(1, 1000);
        }
        // one refill interval later -> exactly one more token, then exhausted again
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

    // ── BanList ─────────────────────────────────────────────────────────────

    #[test]
    fn ban_list_normalizes_prefix_and_case() {
        let mut bans = BanList::new();
        bans.replace(["0xABC", "DEF"]); // mixed case + missing prefix
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
        // 0x1 already present, 0x2 is new.
        let newly = bans.replace(["0x1", "0x2"]);
        assert_eq!(newly, vec!["0x2".to_string()]);
        // Both remain banned after the replace that included them.
        assert!(bans.is_banned("0x1"));
        assert!(bans.is_banned("0x2"));
        // A replace that omits 0x1 drops it from the blocklist.
        let newly = bans.replace(["0x2"]);
        assert!(
            newly.is_empty(),
            "0x2 was already banned, nothing newly added"
        );
        assert!(!bans.is_banned("0x1"));
        assert!(bans.is_banned("0x2"));
    }

    // ── HandshakeReplayPolicy ───────────────────────────────────────────────

    #[test]
    fn replay_first_admit_succeeds_then_dup_rejected() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 4096);
        assert!(p.try_admit(1000, "0xabc", "1700000000000"));
        // Same pair within the window -> rejected.
        assert!(!p.try_admit(1000, "0xabc", "1700000000000"));
        // Case-flipped wallet must NOT bypass the cache.
        assert!(!p.try_admit(1000, "0xABC", "1700000000000"));
    }

    #[test]
    fn replay_same_pair_after_ttl_accepted() {
        let mut p = HandshakeReplayPolicy::new(true, 30_000, 4096);
        assert!(p.try_admit(1000, "0xabc", "ts"));
        // Advance past the TTL window.
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
        // Cache full; a fresh third pair must still be admitted (fail-open on memory).
        assert!(p.try_admit(1000, "0x3", "t3"));
    }

    // ── HandshakeAttemptPolicy ──────────────────────────────────────────────

    #[test]
    fn attempt_throttle_rejects_after_max() {
        let p = HandshakeAttemptPolicy::new(2);
        // attempt 0 -> ok (next=1), 1 -> ok (next=2), 2 -> reject.
        assert_eq!(p.try_record_attempt(0), Some(1));
        assert_eq!(p.try_record_attempt(1), Some(2));
        assert_eq!(p.try_record_attempt(2), None);
    }

    #[test]
    fn attempt_throttle_disabled_when_max_zero() {
        let p = HandshakeAttemptPolicy::new(0);
        assert_eq!(p.try_record_attempt(255), Some(255));
    }

    // ── PreAuthAdmission ────────────────────────────────────────────────────

    #[test]
    fn pre_auth_budget_caps_global_in_flight() {
        let mut a = PreAuthAdmission::new(0, 2); // per-ip disabled, global budget 2
        assert_eq!(a.try_admit(1, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(2, "2.2.2.2"), AdmitResult::Ok);
        assert_eq!(a.try_admit(3, "3.3.3.3"), AdmitResult::BudgetExhausted);
        assert_eq!(a.in_flight(), 2);
        // Promotion frees a slot.
        a.release_on_promotion(1);
        assert_eq!(a.in_flight(), 1);
        assert_eq!(a.try_admit(3, "3.3.3.3"), AdmitResult::Ok);
    }

    #[test]
    fn pre_auth_per_ip_cap_isolates_one_ip() {
        let mut a = PreAuthAdmission::new(2, 0); // per-ip 2, global disabled
        assert_eq!(a.try_admit(1, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(2, "1.1.1.1"), AdmitResult::Ok);
        assert_eq!(a.try_admit(3, "1.1.1.1"), AdmitResult::IpLimitExhausted);
        // A different IP is unaffected.
        assert_eq!(a.try_admit(4, "2.2.2.2"), AdmitResult::Ok);
        // Releasing one of the squatting IP's slots frees room.
        a.release_on_disconnect(1);
        assert_eq!(a.try_admit(3, "1.1.1.1"), AdmitResult::Ok);
    }

    #[test]
    fn pre_auth_release_is_idempotent() {
        let mut a = PreAuthAdmission::new(0, 4);
        a.try_admit(1, "1.1.1.1");
        a.release_on_promotion(1);
        // A second release (disconnect after promotion) is a no-op.
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
        // Spot-check the verbatim integer values carried on the wire.
        assert_eq!(DisconnectReason::AuthTimeout.code(), 2);
        assert_eq!(DisconnectReason::DuplicateSession.code(), 4);
        assert_eq!(DisconnectReason::Banned.code(), 5);
        assert_eq!(DisconnectReason::HandshakeReplayRejected.code(), 14);
        assert_eq!(DisconnectReason::InvalidEmoteField.code(), 12);
    }
}
