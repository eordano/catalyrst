//! The outbound pull.
//!
//! # Why there is no `preverify` analogue here
//!
//! `catalyrst-social-service/src/rest/fed/consumer.rs::preverify` exists because in
//! **push** a peer hands us an envelope we did not ask for, so the peer's bytes are the
//! only thing establishing who is talking. In **pull** we choose the URL, we open the
//! connection, and `AdmittedPeer` pinned the root -- identity is established by the
//! transport before the first byte of payload. Mapping the five gates, so a reviewer
//! can check rather than take this on faith:
//!
//! 1. **signer recovery -> the pinned TLS root.** Reproduced, and strictly earlier in
//!    the sequence than the original. [`crate::fed::peers::AdmittedPeer`] owns it.
//! 2. **skew window on a signed timestamp.** Retired: no peer-supplied time is
//!    load-bearing. `observed_at` is our clock; `last_deployed_at` is stored metadata
//!    that no branch reads.
//! 3. **domain-name check.** Reproduced structurally: `worlds_listing_url` is the only
//!    URL constructor and takes no peer input, so there is no envelope carrying a scope
//!    to confuse. No `Scope::Worlds` is added -- a domain constant nothing signs against
//!    is the orphaned-config defect this branch exists to end.
//! 4. **nonce replay cache.** Retired: we ask, so a peer cannot make us re-apply. Each
//!    poll replaces exactly one peer's rows in one transaction, so re-serving the same
//!    listing is idempotent by construction and there is no accumulating log to replay
//!    into.
//! 5. **inbound rate limit.** Reproduced, inverted: a fixed poll interval, one
//!    in-flight poll per peer, a byte cap applied *before* parse, a row cap, and
//!    connect/total timeouts on the pinned client. A peer cannot consume our resources
//!    on its own schedule, only the budget we spend on it.
//!
//! # What is not claimed
//!
//! The pinned root authenticates the *host*, not the *content*. A compromised admitted
//! peer can lie about which worlds it holds. That lie is contained by namespacing -- the
//! rows live under `(peer_id, world_name)`, are served only under `peerId`, and grant
//! nothing. A peer's claims about content are contained by namespacing; a peer's claims
//! about **authority** are contained by never modelling them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use reqwest::Url;
use tokio::sync::Mutex;

use crate::fed::config::WorldsFedConfig;
use crate::fed::names::{PeerId, RemoteWorldName};
use crate::fed::peers::{AdmittedPeer, WorldsFederationPeers};
use crate::fed::store::{LocalNameCollisionProbe, RemoteWorld, RemoteWorldsComponent};
use crate::fed::wire::intake_page;

/// Entries requested per page. Independent of `WORLDS_FED_MAX_WORLDS_PER_PEER`, which
/// caps stored rows: a peer is free to ignore the parameter, and the loop terminates on
/// what arrived rather than on what we asked for.
const PAGE_SIZE: i64 = 500;

/// A peer that ignores `offset` and re-serves page one forever would otherwise spin
/// until the row cap. Two pages of slack over the cap, then stop.
fn max_pages(max_worlds: u64) -> usize {
    (max_worlds as usize).div_ceil(PAGE_SIZE as usize) + 2
}

/// Why a poll did not happen. Every variant leaves the peer's previously mirrored rows
/// exactly as they were: a failure makes the mirror **stale**, never **empty**.
#[derive(Debug)]
pub enum PollFailure {
    /// TLS handshake against the pinned root, DNS, connect, or timeout.
    Transport(String),
    /// Anything that is not 2xx. A redirect lands here too: the pinned client refuses
    /// to follow one, because a redirect off the pinned host silently defeats the pin.
    Status(u16),
    /// The peer answered something that is not JSON. Checked before the body is read.
    NotJson(String),
    /// The body exceeded `WORLDS_FED_MAX_RESPONSE_BYTES`. Detected while streaming, so
    /// the oversized body is never fully buffered and never parsed.
    BodyTooLarge(u64),
    /// The body was JSON but not a worlds listing.
    Malformed(String),
    /// Our own database. Recorded as a peer-poll failure because that is what a caller
    /// observes, but it is our fault, and the peer's rows still survive the rollback.
    Store(String),
}

/// What [`PollFailure::Store`] publishes in place of the database's own words.
///
/// A named constant so the redaction is matchable in a test and greppable from an
/// operator's console, rather than a literal repeated in two files.
pub const STORE_FAULT_PUBLIC_REASON: &str =
    "mirror store: our own database did not accept the write; see this server's logs";

impl std::fmt::Display for PollFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::Status(s) => write!(f, "peer answered HTTP {s}"),
            Self::NotJson(ct) => write!(f, "peer answered content-type {ct:?}, expected JSON"),
            Self::BodyTooLarge(n) => {
                write!(f, "peer body exceeded {n} bytes; refused before parse")
            }
            Self::Malformed(e) => write!(f, "peer body is not a worlds listing: {e}"),
            Self::Store(e) => write!(f, "mirror store: {e}"),
        }
    }
}

impl PollFailure {
    /// The text safe to write into `remote_peer_status.last_error`, which
    /// `GET /federation/worlds/peers` serves to **unauthenticated** callers.
    ///
    /// Identical to [`Display`](std::fmt::Display) for every variant whose text is
    /// about the *peer* -- a transport error naming their host, an HTTP status they
    /// returned, a serde message about a body they sent. Those are facts about the
    /// other end of a public federation link, and hiding them would cost an operator
    /// on either side the ability to diagnose it.
    ///
    /// [`Self::Store`] is the exception, and the only one: its string is a `sqlx`
    /// error about *our* database -- observed in the wild as `relation "worlds" does
    /// not exist at line 1469`, a local table name and a source line number served to
    /// anyone who asked. The caller had nothing to do with that fault and learns
    /// nothing useful from its text. The **fact** still publishes; the words do not.
    ///
    /// The verbatim string is not discarded -- every call site logs `self` at the same
    /// moment it stores this.
    pub fn published(&self) -> String {
        match self {
            Self::Store(_) => STORE_FAULT_PUBLIC_REASON.to_string(),
            other => other.to_string(),
        }
    }
}

/// What the local-name collision probe established -- or that it established nothing.
///
/// The probe reads local rows and decides nothing, so its failure must not fail a poll.
/// What its failure must also not do is *render as a reading*: a bare `Vec<String>`
/// cannot hold the difference between "we asked, and no local world shares a name with
/// this peer's" and "we could not ask", and the empty vec a failed probe used to return
/// was the same value a clean run produces. Every layer above -- the report, the stored
/// status row, the operator's JSON -- then repeated the fabrication, because there was
/// nothing left in the value to repeat anything else from.
///
/// This enum keeps the two apart at the point of measurement, which is the only place
/// they are still distinguishable, so no caller downstream has to remember to.
#[derive(Debug, Clone)]
pub enum LocalNameCollisions {
    /// The probe ran to completion. `Checked(vec![])` is knowledge of an absence.
    Checked(Vec<String>),
    /// The probe did not answer, and nothing about collisions is known for this poll.
    /// Carries the reason, which reaches the operator verbatim.
    Unavailable(String),
}

impl LocalNameCollisions {
    /// The colliding local names, or `None` when the probe could not answer.
    ///
    /// There is deliberately no accessor that yields a `Vec` in both cases: a caller
    /// that wants a list has to say what it is doing about not having one, and
    /// `unwrap_or_default` at a call site is exactly the defect this type exists to
    /// stop from coming back.
    pub fn checked(&self) -> Option<&[String]> {
        match self {
            Self::Checked(hits) => Some(hits),
            Self::Unavailable(_) => None,
        }
    }

    /// Why the probe could not answer. `Some` exactly when [`Self::checked`] is `None`.
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Checked(_) => None,
            Self::Unavailable(e) => Some(e),
        }
    }
}

/// One log field's worth. `unknown (...)` rather than a number, because a count is a
/// measurement and an unavailable probe produced none -- `collisions = 0` in a log line
/// is the same lie in a smaller place.
impl std::fmt::Display for LocalNameCollisions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checked(hits) => write!(f, "{}", hits.len()),
            Self::Unavailable(e) => write!(f, "unknown ({e})"),
        }
    }
}

/// The `remote_peer_status.last_error` text written when the fetch succeeded and the
/// collision probe did not, so an operator matches a constant rather than a literal and
/// so the line says plainly which half worked.
pub const COLLISION_PROBE_UNAVAILABLE_PREFIX: &str =
    "mirror replaced; local name collision probe unavailable: ";

/// What follows [`COLLISION_PROBE_UNAVAILABLE_PREFIX`] in the stored, publicly served
/// `last_error`. The probe reads our own tables, so its error text is ours; the fact
/// that it did not run is the part a public caller has any use for.
pub const PROBE_FAULT_PUBLIC_REASON: &str = "a local query failed; see this server's logs";

/// What one successful poll did.
///
/// No `Default`: a default `PollReport` would have to invent a collision probe result,
/// and the only honest invention (`Unavailable`) would be a probe failure nobody
/// observed. Every field here is a measurement, so every value is constructed by the
/// code that made it.
#[derive(Debug, Clone)]
pub struct PollReport {
    pub worlds_observed: i64,
    /// Entries refused for their shape, plus duplicate names the peer listed twice.
    pub entries_skipped: u64,
    pub truncated: bool,
    /// Peer-reported names that also exist as **local** worlds, or the fact that we
    /// could not find out. Log material only: nothing about the mirrored row or the
    /// local row changes because of it. Local wins everywhere, structurally, and this is
    /// how an operator finds out it happened.
    pub collisions: LocalNameCollisions,
}

/// The poller and the mirror store, together, because the per-peer in-flight lock has
/// to outlive a single request and the admin refresh route needs the same lock the
/// background loop uses.
pub struct WorldsMirror {
    store: RemoteWorldsComponent,
    probe: LocalNameCollisionProbe,
    cfg: WorldsFedConfig,
    /// One lock per **admitted** peer, built once at boot. A peer id that is not in
    /// this map cannot be polled, which is the same allowlist the rest of the module
    /// enforces, expressed as a missing key.
    in_flight: HashMap<PeerId, Arc<Mutex<()>>>,
}

impl WorldsMirror {
    pub fn new(pool: sqlx::PgPool, cfg: WorldsFedConfig, peers: &WorldsFederationPeers) -> Self {
        let in_flight = peers
            .peers()
            .iter()
            .map(|p| (p.peer_id().clone(), Arc::new(Mutex::new(()))))
            .collect();
        Self {
            store: RemoteWorldsComponent::new(pool.clone()),
            probe: LocalNameCollisionProbe::over(pool),
            cfg,
            in_flight,
        }
    }

    pub fn store(&self) -> &RemoteWorldsComponent {
        &self.store
    }

    pub fn config(&self) -> &WorldsFedConfig {
        &self.cfg
    }

    /// Poll one admitted peer and replace its rows.
    ///
    /// Gate order, fail-closed at every step: an `AdmittedPeer` in hand (otherwise
    /// there is no client and no URL) -> TLS against the pinned root -> 2xx -> JSON
    /// content type -> body under the byte cap -> parses as a listing -> per-entry name
    /// admission -> row cap -> one transaction. Anything short of the end leaves the
    /// previous rows untouched.
    pub async fn poll_peer(&self, peer: &AdmittedPeer) -> Result<PollReport, PollFailure> {
        let peer_id = peer.peer_id();
        let lock = self
            .in_flight
            .get(peer_id)
            .cloned()
            // Unreachable for an `AdmittedPeer` from the boot registry; if it ever
            // happens, serialising on a fresh lock is the safe direction.
            .unwrap_or_else(|| Arc::new(Mutex::new(())));
        let _guard = lock.lock().await;

        if let Err(e) = self.store.record_attempt(peer_id).await {
            return Err(PollFailure::Store(e.to_string()));
        }

        match self.collect_and_apply(peer).await {
            Ok(report) => {
                if let Err(e) = self
                    .store
                    .record_success(
                        peer_id,
                        report.worlds_observed,
                        report.entries_skipped as i64,
                        report.truncated,
                    )
                    .await
                {
                    return Err(PollFailure::Store(e.to_string()));
                }
                // `record_success` clears `last_error`, which is right about the fetch
                // and wrong about a probe we could not run: a clean status row would put
                // the fabrication back in the first place an operator looks. Restate the
                // gap immediately after, so the row reads "the rows are current AND one
                // thing about them went unchecked" -- a fresh `last_success_at` beside a
                // `last_error`, which is a state the two fields can express and neither
                // can express alone.
                if let Some(reason) = report.collisions.unavailable_reason() {
                    // The FACT is stored and published; the probe's own words are not.
                    // `last_error` is served unauthenticated by
                    // `GET /federation/worlds/peers`, and this reason is a `sqlx` error
                    // about our own tables -- seen in the wild as `relation "worlds" does
                    // not exist at line 1469`. `PollFailure::published` draws the same
                    // line for the same field; see it for why peer-derived text stays
                    // verbatim and ours does not.
                    //
                    // Nothing is lost to an operator: the verbatim reason was already
                    // logged where the probe failed, is logged again here, and reaches
                    // the admin-only refresh route as `localNameCollisionsError`.
                    tracing::warn!(
                        peer = %peer_id,
                        reason = %reason,
                        "recording the unavailable collision probe with a redacted reason; \
                         the verbatim text is this log line and the admin refresh route"
                    );
                    let note =
                        format!("{COLLISION_PROBE_UNAVAILABLE_PREFIX}{PROBE_FAULT_PUBLIC_REASON}");
                    if let Err(e) = self.store.record_failure(peer_id, &note).await {
                        tracing::error!(
                            peer = %peer_id,
                            error = %e,
                            "could not record that the local name collision probe was \
                             unavailable; the poll stands and its collision result is \
                             unknown rather than empty in this process, but the stored \
                             status row will read as a clean success"
                        );
                    }
                }
                Ok(report)
            }
            Err(failure) => {
                // Best-effort: if we cannot even record the failure, the poll is still
                // a failure and the rows are still the old ones.
                // `published()`, not `to_string()`: this row is served unauthenticated.
                // The verbatim `failure` is logged four lines down, in the same call.
                if let Err(e) = self
                    .store
                    .record_failure(peer_id, &failure.published())
                    .await
                {
                    tracing::error!(peer = %peer_id, error = %e, "could not record peer poll failure");
                }
                tracing::warn!(
                    peer = %peer_id,
                    error = %failure,
                    "worlds mirror poll failed; the peer's previously mirrored rows are retained \
                     and reported as stale, not replaced with an empty listing"
                );
                Err(failure)
            }
        }
    }

    async fn collect_and_apply(&self, peer: &AdmittedPeer) -> Result<PollReport, PollFailure> {
        let peer_id = peer.peer_id();
        let observed_at = Utc::now();
        let row_cap = self.cfg.max_worlds_per_peer as usize;

        let mut rows: Vec<RemoteWorld> = Vec::new();
        let mut seen: HashSet<RemoteWorldName> = HashSet::new();
        let mut skipped: u64 = 0;
        let mut truncated = false;
        let mut offset: i64 = 0;

        for _page in 0..max_pages(self.cfg.max_worlds_per_peer) {
            let remaining = row_cap.saturating_sub(rows.len());
            if remaining == 0 {
                truncated = true;
                break;
            }

            let url = peer.worlds_listing_url(PAGE_SIZE, offset);
            let body = fetch_listing_bytes(peer.http(), &url, self.cfg.max_response_bytes).await?;
            let intake = intake_page(peer_id, &body, remaining, observed_at)
                .map_err(|e| PollFailure::Malformed(e.to_string()))?;

            skipped += intake.entries_skipped;
            truncated |= intake.truncated;
            let entries_seen = intake.entries_seen;

            for world in intake.worlds {
                if seen.insert(world.name.clone()) {
                    rows.push(world);
                } else {
                    // A peer listing the same name twice is malformed, and the upsert
                    // would fail the whole transaction on it. Count it, drop it.
                    skipped += 1;
                }
            }

            if truncated || entries_seen < PAGE_SIZE as usize {
                break;
            }
            offset += PAGE_SIZE;
        }

        let names: Vec<RemoteWorldName> = rows.iter().map(|r| r.name.clone()).collect();

        self.store
            .replace_peer_worlds(peer_id, &rows)
            .await
            .map_err(|e| PollFailure::Store(e.to_string()))?;

        // After the write, and deliberately: a probe failure must not fail a poll that
        // already succeeded, because the probe decides nothing. What it must not do
        // either is come back as an empty list -- see [`LocalNameCollisions`].
        let collisions = match self.probe.local_names_also_claimed(&names).await {
            Ok(hits) => LocalNameCollisions::Checked(hits),
            Err(e) => {
                tracing::warn!(
                    peer = %peer_id,
                    error = %e,
                    "local name collision probe failed; the poll stands and the peer's rows \
                     are fresh, but whether this peer publishes a name we also hold locally \
                     is UNKNOWN for this poll and is reported as unknown, never as none"
                );
                LocalNameCollisions::Unavailable(e.to_string())
            }
        };
        if let Some(hits) = collisions.checked() {
            for local in hits {
                tracing::warn!(
                    peer = %peer_id,
                    world = %local,
                    "peer publishes a world name we also hold locally; the LOCAL record wins \
                     everywhere \u{2014} /worlds, /about, comms and owner resolution all read the local \
                     tables, and the peer's row is served only under /federation/worlds/mirror \
                     qualified by peerId"
                );
            }
        }

        Ok(PollReport {
            worlds_observed: rows.len() as i64,
            entries_skipped: skipped,
            truncated,
            collisions,
        })
    }

    /// Poll every admitted peer. One peer's failure never affects another's rows.
    pub async fn poll_all(&self, peers: &WorldsFederationPeers) -> Vec<(PeerId, PollOutcome)> {
        let mut out = Vec::new();
        for peer in peers.peers() {
            let outcome = match self.poll_peer(peer).await {
                Ok(report) => PollOutcome::Polled(report),
                Err(failure) => PollOutcome::Failed(failure.to_string()),
            };
            out.push((peer.peer_id().clone(), outcome));
        }
        out
    }
}

/// A per-peer result of a refresh. `Failed` is a first-class outcome on the wire; the
/// admin route never reports a refresh as complete when a peer was unreachable.
#[derive(Debug)]
pub enum PollOutcome {
    Polled(PollReport),
    Failed(String),
}

/// Read at most `max_bytes` of a peer response, refusing before the parser sees
/// anything.
///
/// The status and content-type gates run before a single body byte is read, and the
/// byte counter aborts the stream rather than buffering an oversized body and checking
/// afterwards.
pub(crate) async fn fetch_listing_bytes(
    client: &reqwest::Client,
    url: &Url,
    max_bytes: u64,
) -> Result<Vec<u8>, PollFailure> {
    let mut resp = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| PollFailure::Transport(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(PollFailure::Status(resp.status().as_u16()));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !content_type.to_ascii_lowercase().contains("json") {
        return Err(PollFailure::NotJson(content_type));
    }

    // A peer that declares a huge Content-Length is refused before the first chunk.
    if let Some(len) = resp.content_length() {
        if len > max_bytes {
            return Err(PollFailure::BodyTooLarge(max_bytes));
        }
    }

    let mut body = Vec::new();
    loop {
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| PollFailure::Transport(e.to_string()))?;
        let Some(chunk) = chunk else { break };
        if body.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(PollFailure::BodyTooLarge(max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The background loop.
///
/// Started from `build_state` only when federation is configured *and* at least one
/// peer was admitted.
///
/// # There is no runtime reload, and that is a property of the code, not of the spec
///
/// The peer set is fixed for the process lifetime, and this is checkable rather than
/// asserted. [`WorldsFederationPeers`] is stored **by value** in `AppStateInner`, which
/// is only ever held as `Arc<AppStateInner>` -- no `Mutex`, no `RwLock`, no `ArcSwap`,
/// no `OnceCell` -- so no `&mut` to it exists after `build_state` returns and no handler
/// could mutate it if one wanted to. `load_from_env` and `load_file` have exactly one
/// production call site between them (`lib.rs`, in `build_state`), and the crate
/// installs no signal handler, so there is no SIGHUP path either.
///
/// The consequence that matters for de-admission: within one process, every write to
/// `remote_worlds` is made by a peer from this fixed set (`poll_all` iterates
/// `peers.peers()`, and `poll_peer` needs an `AdmittedPeer` to have a client at all),
/// and every read is filtered against the same fixed set by
/// [`crate::fed::store::RemoteWorldsComponent::list_mirror`]. So a peer cannot be
/// de-admitted while a poll is in flight -- de-admission requires editing the file and
/// restarting, and the restart ends the poll along with the process. What the boot
/// sweep reconciles is the *previous* process's rows, and what the read filter contains
/// is any other writer.
pub fn spawn_poller(state: crate::AppState) {
    let interval = Duration::from_secs(state.mirror.cfg.poll_interval_secs);
    if !state.fed_peers.is_configured() || state.fed_peers.peers().is_empty() {
        return;
    }
    tokio::spawn(async move {
        loop {
            let results = state.mirror.poll_all(&state.fed_peers).await;
            for (peer, outcome) in &results {
                match outcome {
                    // `collisions` prints a count when there was one and
                    // `unknown (<reason>)` when the probe could not run. A grep for
                    // `collisions=0` must not match a poll that never checked.
                    PollOutcome::Polled(r) => tracing::info!(
                        peer = %peer,
                        worlds = r.worlds_observed,
                        skipped = r.entries_skipped,
                        truncated = r.truncated,
                        collisions = %r.collisions,
                        "worlds mirror refreshed"
                    ),
                    PollOutcome::Failed(e) => {
                        tracing::warn!(peer = %peer, error = %e, "worlds mirror poll failed")
                    }
                }
            }
            tokio::time::sleep(interval).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_budget_terminates_on_a_peer_that_ignores_offset() {
        // 10_000 rows / 500 per page = 20, plus two pages of slack.
        assert_eq!(max_pages(10_000), 22);
        assert_eq!(max_pages(1), 3);
        assert_eq!(max_pages(0), 2);
    }

    #[test]
    fn an_unavailable_collision_probe_is_never_a_count_and_never_a_list() {
        let checked = LocalNameCollisions::Checked(vec!["a.dcl.eth".to_string()]);
        assert_eq!(checked.checked(), Some(&["a.dcl.eth".to_string()][..]));
        assert_eq!(checked.unavailable_reason(), None);
        assert_eq!(checked.to_string(), "1");

        // Knowledge of an absence: a list, and it is empty.
        let none_found = LocalNameCollisions::Checked(Vec::new());
        assert_eq!(none_found.checked(), Some(&[][..]));
        assert_eq!(none_found.to_string(), "0");

        // Absence of knowledge: no list at all, and a reason instead.
        let broken = LocalNameCollisions::Unavailable("db is on fire".to_string());
        assert_eq!(broken.checked(), None);
        assert_eq!(broken.unavailable_reason(), Some("db is on fire"));
        assert_eq!(
            broken.to_string(),
            "unknown (db is on fire)",
            "a log line for an unavailable probe must not be greppable as `collisions=0`"
        );
    }

    #[test]
    fn every_failure_variant_says_what_was_retained() {
        // The messages are operator-facing and appear verbatim in
        // `remote_peer_status.last_error`, so their shape is asserted, not assumed.
        assert_eq!(
            PollFailure::Status(500).to_string(),
            "peer answered HTTP 500"
        );
        assert_eq!(
            PollFailure::BodyTooLarge(1024).to_string(),
            "peer body exceeded 1024 bytes; refused before parse"
        );
        assert_eq!(
            PollFailure::NotJson("text/html".into()).to_string(),
            "peer answered content-type \"text/html\", expected JSON"
        );
    }
}
