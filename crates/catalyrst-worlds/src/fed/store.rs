//! The mirrored tables, and nothing else.
//!
//! Deliberately a different type from `WorldsComponent`, so remote rows are
//! unreachable through it: no method here returns a
//! [`crate::ports::worlds::WorldRecord`], none writes `worlds` or `world_scenes`, and
//! there is no `get_permission_records`, `store_access` or
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
/// Deliberately not [`crate::ports::worlds::WorldRecord`] with an `is_remote` flag: no
/// `owner` field means it cannot be fed to the five call sites that compare
/// `WorldRecord::owner` to a signer (`permissions.rs` twice, `comms.rs`, `scenes.rs`,
/// `world_settings.rs`). Also absent: `access`, `blocked_since`,
/// `deployment_auth_chain`, `deployer` -- local operator state a peer has none of.
#[derive(Debug, Clone)]
pub struct RemoteWorld {
    pub peer_id: PeerId,
    pub name: RemoteWorldName,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub categories: Option<Vec<String>>,
    /// An opaque label the peer printed; we hold no bytes for it and never fetch them.
    pub thumbnail_hash: Option<String>,
    pub deployed_scenes: i64,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    /// Local operator veto. The poller never writes this column, so a peer cannot
    /// un-hide itself by re-listing.
    pub hidden_since: Option<DateTime<Utc>>,
}

impl RemoteWorld {
    /// Here rather than in the handler so the no-ownership-leak test in
    /// [`crate::fed::wire`] can assert the exact bytes a client receives.
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

    /// [`Self::as_published_view`] by move rather than clone. Same JSON.
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

/// SQL text, bind order and placeholder count match the older cloning builder
/// (asserted in tests).
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

/// The single conversion from "the allowlist" to "the array every mirror query filters
/// by". Takes the allowlist object, so no id set reaches a mirror query without passing
/// admission. [`WorldsFederationPeers::NotConfigured`] yields an empty vector, and an
/// empty `= ANY(...)` matches nothing -- fail-closed.
fn admitted_ids(admitted: &WorldsFederationPeers) -> Vec<String> {
    admitted
        .peers()
        .iter()
        .map(|p| p.peer_id().as_str().to_string())
        .collect()
}

/// Health of one peer's mirror, as recorded by the poller. Keeps "never reached" and
/// "holds no worlds" distinguishable: `last_success_at: None` means an empty listing
/// under that peer is absence of knowledge, not knowledge of absence.
#[derive(Debug, Clone)]
pub struct RemotePeerStatus {
    pub peer_id: String,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub worlds_observed: i64,
    pub entries_skipped: i64,
    pub truncated: bool,
    /// Set when this peer left the allowlist, cleared when it comes back. The bounded
    /// half of the revocation record -- the per-world rows are deleted.
    pub deadmitted_at: Option<DateTime<Utc>>,
    /// Cumulative rows destroyed by de-admission sweeps. Not served on any route.
    pub deadmitted_worlds_deleted: i64,
}

/// Both delete the peer's rows, but they are different events and must be reported
/// differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweptBecause {
    /// The entry is gone from the peer file -- the DAO revocation the sweep enforces.
    NoLongerInTheAllowlist,
    /// The entry is still in the file, DAO proposal intact, but declares no
    /// `worlds_url`, so it is not a worlds peer. Its rows go, but this is not a
    /// de-admission and must not be reported as one.
    StillListedButRunsNoWorldsServer,
}

#[derive(Debug, Clone)]
pub struct RevokedPeer {
    pub peer_id: String,
    pub because: SweptBecause,
    /// Also accumulated into `remote_peer_status.deadmitted_worlds_deleted`, where it
    /// survives.
    pub worlds_deleted: i64,
    /// Up to twenty names, for the log line. Not all of them and not stored: the full
    /// list is unbounded.
    pub sample_world_names: Vec<String>,
}

/// Two states rather than a `ran: bool`, so "no allowlist to enforce" and "enforced,
/// nothing changed" stay distinguishable.
#[derive(Debug, Clone)]
pub enum Revocation {
    /// `WORLDS_FED_PEERS_FILE` is unset. Nothing read, nothing written, nothing
    /// publishable: the routes answer 503 and
    /// [`RemoteWorldsComponent::list_mirror`] filters against an empty admitted set.
    NoAllowlistToEnforce,
    Swept {
        revoked: Vec<RevokedPeer>,
        /// Tombstone cleared because they are in the file again. Their worlds come back
        /// only as the poller re-observes them.
        readmitted: Vec<String>,
        worlds_deleted: u64,
        /// Left in place under a local operator veto, so published by nothing
        /// regardless of admission.
        vetoed_rows_retained: i64,
    },
}

impl Revocation {
    pub fn worlds_deleted(&self) -> u64 {
        match self {
            Self::NoAllowlistToEnforce => 0,
            Self::Swept { worlds_deleted, .. } => *worlds_deleted,
        }
    }

    /// Sorted; empty when there was no allowlist.
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

    /// One transaction, so a failed poll rolls back intact rather than degrading into
    /// an empty listing. `hidden_since` is preserved -- the `DELETE` spares vetoed rows
    /// and the `UPDATE` arm does not name the column -- so a peer cannot revoke a veto.
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

    /// Stop publishing every peer no longer in the allowlist. **Boot only**; this is
    /// what makes "remove the entry, restart" revoke anything.
    ///
    /// Per-world rows are **deleted**, not tombstoned: they are unbounded, so a
    /// per-world tombstone is a table that only grows and that every mirror query then
    /// filters. The audit trail lives one level up, bounded --
    /// `remote_peer_status.deadmitted_at` and `deadmitted_worlds_deleted` (migration
    /// 0006); the unbounded world names go to the log line.
    ///
    /// Rows under a local operator veto (`hidden_since IS NOT NULL`) are spared, as in
    /// [`Self::replace_peer_worlds`]: nothing publishes them, and deleting them would
    /// silently republish a vetoed world on re-admission.
    ///
    /// Unset `WORLDS_FED_PEERS_FILE` is not "admit nobody": it returns
    /// [`Revocation::NoAllowlistToEnforce`] without writing, so a typo'd env var cannot
    /// destroy every mirrored row (nothing is published in that state anyway). A file
    /// that was read and admitted nobody *does* sweep everything.
    ///
    /// It speaks only for **this** process: during a rolling deploy the old process
    /// keeps re-inserting and publishing its peers until it exits. Those rows are
    /// storage, not publication -- [`Self::list_mirror`] filters them on every request,
    /// which is why the read path does not depend on this sweep.
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

        sqlx::query(
            "UPDATE remote_peer_status SET deadmitted_at = COALESCE(deadmitted_at, now()) \
             WHERE peer_id <> ALL($1)",
        )
        .bind(&admitted)
        .execute(&mut *tx)
        .await?;

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

    /// Vetoed rows are excluded, and so is every row from a peer not in `admitted`.
    /// `peer` is an admitted [`PeerId`], never a raw query string -- the handler
    /// resolves `?peer=` against the admitted set first, so an unknown peer yields "no
    /// such peer", not a scan.
    ///
    /// The allowlist is a parameter rather than an assumption so publication does not
    /// rest on the boot sweep alone: if [`Self::revoke_peers_no_longer_admitted`] is
    /// skipped, mis-ordered against the first request, or defeated by a second process
    /// on the same database, a de-admitted peer's rows are still never served. It also
    /// keeps `GET /federation/worlds/mirror`'s `peers[]` block and its rows from
    /// drifting -- both read the one `state.fed_peers` value in that request.
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

    /// Local operator veto. `false` when no such mirrored row exists, which the handler
    /// turns into a 404.
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

    /// `last_success_at` is deliberately left alone: a failed poll must make the mirror
    /// look stale, not empty, and staleness is the gap between the two timestamps.
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

/// A **read-only** probe over the local `worlds` table: the one place in `fed/` whose
/// SQL names `worlds`, a `SELECT name` whose result decides only what to log.
///
/// Separate from [`RemoteWorldsComponent`] so the source gate in [`crate::fed::wire`]
/// -- which forbids INSERT/UPDATE/DELETE against `worlds` anywhere under `fed/` -- has
/// a single obvious exception to police.
#[derive(Clone)]
pub struct LocalNameCollisionProbe {
    pool: PgPool,
}

impl LocalNameCollisionProbe {
    pub fn over(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Nothing is resolved here -- local always wins structurally (`/worlds` reads
    /// `worlds`, `/federation/worlds/mirror` reads `remote_worlds`, and
    /// `resolve_world_owner` takes a [`crate::fed::names::LocalWorldName`] that cannot
    /// be minted here). This only lets an operator see the collision.
    ///
    /// Raw strings rather than `LocalWorldName` because they came from a table read, not
    /// a request path; they are log material.
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

        let bare = row("bare.dcl.eth", None, None, None, None, None);
        assert_eq!(
            serde_json::to_value(bare.clone().into_published_view()).unwrap(),
            serde_json::to_value(bare.as_published_view()).unwrap(),
        );

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
