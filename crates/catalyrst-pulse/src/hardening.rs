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

    InvalidSceneListenerField = 19,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayRejection {
    Duplicate,
    Forgotten,
    Capacity,
    FutureDated,
    Storage,
}

type ReplayEntries = std::collections::HashMap<(String, String), i64>;

struct ReplayJournal {
    path: std::path::PathBuf,
    file: std::fs::File,
    _lock: std::fs::File,
    records: usize,
}

impl ReplayJournal {
    #[cfg(unix)]
    fn sync_parent(path: &std::path::Path) -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::File::open(parent)?.sync_all()
    }

    #[cfg(not(unix))]
    fn sync_parent(_path: &std::path::Path) -> std::io::Result<()> {
        Ok(())
    }

    fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<(Self, ReplayEntries, i64)> {
        use std::io::BufRead as _;

        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let lock_path = path.with_extension("replay-journal.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        lock.try_lock()?;
        let readable = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;
        Self::sync_parent(&path)?;
        let mut seen = std::collections::HashMap::new();
        let mut forgotten_through_ms = i64::MIN;
        let mut records = 0usize;
        for line in std::io::BufReader::new(&readable).lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            records = records.saturating_add(1);
            let fields: Vec<_> = line.split('\t').collect();
            match fields.as_slice() {
                ["F", through] => {
                    let through = through.parse::<i64>().map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "invalid replay-journal floor",
                        )
                    })?;
                    forgotten_through_ms = forgotten_through_ms.max(through);
                }
                ["S", signed_ms, wallet, timestamp]
                    if !wallet.is_empty()
                        && !wallet.contains(char::is_whitespace)
                        && !timestamp.is_empty()
                        && !timestamp.contains(char::is_whitespace) =>
                {
                    let signed_ms = signed_ms.parse::<i64>().map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "invalid replay-journal timestamp",
                        )
                    })?;
                    if timestamp.parse::<i64>().ok() != Some(signed_ms) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "inconsistent replay-journal timestamp",
                        ));
                    }
                    seen.insert((wallet.to_string(), timestamp.to_string()), signed_ms);
                }
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid replay-journal record",
                    ));
                }
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok((
            Self {
                path,
                file,
                _lock: lock,
                records,
            },
            seen,
            forgotten_through_ms,
        ))
    }

    fn append_seen(
        &mut self,
        wallet: &str,
        timestamp: &str,
        signed_ms: i64,
    ) -> std::io::Result<()> {
        use std::io::Write as _;

        writeln!(self.file, "S\t{signed_ms}\t{wallet}\t{timestamp}")?;
        self.file.sync_data()?;
        self.records = self.records.saturating_add(1);
        Ok(())
    }

    fn append_floor(&mut self, through: i64) -> std::io::Result<()> {
        use std::io::Write as _;

        writeln!(self.file, "F\t{through}")?;
        self.file.sync_data()?;
        self.records = self.records.saturating_add(1);
        Ok(())
    }

    fn compact(
        &mut self,
        seen: &std::collections::HashMap<(String, String), i64>,
        forgotten_through_ms: i64,
    ) -> std::io::Result<()> {
        use std::io::Write as _;

        let temporary = self.path.with_extension("replay-journal.tmp");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        writeln!(file, "F\t{forgotten_through_ms}")?;
        let mut entries: Vec<_> = seen.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for ((wallet, timestamp), signed_ms) in entries {
            writeln!(file, "S\t{signed_ms}\t{wallet}\t{timestamp}")?;
        }
        file.sync_all()?;
        std::fs::rename(&temporary, &self.path)?;
        Self::sync_parent(&self.path)?;
        self.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.records = seen.len().saturating_add(1);
        Ok(())
    }
}

/// Admits each `(wallet, signed timestamp)` at most once. The production constructor journals a
/// valid pair before answering, including a clock-ahead pair that is refused for now, so restart
/// cannot turn observed signed bytes into a first use. An entry is dropped only after the wall
/// clock has left its freshness window, and every dropped timestamp raises
/// `forgotten_through_ms`, at or below which nothing is admitted. A stepped-back wall clock
/// therefore cannot make a dropped timestamp fresh again, and `forget_before` puts everything
/// signed before this process started under the same floor.
pub struct HandshakeReplayPolicy {
    enabled: bool,
    skew_ms: i64,
    max_entries: usize,
    forgotten_through_ms: i64,
    seen: ReplayEntries,
    journal: Option<ReplayJournal>,
    storage_failed: bool,
}

impl HandshakeReplayPolicy {
    pub fn new(enabled: bool, skew_ms: i64, max_entries: usize) -> Self {
        Self {
            enabled,
            skew_ms,
            max_entries,
            forgotten_through_ms: i64::MIN,
            seen: std::collections::HashMap::new(),
            journal: None,
            storage_failed: false,
        }
    }

    pub fn durable(
        enabled: bool,
        skew_ms: i64,
        max_entries: usize,
        path: impl AsRef<std::path::Path>,
        now_ms: i64,
    ) -> std::io::Result<Self> {
        let (journal, seen, forgotten_through_ms) = ReplayJournal::open(path)?;
        let mut policy = Self {
            enabled,
            skew_ms,
            max_entries,
            forgotten_through_ms,
            seen,
            journal: Some(journal),
            storage_failed: false,
        };
        policy.sweep_expired_in_memory(now_ms);
        if policy.seen.len() > max_entries {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replay journal exceeds configured capacity",
            ));
        }
        policy.compact_journal()?;
        Ok(policy)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn forget_before(&mut self, wall_ms: i64) {
        let next = self.forgotten_through_ms.max(wall_ms.saturating_sub(1));
        if next == self.forgotten_through_ms {
            return;
        }
        self.forgotten_through_ms = next;
        if let Some(journal) = self.journal.as_mut() {
            if journal.append_floor(next).is_err() {
                self.storage_failed = true;
                tracing::error!("Pulse replay journal write failed; admission is closed");
            }
        }
    }

    pub fn try_admit(
        &mut self,
        now_ms: i64,
        wallet: &str,
        timestamp: &str,
    ) -> Result<(), ReplayRejection> {
        if !self.enabled {
            return Ok(());
        }
        if self.storage_failed {
            return Err(ReplayRejection::Storage);
        }
        let signed_ms = match timestamp.parse::<i64>() {
            Ok(signed_ms) if signed_ms > self.forgotten_through_ms => signed_ms,
            _ => return Err(ReplayRejection::Forgotten),
        };
        let key = (wallet.to_lowercase(), timestamp.to_string());
        if self.seen.contains_key(&key) {
            return Err(ReplayRejection::Duplicate);
        }

        if self.seen.len() >= self.max_entries / 2 {
            self.sweep_expired(now_ms)?;
        }

        if self.seen.len() >= self.max_entries {
            return Err(ReplayRejection::Capacity);
        }

        if let Some(journal) = self.journal.as_mut() {
            if journal.append_seen(&key.0, &key.1, signed_ms).is_err() {
                self.storage_failed = true;
                tracing::error!("Pulse replay journal write failed; admission is closed");
                return Err(ReplayRejection::Storage);
            }
        }
        self.seen.insert(key, signed_ms);
        if self
            .journal
            .as_ref()
            .is_some_and(|journal| journal.records > self.max_entries.saturating_mul(4).max(4))
            && self.compact_journal().is_err()
        {
            self.storage_failed = true;
            tracing::error!("Pulse replay journal compaction failed; admission is closed");
            return Err(ReplayRejection::Storage);
        }
        if signed_ms > now_ms {
            Err(ReplayRejection::FutureDated)
        } else {
            Ok(())
        }
    }

    fn sweep_expired(&mut self, now_ms: i64) -> Result<(), ReplayRejection> {
        let old_floor = self.forgotten_through_ms;
        self.sweep_expired_in_memory(now_ms);
        if self.forgotten_through_ms != old_floor {
            if let Some(journal) = self.journal.as_mut() {
                if journal.append_floor(self.forgotten_through_ms).is_err() {
                    self.storage_failed = true;
                    tracing::error!("Pulse replay journal write failed; admission is closed");
                    return Err(ReplayRejection::Storage);
                }
            }
        }
        Ok(())
    }

    fn sweep_expired_in_memory(&mut self, now_ms: i64) {
        let oldest_fresh_ms = now_ms.saturating_sub(self.skew_ms);
        let mut forgotten_through_ms = self.forgotten_through_ms;
        self.seen.retain(|_, &mut signed_ms| {
            if signed_ms < oldest_fresh_ms {
                forgotten_through_ms = forgotten_through_ms.max(signed_ms);
            }
            signed_ms >= oldest_fresh_ms
        });
        self.forgotten_through_ms = forgotten_through_ms;
    }

    fn compact_journal(&mut self) -> std::io::Result<()> {
        if let Some(journal) = self.journal.as_mut() {
            journal.compact(&self.seen, self.forgotten_through_ms)?;
        }
        Ok(())
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

pub const DEFAULT_PRE_AUTH_BUDGET_WT: i64 = 512;

pub const DEFAULT_MAX_CONCURRENT_PRE_AUTH_PER_IP: i64 = 32;

pub const DEFAULT_MAX_HANDSHAKE_ATTEMPTS: u8 = 2;

pub const DEFAULT_MAX_EMOTE_ID_LENGTH: usize = 512;

pub const DEFAULT_MAX_EMOTE_DURATION_MS: u32 = 60_000;

pub const DEFAULT_MAX_REALM_LENGTH: usize = 255;

pub const DEFAULT_SCENE_LISTENER_MAX_PARCELS: usize = 4096;

pub const SCENE_LISTENER_REALM_BUDGET_COST: usize = 4;

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
                    b.tokens = (b.tokens as u32).saturating_add(refills).min(cap as u32) as u8;
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

pub const DEFAULT_INPUT_MAX_HZ: u32 = 20;

pub const DEFAULT_INPUT_BURST: u32 = 16;

pub const DEFAULT_DISCRETE_RATE_PER_SEC: u32 = 20;

pub const DEFAULT_DISCRETE_BURST: u32 = 16;

#[derive(Clone, Copy, Default)]
struct GameplayBucket {
    tokens: u8,
    last_refill_ms: u32,
}

struct BucketParams {
    burst: u8,
    refill_interval_ms: u32,
}

impl BucketParams {
    fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            burst: burst.min(u8::MAX as u32) as u8,
            refill_interval_ms: 1000u32
                .checked_div(rate_per_sec)
                .map(|v| v.max(1))
                .unwrap_or(0),
        }
    }

    fn is_enabled(&self) -> bool {
        self.refill_interval_ms > 0 && self.burst > 0
    }
}

#[derive(Clone, Copy, Default)]
struct PeerBuckets {
    input: GameplayBucket,
    discrete: GameplayBucket,
}

pub struct GameplayRateLimiter {
    input: BucketParams,
    discrete: BucketParams,
    peers: std::collections::HashMap<u32, PeerBuckets>,
}

impl GameplayRateLimiter {
    pub fn new(
        input_max_hz: u32,
        input_burst: u32,
        discrete_rate_per_sec: u32,
        discrete_burst: u32,
    ) -> Self {
        Self {
            input: BucketParams::new(input_max_hz, input_burst),
            discrete: BucketParams::new(discrete_rate_per_sec, discrete_burst),
            peers: std::collections::HashMap::new(),
        }
    }

    pub fn try_accept_input(&mut self, peer: u32, now_ms: u32) -> bool {
        if !self.input.is_enabled() {
            return true;
        }
        let bucket = &mut self.peers.entry(peer).or_default().input;
        Self::charge(bucket, &self.input, now_ms)
    }

    pub fn try_accept_discrete(&mut self, peer: u32, now_ms: u32) -> bool {
        if !self.discrete.is_enabled() {
            return true;
        }
        let bucket = &mut self.peers.entry(peer).or_default().discrete;
        Self::charge(bucket, &self.discrete, now_ms)
    }

    pub fn release(&mut self, peer: u32) {
        self.peers.remove(&peer);
    }

    fn charge(bucket: &mut GameplayBucket, p: &BucketParams, now_ms: u32) -> bool {
        let cap = p.burst;
        let interval = p.refill_interval_ms;
        if bucket.last_refill_ms == 0 {
            *bucket = GameplayBucket {
                tokens: cap,
                last_refill_ms: if now_ms == 0 { 1 } else { now_ms },
            };
        } else {
            let refills = now_ms.wrapping_sub(bucket.last_refill_ms) / interval;
            if refills > 0 {
                bucket.tokens = (bucket.tokens as u32)
                    .saturating_add(refills)
                    .min(cap as u32) as u8;
                bucket.last_refill_ms = bucket.last_refill_ms.wrapping_add(refills * interval);
            }
        }
        if bucket.tokens == 0 {
            return false;
        }
        bucket.tokens -= 1;
        true
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::{
        CorruptedPacketLimiter, GameplayRateLimiter, DEFAULT_DISCRETE_BURST,
        DEFAULT_DISCRETE_RATE_PER_SEC, DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ,
    };

    #[test]
    fn gameplay_defaults_track_upstream_appsettings() {
        assert_eq!(
            (DEFAULT_INPUT_MAX_HZ, DEFAULT_INPUT_BURST),
            (20, 16),
            "upstream appsettings Messaging:Hardening:MovementInput MaxHz / BurstCapacity"
        );
        assert_eq!(
            (DEFAULT_DISCRETE_RATE_PER_SEC, DEFAULT_DISCRETE_BURST),
            (20, 16),
            "upstream appsettings Messaging:Hardening:DiscreteEvent RatePerSecond / \
             BurstCapacity: the registered values, not the 5/10 option-class defaults"
        );
    }

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

    #[test]
    fn gameplay_release_resets_buckets() {
        let mut l = GameplayRateLimiter::new(20, 16, 5, 10);
        for _ in 0..16 {
            assert!(l.try_accept_input(1, 1000));
        }
        assert!(!l.try_accept_input(1, 1000));
        l.release(1);
        assert!(l.try_accept_input(1, 1000));
    }

    #[test]
    fn gameplay_disabled_passes_everything() {
        let mut l = GameplayRateLimiter::new(0, 16, 0, 10);
        for _ in 0..100 {
            assert!(l.try_accept_input(1, 0));
            assert!(l.try_accept_discrete(1, 0));
        }
    }

    #[test]
    fn gameplay_refills_over_time() {
        let mut l = GameplayRateLimiter::new(20, 16, 5, 10);
        for _ in 0..16 {
            assert!(l.try_accept_input(1, 1000));
        }
        assert!(!l.try_accept_input(1, 1000));
        assert!(l.try_accept_input(1, 1050));
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

    const SKEW: i64 = crate::handshake::MAX_TIMESTAMP_SKEW_MS;
    const SIGNED: i64 = 1_700_000_000_000;

    struct ReplayPath(std::path::PathBuf);

    impl ReplayPath {
        fn new(label: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "catalyrst-pulse-{label}-{}-{id}.tsv",
                std::process::id()
            )))
        }
    }

    impl Drop for ReplayPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("replay-journal.tmp"));
            let _ = std::fs::remove_file(self.0.with_extension("replay-journal.lock"));
        }
    }

    #[test]
    fn replay_first_admit_succeeds_then_dup_rejected() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 4096);
        assert_eq!(p.try_admit(SIGNED, "0xabc", "1700000000000"), Ok(()));

        assert_eq!(
            p.try_admit(SIGNED, "0xabc", "1700000000000"),
            Err(ReplayRejection::Duplicate)
        );

        assert_eq!(
            p.try_admit(SIGNED, "0xABC", "1700000000000"),
            Err(ReplayRejection::Duplicate)
        );
    }

    #[test]
    fn replay_same_pair_is_never_admitted_again_after_its_entry_is_dropped() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 2);
        let used = SIGNED.to_string();
        assert_eq!(p.try_admit(SIGNED + SKEW, "0xabc", &used), Ok(()));

        let later = SIGNED + 2 * SKEW + 1;
        assert_eq!(p.try_admit(later, "0xdef", &later.to_string()), Ok(()));
        assert_eq!(p.seen.len(), 1, "the expired entry was dropped");
        for stepped_back in [later, SIGNED + SKEW, SIGNED, SIGNED - SKEW] {
            assert_eq!(
                p.try_admit(stepped_back, "0xabc", &used),
                Err(ReplayRejection::Forgotten)
            );
        }
    }

    #[test]
    fn replay_floor_never_reaches_a_timestamp_inside_the_freshness_window() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 2);
        let future_dated = (SIGNED + SKEW).to_string();
        assert_eq!(
            p.try_admit(SIGNED, "0xabc", &future_dated),
            Err(ReplayRejection::FutureDated)
        );
        let later = SIGNED + 2 * SKEW + 1;
        assert_eq!(p.try_admit(later, "0x1", &later.to_string()), Ok(()));
        assert_eq!(p.seen.len(), 1);
        let slowest_fresh = (later - SKEW).to_string();
        assert_eq!(p.try_admit(later, "0x2", &slowest_fresh), Ok(()));
    }

    #[test]
    fn replay_forget_before_refuses_everything_signed_before_the_process_started() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 4096);
        p.forget_before(SIGNED);
        assert_eq!(
            p.try_admit(SIGNED, "0xabc", &(SIGNED - 1).to_string()),
            Err(ReplayRejection::Forgotten)
        );
        assert_eq!(p.try_admit(SIGNED, "0xabc", &SIGNED.to_string()), Ok(()));
        p.forget_before(SIGNED - SKEW);
        assert_eq!(
            p.try_admit(SIGNED, "0xdef", &(SIGNED - 1).to_string()),
            Err(ReplayRejection::Forgotten),
            "the floor never moves back"
        );
        assert_eq!(
            p.try_admit(SIGNED, "0xdef", "not-a-number"),
            Err(ReplayRejection::Forgotten)
        );
    }

    #[test]
    fn replay_different_timestamp_or_wallet_accepted() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 4096);
        assert_eq!(p.try_admit(SIGNED + 2, "0xabc", "1700000000001"), Ok(()));
        assert!(
            p.try_admit(SIGNED + 2, "0xabc", "1700000000002").is_ok(),
            "fresh timestamp = legit reconnect"
        );
        assert!(
            p.try_admit(SIGNED + 2, "0xdef", "1700000000001").is_ok(),
            "different wallet, same ts"
        );
    }

    #[test]
    fn replay_disabled_admits_everything() {
        let mut p = HandshakeReplayPolicy::new(false, SKEW, 4096);
        for _ in 0..100 {
            assert_eq!(p.try_admit(SIGNED, "0xabc", "ts"), Ok(()));
        }
    }

    #[test]
    fn replay_capacity_rejects_new_admission_without_forgetting_live_entries() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 2);
        assert_eq!(p.try_admit(SIGNED + 3, "0x1", "1700000000001"), Ok(()));
        assert_eq!(p.try_admit(SIGNED + 3, "0x2", "1700000000002"), Ok(()));
        assert_eq!(
            p.try_admit(SIGNED + 3, "0x3", "1700000000003"),
            Err(ReplayRejection::Capacity)
        );
        assert_eq!(
            p.try_admit(SIGNED + 1, "0x1", "1700000000001"),
            Err(ReplayRejection::Duplicate)
        );
        let later = SIGNED + SKEW + 3;
        assert_eq!(p.try_admit(later, "0x3", &later.to_string()), Ok(()));
        assert_eq!(p.seen.len(), 1);
    }

    #[test]
    fn replay_retention_includes_entire_freshness_window() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 1);
        let future_dated = (SIGNED + SKEW).to_string();
        assert_eq!(
            p.try_admit(SIGNED, "wallet", &future_dated),
            Err(ReplayRejection::FutureDated)
        );
        for delta in [7_000, 31_000, 60_000, 120_000] {
            assert_eq!(
                p.try_admit(SIGNED + delta, "wallet", &future_dated),
                Err(ReplayRejection::Duplicate)
            );
            assert_eq!(
                p.try_admit(SIGNED + delta, "other", &(SIGNED + delta).to_string()),
                Err(ReplayRejection::Capacity)
            );
        }
        let later = SIGNED + 2 * SKEW + 1;
        assert_eq!(p.try_admit(later, "other", &later.to_string()), Ok(()));
    }

    #[test]
    fn durable_replay_journal_consumes_future_dated_bytes_across_restart() {
        let path = ReplayPath::new("future-restart");
        let future = SIGNED + SKEW;
        {
            let mut first = HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED).unwrap();
            assert_eq!(
                first.try_admit(SIGNED, "0xabc", &future.to_string()),
                Err(ReplayRejection::FutureDated)
            );
        }
        let mut restarted =
            HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED + 1).unwrap();
        restarted.forget_before(SIGNED + 1);
        assert_eq!(
            restarted.try_admit(future, "0xabc", &future.to_string()),
            Err(ReplayRejection::Duplicate)
        );
    }

    #[test]
    fn malformed_replay_journal_fails_startup_closed() {
        let path = ReplayPath::new("malformed");
        std::fs::write(&path.0, b"not a replay record\n").unwrap();
        assert!(HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED).is_err());
    }

    #[test]
    fn durable_replay_journal_has_exactly_one_writer() {
        let path = ReplayPath::new("single-writer");
        let first = HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED).unwrap();
        assert!(
            HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED).is_err(),
            "two processes must not race appends or compaction through one journal path"
        );
        drop(first);
        assert!(
            HandshakeReplayPolicy::durable(true, SKEW, 4, &path.0, SIGNED).is_ok(),
            "the next process must acquire the journal after the owner exits"
        );
    }

    #[test]
    fn zero_capacity_fails_closed() {
        let mut p = HandshakeReplayPolicy::new(true, SKEW, 0);
        assert_eq!(
            p.try_admit(SIGNED, "wallet", "1700000000000"),
            Err(ReplayRejection::Capacity)
        );
        assert!(p.seen.is_empty());
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
        assert_eq!(DisconnectReason::InvalidHandshakeField.code(), 15);
        assert_eq!(DisconnectReason::InvalidSceneListenerField.code(), 19);
    }
}
