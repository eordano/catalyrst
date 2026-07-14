//! The mirrored tables, and nothing else.
//!
//! `WorldsComponent` is one struct with ~40 methods over the local pool. If remote
//! rows lived in it, `state.worlds.get_world(name)` could reach them. They cannot,
//! because this is a different type with a different method set: no method returns a
//! [`crate::ports::worlds::WorldRecord`], no method issues a write against `worlds` or
//! `world_scenes`, and there is no `get_permission_records`, no `store_access`, and no
//! `create_basic_world_if_not_exists`.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, QueryBuilder, Row};

use crate::fed::names::{PeerId, RemoteWorldName};
use crate::fed::peers::{PeerOmitted, WorldsFederationPeers};

/// Rows per multi-row INSERT. Postgres caps a statement at 65535 bind parameters and
/// each row binds 10, so 500 leaves an order of magnitude of headroom.
const UPSERT_CHUNK: usize = 500;

/// One world a peer reports holding.
///
/// This is **not** [`crate::ports::worlds::WorldRecord`] with a flag, and the
/// difference is not stylistic. `WorldRecord` carries `owner: Option<String>` and five
/// independent call sites compare it to a signer with `eq_ignore_ascii_case`
/// (`permissions.rs` twice, `comms.rs`, `scenes.rs`, `world_settings.rs`). An
/// `is_remote: bool` on `WorldRecord` would mean all five must remember to check it,
/// forever, including the sixth someone adds next quarter. A type with no `owner`
/// field cannot be fed to any of them.
///
/// Also absent: `access`, `blocked_since`, `deployment_auth_chain`, `deployer`. Those
/// are local operator state and local proof; a peer has neither.
#[derive(Debug, Clone)]
pub struct RemoteWorld {
    pub peer_id: PeerId,
    pub name: RemoteWorldName,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub categories: Option<Vec<String>>,
    /// An opaque label the peer printed. We hold no bytes for it and never fetch them
    /// in this slice; `/contents/{hash}` is untouched and `contents_dir` gains nothing.
    pub thumbnail_hash: Option<String>,
    pub deployed_scenes: i64,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    /// LOCAL operator veto. Ours, not the peer's. The poller never writes this column,
    /// so a peer cannot un-hide itself by re-listing.
    pub hidden_since: Option<DateTime<Utc>>,
}

impl RemoteWorld {
    /// What `/federation/worlds/mirror` prints for this row. Defined here so the
    /// no-ownership-leak test in [`crate::fed::wire`] can assert against the exact
    /// bytes a client receives.
    pub fn as_published_view(&self) -> crate::fed::handlers::RemoteWorldView {
        crate::fed::handlers::RemoteWorldView {
            peer_id: self.peer_id.as_str().to_string(),
            name: self.name.as_peer_reported_str().to_string(),
            title: self.title.clone(),
            description: self.description.clone(),
            content_rating: self.content_rating.clone(),
            categories: self.categories.clone().unwrap_or_default(),
            thumbnail_hash: self.thumbnail_hash.clone(),
            deployed_scenes: self.deployed_scenes,
            last_deployed_at: self.last_deployed_at.map(|t| t.to_rfc3339()),
            observed_at: self.observed_at.to_rfc3339(),
        }
    }

    /// [`Self::as_published_view`], consuming: moves the owned fields into the view instead of
    /// cloning them. For callers that drop the row right after. Same JSON.
    pub fn into_published_view(self) -> crate::fed::handlers::RemoteWorldView {
        crate::fed::handlers::RemoteWorldView {
            peer_id: self.peer_id.as_str().to_string(),
            name: self.name.as_peer_reported_str().to_string(),
            title: self.title,
            description: self.description,
            content_rating: self.content_rating,
            categories: self.categories.unwrap_or_default(),
            thumbnail_hash: self.thumbnail_hash,
            deployed_scenes: self.deployed_scenes,
            last_deployed_at: self.last_deployed_at.map(|t| t.to_rfc3339()),
            observed_at: self.observed_at.to_rfc3339(),
        }
    }
}

/// The multi-row UPSERT for one chunk. Binds borrow from `chunk` rather than cloning every
/// row's owned fields; SQL text, bind order and placeholder count are unchanged.
fn build_upsert_chunk_query(chunk: &[RemoteWorld]) -> QueryBuilder<sqlx::Postgres> {
    let mut qb = QueryBuilder::new(
        "INSERT INTO remote_worlds (peer_id, world_name, title, description, \
         content_rating, categories, thumbnail_hash, deployed_scenes, \
         last_deployed_at, observed_at) ",
    );
    qb.push_values(chunk, |mut b, w| {
        b.push_bind(w.peer_id.as_str())
            .push_bind(w.name.as_peer_reported_str())
            .push_bind(w.title.as_deref())
            .push_bind(w.description.as_deref())
            .push_bind(w.content_rating.as_deref())
            .push_bind(w.categories.as_deref())
            .push_bind(w.thumbnail_hash.as_deref())
            .push_bind(w.deployed_scenes)
            .push_bind(w.last_deployed_at)
            .push_bind(w.observed_at);
    });
    // hidden_since is absent from the UPDATE arm on purpose.
    qb.push(
        " ON CONFLICT (peer_id, world_name) DO UPDATE SET \
           title            = EXCLUDED.title, \
           description      = EXCLUDED.description, \
           content_rating   = EXCLUDED.content_rating, \
           categories       = EXCLUDED.categories, \
           thumbnail_hash   = EXCLUDED.thumbnail_hash, \
           deployed_scenes  = EXCLUDED.deployed_scenes, \
           last_deployed_at = EXCLUDED.last_deployed_at, \
           observed_at      = EXCLUDED.observed_at",
    );
    qb
}

/// The admitted peer ids, canonical, as SQL bind material.
///
/// The single conversion from "the allowlist" to "the array every mirror query is
/// filtered by", so the read path and the revocation sweep cannot disagree about what
/// admitted means. It takes the allowlist **object**, not a list of strings: there is
/// no way to call a mirror query with an id set that did not come out of admission.
///
/// [`WorldsFederationPeers::NotConfigured`] yields an empty vector, and an empty
/// `= ANY(...)` matches nothing. That is the fail-closed direction: with no adjudicated
/// allowlist, nothing is publishable.
fn admitted_ids(admitted: &WorldsFederationPeers) -> Vec<String> {
    admitted
        .peers()
        .iter()
        .map(|p| p.peer_id().as_str().to_string())
        .collect()
}

/// Health of one peer's mirror, as recorded by the poller.
///
/// The point of this row is that "we have never reached this peer" and "this peer
/// holds no worlds" are different observable states. A caller that sees
/// `last_success_at: None` knows the empty listing under that peer is an absence of
/// knowledge, not knowledge of an absence.
#[derive(Debug, Clone)]
pub struct RemotePeerStatus {
    pub peer_id: String,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub worlds_observed: i64,
    pub entries_skipped: i64,
    pub truncated: bool,
    /// Set by [`RemoteWorldsComponent::revoke_peers_no_longer_admitted`] when this peer
    /// left the allowlist, cleared when it comes back. This is the bounded half of the
    /// revocation record: the per-world rows are deleted, this row says that they were
    /// and when.
    pub deadmitted_at: Option<DateTime<Utc>>,
    /// Cumulative rows destroyed by de-admission sweeps. Not served on any route; it is
    /// the operator's answer to "what did we stop publishing, and how much of it".
    pub deadmitted_worlds_deleted: i64,
}

/// Why the sweep stopped publishing a peer. Both delete the peer's rows; they are
/// different events, and a caller that cannot tell them apart will report the wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweptBecause {
    /// The entry is gone from the peer file. This is the DAO revocation the sweep exists
    /// to enforce.
    NoLongerInTheAllowlist,
    /// The entry is still in the file, DAO proposal intact, but it declares no
    /// `worlds_url` -- so it is not a worlds peer and nothing should publish rows for it.
    /// Deleting them is right; calling it a de-admission is not, and it sends an
    /// operator looking for a governance decision that never happened.
    StillListedButRunsNoWorldsServer,
}

/// One peer the boot sweep stopped publishing.
#[derive(Debug, Clone)]
pub struct RevokedPeer {
    pub peer_id: String,
    /// Which of the two sweep reasons applies. Carried in the value rather than only in
    /// the log line, so a caller reporting this to anyone else reports it correctly.
    pub because: SweptBecause,
    /// Rows destroyed for this peer by this sweep. Also accumulated into
    /// `remote_peer_status.deadmitted_worlds_deleted`, which is where it survives.
    pub worlds_deleted: i64,
    /// Up to twenty names, for the log line. Deliberately not all of them and
    /// deliberately not stored: the full list is unbounded, and a table that grows
    /// without limit is the cost a tombstone would have imposed.
    pub sample_world_names: Vec<String>,
}

/// What [`RemoteWorldsComponent::revoke_peers_no_longer_admitted`] did.
///
/// Two states rather than a report with a `ran: bool`, for the reason this module keeps
/// making: "there was no allowlist to enforce" and "the allowlist was enforced and
/// nothing had to change" are different facts, and a caller that cannot tell them apart
/// will eventually treat the first as the second.
#[derive(Debug, Clone)]
pub enum Revocation {
    /// `WORLDS_FED_PEERS_FILE` is unset. Nothing was read, nothing was written, and
    /// nothing is publishable: the routes answer 503 and [`RemoteWorldsComponent::list_mirror`]
    /// filters against an empty admitted set.
    NoAllowlistToEnforce,
    Swept {
        /// Peers whose rows were destroyed. Empty on a boot where nothing changed.
        revoked: Vec<RevokedPeer>,
        /// Peers whose de-admission tombstone was cleared because they are in the file
        /// again. Their worlds come back only as the poller re-observes them.
        readmitted: Vec<String>,
        worlds_deleted: u64,
        /// Rows left in place because they are under a local operator veto, and are
        /// therefore published by nothing regardless of admission.
        vetoed_rows_retained: i64,
    },
}

impl Revocation {
    /// Rows this sweep stopped publishing. `0` for
    /// [`Self::NoAllowlistToEnforce`] -- which is true: it published nothing to stop.
    pub fn worlds_deleted(&self) -> u64 {
        match self {
            Self::NoAllowlistToEnforce => 0,
            Self::Swept { worlds_deleted, .. } => *worlds_deleted,
        }
    }

    /// The de-admitted peer ids, sorted, or empty when there was no allowlist.
    pub fn revoked_peer_ids(&self) -> Vec<&str> {
        match self {
            Self::NoAllowlistToEnforce => Vec::new(),
            Self::Swept { revoked, .. } => revoked.iter().map(|p| p.peer_id.as_str()).collect(),
        }
    }
}

/// A reader/writer over `remote_worlds` and `remote_peer_status` only.
#[derive(Clone)]
pub struct RemoteWorldsComponent {
    pool: PgPool,
}

impl RemoteWorldsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Replace exactly one peer's rows, in one transaction.
    ///
    /// Any failure rolls the whole thing back and the peer's previous rows survive
    /// intact -- there is never a partially-replaced peer view, and a failed poll never
    /// degrades into an empty listing.
    ///
    /// `hidden_since` is preserved: the `DELETE` spares vetoed rows and the `UPDATE`
    /// arm of the upsert does not name the column. That is what makes the veto
    /// something a peer cannot revoke.
    pub async fn replace_peer_worlds(
        &self,
        peer_id: &PeerId,
        worlds: &[RemoteWorld],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM remote_worlds WHERE peer_id = $1 AND hidden_since IS NULL")
            .bind(peer_id.as_str())
            .execute(&mut *tx)
            .await?;

        for chunk in worlds.chunks(UPSERT_CHUNK) {
            let mut qb = build_upsert_chunk_query(chunk);
            qb.build().execute(&mut *tx).await?;
        }

        tx.commit().await
    }

    /// Stop publishing every peer that is no longer in the allowlist. **Boot only.**
    ///
    /// This is the line that makes the spec's revocation mechanism -- remove the entry,
    /// restart -- actually revoke something. Before it existed, `remote_worlds` rows were
    /// written per `peer_id` and never compared to the admitted set, so a peer the DAO
    /// had dropped kept being served under our origin, attributed to a peer id that
    /// `/federation/worlds/peers` no longer listed and that `?peer=` answered 404 for.
    ///
    /// # DELETE, not a tombstone -- at this granularity
    ///
    /// Per-world rows are **deleted**. They are unbounded in both directions: a peer may
    /// hold tens of thousands of worlds, and there is no ceiling on how many peers pass
    /// through the file over a deployment's life, so a tombstone per world is a table
    /// that only grows and that every mirror query then has to filter.
    ///
    /// The audit trail a bare DELETE would lose is kept one level up, where it is
    /// bounded: `remote_peer_status` already holds exactly one row per peer we have ever
    /// contacted, and migration 0006 adds `deadmitted_at` and
    /// `deadmitted_worlds_deleted` to it. After a sweep the database still says that we
    /// published this peer, when we stopped, how many rows that destroyed, when we last
    /// heard from it successfully, and how much it was serving. The world **names** --
    /// the unbounded part -- go to the log line below, which is where an unbounded list
    /// belongs.
    ///
    /// # What is spared, and why
    ///
    /// Rows under a local operator veto (`hidden_since IS NOT NULL`) are left in place,
    /// exactly as [`Self::replace_peer_worlds`] leaves them. The veto is the one thing in
    /// this table that is **ours** rather than the peer's, it took a deliberate admin
    /// action to record, and those rows are not published by any query in this file. If
    /// the sweep deleted them, a peer that was later re-admitted would silently get a
    /// world we had vetoed published again. They are bounded by operator actions, not by
    /// peer content.
    ///
    /// # Not configured is not the same as admitting nobody
    ///
    /// With `WORLDS_FED_PEERS_FILE` unset there is no adjudicated allowlist, so there is
    /// nothing to enforce and this returns [`Revocation::NoAllowlistToEnforce`] without
    /// writing. Unsetting an environment variable is a local operator mistake away from
    /// destroying every mirrored row, and it revokes nothing: all four federation routes
    /// already answer 503 in that state, and [`Self::list_mirror`] filters against an
    /// empty admitted set. Nothing is published either way, so the destructive branch
    /// runs only when a real file was read and adjudicated.
    ///
    /// A file that *was* adjudicated and admitted nobody is a different statement -- we
    /// federate with no one -- and that case does sweep everything.
    ///
    /// # What this does not do, stated plainly
    ///
    /// It is a boot sweep, and it speaks only for **this** process. During a rolling
    /// deploy the previous process is still running with the previous allowlist, and its
    /// poller will happily re-insert rows for a peer this one has just revoked. Those
    /// rows are never published by this process -- [`Self::list_mirror`] filters them out
    /// on every request, which is the entire reason the read path does not depend on this
    /// sweep -- but they do sit in the table until the old process exits and something
    /// sweeps again. They are storage, not publication. The old process, meanwhile, keeps
    /// publishing that peer until it exits, because it has a different allowlist; that is
    /// a property of running two versions at once, not something a query in this file can
    /// fix.
    pub async fn revoke_peers_no_longer_admitted(
        &self,
        admitted: &WorldsFederationPeers,
    ) -> Result<Revocation, sqlx::Error> {
        if !admitted.is_configured() {
            tracing::info!(
                "worlds federation is not configured, so there is no allowlist to enforce; \
                 mirrored rows are retained and are published by nothing \u{2014} the federation \
                 routes answer 503 and list_mirror filters against an empty admitted set"
            );
            return Ok(Revocation::NoAllowlistToEnforce);
        }
        // A peer file that names NOBODY AT ALL does not sweep.
        //
        // `peer_id <> ALL('{}')` is true for every row, so proceeding on an empty
        // admitted set deletes the entire mirror for every peer at once. A file that
        // parses to zero entries is reachable by accident in at least three ways:
        // `[[peers]]` instead of `[[peer]]` (one character, and `PeerFile.peer` is
        // `#[serde(default)]`), a truncated write, and an empty ConfigMap key.
        // Observed before this guard: a three-row mirror went to zero and the process
        // then served normally.
        //
        // The deliberate version of that state -- "federation is on, we trust nobody" --
        // is indistinguishable from the accidental one at this point, so this refuses
        // rather than guessing. Nothing is published either way: `list_mirror` filters
        // against the same empty admitted set, so the rows are retained and
        // unreachable. Revoking every peer on purpose is done by removing them
        // individually, or through the veto route.
        //
        // The test is on the whole file, not on `admitted_ids`. A file that names
        // peers, all of which are `Omitted` for running no worlds server, admits
        // nobody too -- but the operator did write those entries, so it is a statement
        // rather than a typo, and it sweeps. Guarding on the empty admitted set
        // instead would make that legitimate case undeletable and would also refuse
        // an ordinary de-admission down to zero peers.
        if !admitted.names_any_peer() {
            tracing::warn!(
                "worlds federation is configured but the peer file names no peers at all, \
                 so the reconcile sweep would delete every mirrored row; refusing. \
                 Mirrored rows are retained and published by nothing. If you meant to \
                 disable federation, unset the peer file path. If the file was expected to \
                 declare peers, check the section header: [[peer]] is singular, and \
                 [[peers]] parses as zero peers."
            );
            return Ok(Revocation::NoAllowlistToEnforce);
        }

        // The peer ids the file names but did NOT admit, because they run no worlds
        // server. Their rows still go -- they are not worlds peers, and nothing should
        // publish rows for a peer that has stopped being one -- but the reason is
        // different from a de-admission, and the log below says so. Reading a boot log
        // after clearing a `worlds_url` used to report that the DAO had dropped a peer
        // it had not dropped.
        let omitted_ids: std::collections::HashSet<String> = admitted
            .omitted()
            .iter()
            .map(|o| {
                let PeerOmitted::NoWorldsUrl { peer_id } = o;
                peer_id.clone()
            })
            .collect();
        let admitted = admitted_ids(admitted);

        let mut tx = self.pool.begin().await?;

        // What is about to be destroyed, per peer, read BEFORE destroying it. A sample
        // of names is carried into the log so the deletion is legible to whoever reads
        // the boot output; the count is what survives in the database.
        let doomed = sqlx::query(
            "SELECT peer_id, count(*) AS n, (array_agg(world_name ORDER BY world_name))[1:20] \
                    AS sample \
             FROM remote_worlds \
             WHERE hidden_since IS NULL AND peer_id <> ALL($1) \
             GROUP BY peer_id ORDER BY peer_id",
        )
        .bind(&admitted)
        .fetch_all(&mut *tx)
        .await?;

        let mut revoked = Vec::with_capacity(doomed.len());
        for row in doomed {
            let peer_id: String = row.try_get("peer_id")?;
            let because = if omitted_ids.contains(&peer_id) {
                SweptBecause::StillListedButRunsNoWorldsServer
            } else {
                SweptBecause::NoLongerInTheAllowlist
            };
            revoked.push(RevokedPeer {
                because,
                peer_id,
                worlds_deleted: row.try_get("n")?,
                sample_world_names: row.try_get("sample")?,
            });
        }

        let worlds_deleted = sqlx::query(
            "DELETE FROM remote_worlds WHERE hidden_since IS NULL AND peer_id <> ALL($1)",
        )
        .bind(&admitted)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        let vetoed_rows_retained: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_worlds \
             WHERE hidden_since IS NOT NULL AND peer_id <> ALL($1)",
        )
        .bind(&admitted)
        .fetch_one(&mut *tx)
        .await?;

        // Tombstone every peer we have a status row for and no longer admit. `COALESCE`
        // keeps the FIRST de-admission timestamp: the interesting date is when we
        // stopped publishing it, not when we last rebooted.
        sqlx::query(
            "UPDATE remote_peer_status SET deadmitted_at = COALESCE(deadmitted_at, now()) \
             WHERE peer_id <> ALL($1)",
        )
        .bind(&admitted)
        .execute(&mut *tx)
        .await?;

        // ...and record the destroyed count against it. This is an upsert rather than an
        // UPDATE because a peer can have mirrored rows with no status row at all, and
        // "we deleted 1,551 rows attributed to a peer we have no record of contacting"
        // is precisely the fact that must not evaporate.
        for peer in &revoked {
            sqlx::query(
                "INSERT INTO remote_peer_status \
                     (peer_id, deadmitted_at, deadmitted_worlds_deleted) \
                 VALUES ($1, now(), $2) \
                 ON CONFLICT (peer_id) DO UPDATE SET \
                     deadmitted_at = COALESCE(remote_peer_status.deadmitted_at, now()), \
                     deadmitted_worlds_deleted = remote_peer_status.deadmitted_worlds_deleted \
                                               + EXCLUDED.deadmitted_worlds_deleted",
            )
            .bind(&peer.peer_id)
            .bind(peer.worlds_deleted)
            .execute(&mut *tx)
            .await?;
        }

        // Re-admission clears the tombstone, so `deadmitted_at IS NOT NULL` keeps meaning
        // "we are not publishing this peer" rather than "we once weren't".
        let readmitted: Vec<String> = sqlx::query_scalar(
            "UPDATE remote_peer_status SET deadmitted_at = NULL \
             WHERE peer_id = ANY($1) AND deadmitted_at IS NOT NULL \
             RETURNING peer_id",
        )
        .bind(&admitted)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        for peer in &revoked {
            // Two ways to be swept, and they are not the same event. Saying "no longer
            // in the allowlist" about a peer that IS in the file, with its DAO proposal
            // intact, sends an operator looking for a governance decision that never
            // happened.
            if peer.because == SweptBecause::StillListedButRunsNoWorldsServer {
                tracing::warn!(
                    peer_id = %peer.peer_id,
                    worlds_deleted = peer.worlds_deleted,
                    sample = ?peer.sample_world_names,
                    "federation peer is still in the allowlist but no longer declares a \
                     worlds_url, so it is not a worlds peer; its mirrored worlds have been \
                     deleted and are no longer published. This is NOT a de-admission \u{2014} the \
                     entry, and its DAO proposal, are untouched. Restore worlds_url and the \
                     poller re-observes its worlds"
                );
            } else {
                tracing::warn!(
                    peer_id = %peer.peer_id,
                    worlds_deleted = peer.worlds_deleted,
                    sample = ?peer.sample_world_names,
                    "federation peer is no longer in the allowlist; its mirrored worlds have \
                     been deleted and are no longer published. remote_peer_status.deadmitted_at \
                     records when, and deadmitted_worlds_deleted records how many"
                );
            }
        }
        for peer_id in &readmitted {
            tracing::info!(
                peer_id = %peer_id,
                "federation peer is admitted again; its de-admission tombstone is cleared. Its \
                 worlds are republished only as the poller re-observes them \u{2014} nothing was \
                 restored from the deleted rows"
            );
        }
        if vetoed_rows_retained > 0 {
            tracing::info!(
                rows = vetoed_rows_retained,
                "rows belonging to de-admitted peers were retained because they are under a \
                 local operator veto; they are published by nothing and the veto survives a \
                 re-admission"
            );
        }
        tracing::info!(
            admitted = admitted.len(),
            peers_revoked = revoked.len(),
            worlds_deleted,
            "worlds mirror reconciled against the admitted set before serving"
        );

        Ok(Revocation::Swept {
            revoked,
            readmitted,
            worlds_deleted,
            vetoed_rows_retained,
        })
    }

    /// Vetoed rows are excluded, and so is every row belonging to a peer that is not in
    /// `admitted`. `peer` is an admitted [`PeerId`], never a raw query string: the
    /// handler resolves the `?peer=` parameter against the admitted set before it gets
    /// here, so an unknown peer yields "no such peer", not a scan.
    ///
    /// # Why the allowlist is a parameter and not an assumption
    ///
    /// The boot sweep ([`Self::revoke_peers_no_longer_admitted`]) deletes the rows of a
    /// peer that has left the file, and this predicate refuses to publish them. Those
    /// are two independent mechanisms on two different paths -- one write, one read --
    /// and the audit's point was that publication must not rest on either alone. If the
    /// sweep is skipped, mis-ordered against the first request, or defeated by a second
    /// process writing to the same database, a de-admitted peer's rows are still not
    /// served, because this query never asked for them.
    ///
    /// It also makes the two federation routes structurally incapable of disagreeing.
    /// `GET /federation/worlds/mirror` renders its `peers[]` health block from
    /// `state.fed_peers` and passes **that same value** here, in the same request, so
    /// the set of peers that can contribute a row and the set of peers that get a status
    /// line are one value read twice -- not two copies that could drift.
    pub async fn list_mirror(
        &self,
        admitted: &WorldsFederationPeers,
        peer: Option<&PeerId>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<RemoteWorld>, i64), sqlx::Error> {
        let admitted = admitted_ids(admitted);
        let peer = peer.map(|p| p.as_str().to_string());

        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_worlds \
             WHERE hidden_since IS NULL AND peer_id = ANY($1) \
               AND ($2::text IS NULL OR peer_id = $2)",
        )
        .bind(&admitted)
        .bind(peer.as_deref())
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query(
            "SELECT peer_id, world_name, title, description, content_rating, categories, \
                    thumbnail_hash, deployed_scenes, last_deployed_at, observed_at, hidden_since \
             FROM remote_worlds \
             WHERE hidden_since IS NULL AND peer_id = ANY($1) \
               AND ($2::text IS NULL OR peer_id = $2) \
             ORDER BY peer_id, world_name \
             LIMIT $3 OFFSET $4",
        )
        .bind(&admitted)
        .bind(peer.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let stored_name: String = row.try_get("world_name")?;
            // The CHECK constraint guarantees the shape that produced the row, so a
            // value that no longer parses means the table was edited out of band.
            // Fail closed: omit the row rather than publish an unadjudicated name.
            let Some(name) = RemoteWorldName::from_peer_listing(&stored_name) else {
                tracing::error!(
                    stored_name = %stored_name.escape_debug(),
                    "remote_worlds holds a name that does not pass admission; row omitted"
                );
                continue;
            };
            let stored_peer: String = row.try_get("peer_id")?;
            out.push(RemoteWorld {
                peer_id: PeerId::from_admitted(&stored_peer),
                name,
                title: row.try_get("title")?,
                description: row.try_get("description")?,
                content_rating: row.try_get("content_rating")?,
                categories: row.try_get("categories")?,
                thumbnail_hash: row.try_get("thumbnail_hash")?,
                deployed_scenes: row.try_get("deployed_scenes")?,
                last_deployed_at: row.try_get("last_deployed_at")?,
                observed_at: row.try_get("observed_at")?,
                hidden_since: row.try_get("hidden_since")?,
            });
        }
        Ok((out, total))
    }

    /// Local operator veto. Returns `false` when no such mirrored row exists, which
    /// the handler turns into a 404 -- hiding is never reported as having happened to
    /// a row that is not there.
    pub async fn set_hidden(
        &self,
        peer_id: &PeerId,
        name: &RemoteWorldName,
        hidden: bool,
    ) -> Result<bool, sqlx::Error> {
        let affected = sqlx::query(
            "UPDATE remote_worlds \
             SET hidden_since = CASE WHEN $3 THEN COALESCE(hidden_since, now()) ELSE NULL END \
             WHERE peer_id = $1 AND world_name = $2",
        )
        .bind(peer_id.as_str())
        .bind(name.as_peer_reported_str())
        .bind(hidden)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(affected > 0)
    }

    pub async fn record_attempt(&self, peer_id: &PeerId) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO remote_peer_status (peer_id, last_attempt_at) VALUES ($1, now()) \
             ON CONFLICT (peer_id) DO UPDATE SET last_attempt_at = now()",
        )
        .bind(peer_id.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_success(
        &self,
        peer_id: &PeerId,
        worlds_observed: i64,
        entries_skipped: i64,
        truncated: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO remote_peer_status \
                 (peer_id, last_attempt_at, last_success_at, last_error, \
                  worlds_observed, entries_skipped, truncated) \
             VALUES ($1, now(), now(), NULL, $2, $3, $4) \
             ON CONFLICT (peer_id) DO UPDATE SET \
                 last_attempt_at = now(), last_success_at = now(), last_error = NULL, \
                 worlds_observed = EXCLUDED.worlds_observed, \
                 entries_skipped = EXCLUDED.entries_skipped, \
                 truncated       = EXCLUDED.truncated",
        )
        .bind(peer_id.as_str())
        .bind(worlds_observed)
        .bind(entries_skipped)
        .bind(truncated)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// `last_success_at` is deliberately left alone: a failed poll must make the
    /// mirror look *stale*, not empty, and staleness is exactly the gap between
    /// `last_attempt_at` and `last_success_at`.
    pub async fn record_failure(&self, peer_id: &PeerId, error: &str) -> Result<(), sqlx::Error> {
        let clipped: String = error.chars().take(500).collect();
        sqlx::query(
            "INSERT INTO remote_peer_status (peer_id, last_attempt_at, last_error) \
             VALUES ($1, now(), $2) \
             ON CONFLICT (peer_id) DO UPDATE SET last_attempt_at = now(), last_error = $2",
        )
        .bind(peer_id.as_str())
        .bind(&clipped)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn peer_statuses(&self) -> Result<Vec<RemotePeerStatus>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT peer_id, last_attempt_at, last_success_at, last_error, \
                    worlds_observed, entries_skipped, truncated, \
                    deadmitted_at, deadmitted_worlds_deleted \
             FROM remote_peer_status ORDER BY peer_id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(RemotePeerStatus {
                    peer_id: row.try_get("peer_id")?,
                    last_attempt_at: row.try_get("last_attempt_at")?,
                    last_success_at: row.try_get("last_success_at")?,
                    last_error: row.try_get("last_error")?,
                    worlds_observed: row.try_get("worlds_observed")?,
                    entries_skipped: row.try_get("entries_skipped")?,
                    truncated: row.try_get("truncated")?,
                    deadmitted_at: row.try_get("deadmitted_at")?,
                    deadmitted_worlds_deleted: row.try_get("deadmitted_worlds_deleted")?,
                })
            })
            .collect()
    }
}

/// A **read-only** probe over the local `worlds` table.
///
/// This is the one place in `fed/` whose SQL names `worlds`, it is a `SELECT name`,
/// and its result decides exactly one thing: what to write to the log. It never
/// filters, alters or suppresses a mirrored row, and it never flows into an
/// authorization decision.
///
/// It is a separate type from [`RemoteWorldsComponent`] so that "the mirror store" and
/// "the thing allowed to look at local names" are not the same object, and so the
/// source gate in [`crate::fed::wire`] -- which forbids any INSERT/UPDATE/DELETE
/// against `worlds` anywhere under `fed/` -- has a single, obvious exception to police.
#[derive(Clone)]
pub struct LocalNameCollisionProbe {
    pool: PgPool,
}

impl LocalNameCollisionProbe {
    pub fn over(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Which of these peer-reported names also exist as local worlds.
    ///
    /// Local wins, structurally and everywhere: `/worlds` reads `worlds`,
    /// `/federation/worlds/mirror` reads `remote_worlds`, and `resolve_world_owner`
    /// takes a [`crate::fed::names::LocalWorldName`] that cannot be minted from here.
    /// Nothing is *resolved* by this call. It exists so an operator can **see** that a
    /// peer is publishing a name we also hold, which is otherwise invisible until
    /// somebody wonders why two servers list the same world.
    ///
    /// Returns the raw local strings rather than `LocalWorldName`, because these came
    /// from a table read and not from a request path, and the constructor's name is
    /// load-bearing. They are log material, nothing more.
    pub async fn local_names_also_claimed(
        &self,
        peer_reported: &[RemoteWorldName],
    ) -> Result<Vec<String>, sqlx::Error> {
        if peer_reported.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = peer_reported
            .iter()
            .map(|n| n.as_peer_reported_str().to_string())
            .collect();
        sqlx::query_scalar("SELECT name FROM worlds WHERE lower(name) = ANY($1)")
            .bind(&lowered)
            .fetch_all(&self.pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn row(
        name: &str,
        title: Option<&str>,
        description: Option<&str>,
        content_rating: Option<&str>,
        thumbnail_hash: Option<&str>,
        categories: Option<Vec<String>>,
    ) -> RemoteWorld {
        RemoteWorld {
            peer_id: PeerId::from_admitted("peer-a"),
            name: RemoteWorldName::from_peer_listing(name).unwrap(),
            title: title.map(str::to_string),
            description: description.map(str::to_string),
            content_rating: content_rating.map(str::to_string),
            categories,
            thumbnail_hash: thumbnail_hash.map(str::to_string),
            deployed_scenes: 3,
            last_deployed_at: Some(ts(1_700_000_000)),
            observed_at: ts(1_700_000_100),
            hidden_since: None,
        }
    }

    /// The clone-binding builder this replaced, verbatim, as the SQL-text parity oracle.
    fn reference_clone_builder(chunk: &[RemoteWorld]) -> QueryBuilder<sqlx::Postgres> {
        let mut qb = QueryBuilder::new(
            "INSERT INTO remote_worlds (peer_id, world_name, title, description, \
             content_rating, categories, thumbnail_hash, deployed_scenes, \
             last_deployed_at, observed_at) ",
        );
        qb.push_values(chunk, |mut b, w| {
            b.push_bind(w.peer_id.as_str())
                .push_bind(w.name.as_peer_reported_str())
                .push_bind(w.title.clone())
                .push_bind(w.description.clone())
                .push_bind(w.content_rating.clone())
                .push_bind(w.categories.clone())
                .push_bind(w.thumbnail_hash.clone())
                .push_bind(w.deployed_scenes)
                .push_bind(w.last_deployed_at)
                .push_bind(w.observed_at);
        });
        qb.push(
            " ON CONFLICT (peer_id, world_name) DO UPDATE SET \
               title            = EXCLUDED.title, \
               description      = EXCLUDED.description, \
               content_rating   = EXCLUDED.content_rating, \
               categories       = EXCLUDED.categories, \
               thumbnail_hash   = EXCLUDED.thumbnail_hash, \
               deployed_scenes  = EXCLUDED.deployed_scenes, \
               last_deployed_at = EXCLUDED.last_deployed_at, \
               observed_at      = EXCLUDED.observed_at",
        );
        qb
    }

    #[test]
    fn upsert_chunk_sql_is_identical_to_the_clone_binding_version() {
        let rows = vec![
            row(
                "w1.dcl.eth",
                Some("Title 1"),
                Some("Desc 1"),
                Some("E"),
                Some("hash1"),
                Some(vec!["art".to_string(), "games".to_string()]),
            ),
            row("w2.dcl.eth", None, Some("Desc 2"), None, None, None),
            row(
                "w3.dcl.eth",
                Some("Title 3"),
                None,
                Some("T"),
                Some("hash3"),
                Some(vec!["music".to_string()]),
            ),
        ];
        let new_sql = build_upsert_chunk_query(&rows).into_sql();
        let old_sql = reference_clone_builder(&rows).into_sql();
        assert_eq!(new_sql, old_sql, "SQL text / placeholder count drifted");
    }

    #[test]
    fn into_published_view_matches_borrowed_view_and_moves_the_strings() {
        let full = row(
            "w.dcl.eth",
            Some("The Title"),
            Some("A description"),
            Some("EVERYONE"),
            Some("QmThumb"),
            Some(vec!["art".to_string(), "games".to_string()]),
        );
        assert_eq!(
            serde_json::to_value(full.clone().into_published_view()).unwrap(),
            serde_json::to_value(full.as_published_view()).unwrap(),
        );

        // All-None optionals: categories must serialize as [] in both.
        let bare = row("bare.dcl.eth", None, None, None, None, None);
        assert_eq!(
            serde_json::to_value(bare.clone().into_published_view()).unwrap(),
            serde_json::to_value(bare.as_published_view()).unwrap(),
        );

        // Zero-copy proof: the String/Vec buffers are MOVED, not cloned.
        let moved = row(
            "z.dcl.eth",
            Some("keep-title"),
            Some("keep-desc"),
            Some("E"),
            Some("h"),
            Some(vec!["first-cat".to_string()]),
        );
        let t_ptr = moved.title.as_ref().unwrap().as_ptr();
        let d_ptr = moved.description.as_ref().unwrap().as_ptr();
        let c_ptr = moved.categories.as_ref().unwrap()[0].as_ptr();
        let view = moved.into_published_view();
        assert_eq!(view.title.as_ref().unwrap().as_ptr(), t_ptr);
        assert_eq!(view.description.as_ref().unwrap().as_ptr(), d_ptr);
        assert_eq!(view.categories[0].as_ptr(), c_ptr);
    }
}
