//! Admin controls owned by catalyrst-telemetry (admin-console §4 "LATER"
//! tranche). Every route here is bearer-gated: `Authorization: Bearer <token>`
//! compared in constant time against `CATALYRST_TELEMETRY_ADMIN_TOKEN`. When that
//! env is unset the crate fails closed — every admin route returns 403.
//!
//! These are loopback-trusted, consistent with the rest of the `/dash/*` surface
//! (which is fronted by the tailnet / loopback and never exposed publicly), but
//! unlike the read-only `/dash/*` routes these mutate, so they additionally
//! require the bearer. They are namespaced under `/dash/admin/*` so they never
//! collide with the ingest or existing dashboard routes.
//!
//! Every mutation records a row in `admin_audit` (who/what/when), which doubles
//! as the issue-history feed read back by `GET /dash/admin/audit`.

use std::sync::atomic::Ordering;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

type AdminResult = Result<Json<Value>, (StatusCode, String)>;

fn db_err(e: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}"))
}

fn bad(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

/// Constant-time bearer comparison, mirroring catalyrst-comms `timing_safe_eq` /
/// `authorize_moderator`. Fails closed (403) when the admin token env is unset.
fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Core authorization decision, split out from `HeaderMap` so it is unit-testable.
/// Fails closed when `expected` is None (admin token env unset).
fn token_ok(expected: Option<&str>, presented: Option<&str>) -> bool {
    match (expected, presented) {
        (Some(e), Some(p)) => timing_safe_eq(p, e),
        _ => false,
    }
}

/// Returns Ok(()) iff the request carries a valid admin bearer. 403 otherwise
/// (including when the env is unset — default-safe).
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    if state.admin_token.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            "admin disabled (CATALYRST_TELEMETRY_ADMIN_TOKEN unset)".into(),
        ));
    }
    let presented = bearer_token(headers);
    if token_ok(state.admin_token.as_deref(), presented.as_deref()) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "invalid admin bearer".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_safe_eq_matches_and_rejects() {
        assert!(timing_safe_eq("secret", "secret"));
        assert!(!timing_safe_eq("secret", "Secret"));
        assert!(!timing_safe_eq("secret", "secre"));
        assert!(!timing_safe_eq("secret", ""));
    }

    #[test]
    fn fails_closed_when_token_unset() {
        // env unset -> expected is None -> any presentation is rejected (403 path).
        assert!(!token_ok(None, Some("anything")));
        assert!(!token_ok(None, None));
    }

    #[test]
    fn requires_matching_bearer() {
        assert!(token_ok(Some("tok"), Some("tok")));
        assert!(!token_ok(Some("tok"), Some("nope")));
        assert!(!token_ok(Some("tok"), None));
    }

    #[test]
    fn bearer_token_extracts_prefix() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&h).as_deref(), Some("abc123"));
        let empty = HeaderMap::new();
        assert_eq!(bearer_token(&empty), None);
    }

    #[test]
    fn actor_prefers_console_header_over_query() {
        let mut h = HeaderMap::new();
        h.insert("x-catalyrst-admin", "alice".parse().unwrap());
        let q = ActorQuery { actor: Some("mallory".into()) };
        // Trusted header wins over the spoofable query param.
        assert_eq!(actor_of(&h, &q), "alice");
    }

    #[test]
    fn actor_falls_back_to_query_when_header_absent() {
        let h = HeaderMap::new();
        let q = ActorQuery { actor: Some("bob".into()) };
        assert_eq!(actor_of(&h, &q), "bob");
    }

    #[test]
    fn actor_defaults_to_loopback() {
        let h = HeaderMap::new();
        let q = ActorQuery::default();
        assert_eq!(actor_of(&h, &q), "loopback");
        // Blank header + blank query also default to loopback.
        let mut h2 = HeaderMap::new();
        h2.insert("x-catalyrst-admin", "   ".parse().unwrap());
        let q2 = ActorQuery { actor: Some("  ".into()) };
        assert_eq!(actor_of(&h2, &q2), "loopback");
    }

    #[test]
    fn actor_label_truncates_to_100_chars() {
        let mut h = HeaderMap::new();
        let long: String = "x".repeat(250);
        h.insert("x-catalyrst-admin", long.parse().unwrap());
        let q = ActorQuery::default();
        assert_eq!(actor_of(&h, &q).chars().count(), 100);
    }
}

/// Operator label for the audit log. The trustworthy source is the
/// `X-Catalyrst-Admin` request header, set server-side by the admin console
/// (not reachable by a browser fetch / spoofable client). The legacy `?actor=`
/// query param is honored only as a last-resort fallback when the header is
/// absent. When neither is present we default to "loopback" — these routes are
/// loopback-trusted, so an unattributed call is a direct local operator.
#[derive(Deserialize, Default)]
pub struct ActorQuery {
    #[serde(default)]
    actor: Option<String>,
}

/// Truncate/trim a candidate actor label; None if blank after trimming.
fn clean_actor(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

/// Pull the console-set actor from the `X-Catalyrst-Admin` header.
fn header_actor(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_actor)
}

/// Resolve the audit actor: trusted header first, then the spoofable `?actor=`
/// query param as a last-resort fallback, else "loopback".
fn actor_of(headers: &HeaderMap, q: &ActorQuery) -> String {
    header_actor(headers)
        .or_else(|| q.actor.as_deref().and_then(clean_actor))
        .unwrap_or_else(|| "loopback".to_string())
}

async fn audit(state: &AppState, actor: &str, action: &str, detail: Value) {
    let _ = sqlx::query("INSERT INTO admin_audit (actor, action, detail) VALUES ($1, $2, $3)")
        .bind(actor)
        .bind(action)
        .bind(detail)
        .execute(&state.pool)
        .await;
}

// ===================== Data retention / purge =====================

#[derive(Deserialize)]
pub struct PurgeBody {
    /// Delete events strictly older than this many days. Required, must be >= 1.
    older_than_days: i64,
    /// Optional scoping. When omitted, applies across all events.
    source: Option<String>,
    project: Option<String>,
}

/// Delete events older than N days (optionally scoped to a source/project).
/// Distinct from bulk-delete: this is the retention guillotine keyed only on age.
pub async fn purge(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<PurgeBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    if b.older_than_days < 1 {
        return Err(bad("older_than_days must be >= 1"));
    }
    let res = sqlx::query(
        "DELETE FROM telemetry_events \
         WHERE received_at < now() - make_interval(days => $1::int) \
           AND ($2::text IS NULL OR source = $2) \
           AND ($3::text IS NULL OR project = $3)",
    )
    .bind(b.older_than_days)
    .bind(b.source.as_deref().filter(|s| !s.is_empty()))
    .bind(b.project.as_deref().filter(|s| !s.is_empty()))
    .execute(&st.pool)
    .await
    .map_err(db_err)?;
    let deleted = res.rows_affected() as i64;
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "purge",
        json!({ "older_than_days": b.older_than_days, "source": b.source, "project": b.project, "deleted": deleted }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "deleted": deleted })))
}

// ===================== Ingest enable/disable toggle =====================

#[derive(Deserialize)]
pub struct IngestBody {
    enabled: bool,
}

/// Flip the master ingest toggle. Persisted to `admin_settings` and mirrored into
/// the in-memory `IngestControl` so the hot ingest path enforces it without a DB
/// read. When disabled, sentry/segment ingest accepts the request shape but drops
/// the payload (no client errors / retry storms).
pub async fn ingest_toggle(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<IngestBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    sqlx::query(
        "INSERT INTO admin_settings (key, value, updated_at) VALUES ('ingest_enabled', $1, now()) \
         ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = now()",
    )
    .bind(if b.enabled { "true" } else { "false" })
    .execute(&st.pool)
    .await
    .map_err(db_err)?;
    st.ingest.enabled.store(b.enabled, Ordering::Relaxed);
    let actor = actor_of(&headers, &aq);
    audit(&st, &actor, "ingest_toggle", json!({ "enabled": b.enabled })).await;
    Ok(Json(json!({ "ok": true, "enabled": b.enabled })))
}

// ===================== Per-project quota =====================

#[derive(Deserialize)]
pub struct QuotaBody {
    project: String,
    /// Daily event cap (UTC day) for this project. `null` clears the quota
    /// (back to unlimited). When present, must be >= 0.
    daily_limit: Option<i64>,
}

/// Set or clear a per-project daily ingest quota. Persisted to `project_quota`
/// and mirrored into the in-memory quota map. Counting is O(1) per event and
/// resets each UTC day. Clearing (`daily_limit: null`) restores unlimited.
pub async fn quota(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<QuotaBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    if b.project.is_empty() {
        return Err(bad("project required"));
    }
    match b.daily_limit {
        Some(limit) => {
            if limit < 0 {
                return Err(bad("daily_limit must be >= 0"));
            }
            sqlx::query(
                "INSERT INTO project_quota (project, daily_limit, updated_at) VALUES ($1, $2, now()) \
                 ON CONFLICT (project) DO UPDATE SET daily_limit = $2, updated_at = now()",
            )
            .bind(&b.project)
            .bind(limit)
            .execute(&st.pool)
            .await
            .map_err(db_err)?;
            st.ingest
                .quotas
                .write()
                .unwrap()
                .insert(b.project.clone(), limit);
        }
        None => {
            sqlx::query("DELETE FROM project_quota WHERE project = $1")
                .bind(&b.project)
                .execute(&st.pool)
                .await
                .map_err(db_err)?;
            st.ingest.quotas.write().unwrap().remove(&b.project);
        }
    }
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "quota",
        json!({ "project": b.project, "daily_limit": b.daily_limit }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "project": b.project, "daily_limit": b.daily_limit })))
}

// ===================== Bulk delete =====================

#[derive(Deserialize)]
pub struct BulkFilter {
    source: Option<String>,
    project: Option<String>,
    fingerprint: Option<String>,
    /// Only events received before this RFC3339/parseable timestamp (passed to
    /// postgres as text, cast to timestamptz).
    before: Option<String>,
    /// Restrict by event level (body->>'level').
    level: Option<String>,
}

impl BulkFilter {
    fn require_some(&self) -> Result<(), (StatusCode, String)> {
        if self.source.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.project.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.fingerprint.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.before.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.level.as_deref().filter(|s| !s.is_empty()).is_none()
        {
            return Err(bad(
                "refusing an unfiltered bulk operation; specify at least one of source/project/fingerprint/before/level",
            ));
        }
        Ok(())
    }
    /// The five NULL-skippable bind values for `BULK_WHERE`, in $1..$5 order.
    fn binds(&self) -> [Option<&str>; 5] {
        [
            self.source.as_deref().filter(|s| !s.is_empty()),
            self.project.as_deref().filter(|s| !s.is_empty()),
            self.fingerprint.as_deref().filter(|s| !s.is_empty()),
            self.before.as_deref().filter(|s| !s.is_empty()),
            self.level.as_deref().filter(|s| !s.is_empty()),
        ]
    }
}

/// Shared WHERE for bulk delete/export. $1 source, $2 project, $3 fingerprint,
/// $4 before (timestamptz text), $5 level. Each NULL-skips.
const BULK_WHERE: &str = "($1::text IS NULL OR source = $1) \
     AND ($2::text IS NULL OR project = $2) \
     AND ($3::text IS NULL OR fingerprint = $3) \
     AND ($4::text IS NULL OR received_at < $4::timestamptz) \
     AND ($5::text IS NULL OR body->>'level' = $5)";

/// Delete events matching an explicit filter. Refuses a fully-unfiltered request
/// (use /dash/admin/purge for blanket age-based retention).
pub async fn bulk_delete(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(f): Json<BulkFilter>,
) -> AdminResult {
    authorize(&st, &headers)?;
    f.require_some()?;
    let sql = format!("DELETE FROM telemetry_events WHERE {BULK_WHERE}");
    let [b1, b2, b3, b4, b5] = f.binds();
    let res = sqlx::query(&sql)
        .bind(b1)
        .bind(b2)
        .bind(b3)
        .bind(b4)
        .bind(b5)
        .execute(&st.pool)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("db error: {e}")))?;
    let deleted = res.rows_affected() as i64;
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "bulk_delete",
        json!({ "source": f.source, "project": f.project, "fingerprint": f.fingerprint, "before": f.before, "level": f.level, "deleted": deleted }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "deleted": deleted })))
}

// ===================== Bulk export =====================

#[derive(Deserialize)]
pub struct ExportBody {
    #[serde(flatten)]
    filter: BulkFilter,
    /// Max rows to return (default 1000, capped at 10000).
    limit: Option<i64>,
}

/// Export matching events as JSON (newest first). Same filter grammar as
/// bulk-delete; an unfiltered export IS allowed (read-only) but capped.
pub async fn export(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<ExportBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    let limit = b.limit.unwrap_or(1000).clamp(1, 10_000);
    let sql = format!(
        "SELECT to_jsonb(t) AS row FROM ( \
           SELECT id, source, project, event_kind, fingerprint, \
             to_char(received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS received_at, \
             body \
           FROM telemetry_events WHERE {BULK_WHERE} \
           ORDER BY received_at DESC LIMIT {limit} \
         ) t"
    );
    let [b1, b2, b3, b4, b5] = b.filter.binds();
    let rows: Vec<Value> = sqlx::query_scalar(&sql)
        .bind(b1)
        .bind(b2)
        .bind(b3)
        .bind(b4)
        .bind(b5)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("db error: {e}")))?;
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "export",
        json!({ "source": b.filter.source, "project": b.filter.project, "fingerprint": b.filter.fingerprint, "before": b.filter.before, "level": b.filter.level, "count": rows.len() }),
    )
    .await;
    let truncated = rows.len() as i64 >= limit;
    Ok(Json(json!({ "ok": true, "count": rows.len(), "truncated": truncated, "events": rows })))
}

// ===================== Issue history / audit =====================

#[derive(Deserialize)]
pub struct AuditQuery {
    /// Filter to one fingerprint's history (issue audit drill-down).
    fingerprint: Option<String>,
    /// Filter by action verb (exact, e.g. "regroup").
    action: Option<String>,
    #[serde(default = "d_audit_limit")]
    limit: i64,
}
fn d_audit_limit() -> i64 {
    200
}

/// Read the admin audit log (newest first). Doubles as issue history when filtered
/// by `fingerprint`. Read-only, but still bearer-gated (the log names operators).
pub async fn audit_list(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> AdminResult {
    authorize(&st, &headers)?;
    let limit = q.limit.clamp(1, 1000);
    let sql = format!(
        "SELECT id, to_char(at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS at, \
           actor, action, detail \
         FROM admin_audit \
         WHERE ($1::text IS NULL OR detail->>'fingerprint' = $1) \
           AND ($2::text IS NULL OR action = $2) \
         ORDER BY at DESC LIMIT {limit}"
    );
    let rows = sqlx::query_as::<_, (i64, String, String, String, Value)>(&sql)
        .bind(q.fingerprint.as_deref().filter(|s| !s.is_empty()))
        .bind(q.action.as_deref().filter(|s| !s.is_empty()))
        .fetch_all(&st.pool)
        .await
        .map_err(db_err)?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, at, actor, action, detail)| {
            json!({ "id": id, "at": at, "actor": actor, "action": action, "detail": detail })
        })
        .collect();
    Ok(Json(json!({ "ok": true, "count": items.len(), "items": items })))
}

// ===================== Regroup (issue merge) =====================

#[derive(Deserialize)]
pub struct RegroupBody {
    /// Fingerprint(s) to fold into the canonical issue.
    sources: Vec<String>,
    /// The surviving (canonical) fingerprint.
    canonical: String,
}

/// Merge one or more source fingerprints into a canonical one. Records the
/// mapping in `issue_merge` (so the read path can resolve the effective
/// fingerprint) and copies the canonical's workflow state forward where the
/// source had none. Idempotent per source via upsert.
pub async fn regroup(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<RegroupBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    if b.canonical.is_empty() {
        return Err(bad("canonical required"));
    }
    let sources: Vec<String> = b
        .sources
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != &b.canonical)
        .collect();
    if sources.is_empty() {
        return Err(bad("at least one source fingerprint (distinct from canonical) required"));
    }
    let mut merged = 0i64;
    for src in &sources {
        sqlx::query(
            "INSERT INTO issue_merge (source_fingerprint, canonical_fingerprint, merged_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (source_fingerprint) \
             DO UPDATE SET canonical_fingerprint = $2, merged_at = now()",
        )
        .bind(src)
        .bind(&b.canonical)
        .execute(&st.pool)
        .await
        .map_err(db_err)?;
        merged += 1;
    }
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "regroup",
        json!({ "fingerprint": b.canonical, "canonical": b.canonical, "sources": sources, "merged": merged }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "canonical": b.canonical, "merged": merged })))
}

// ===================== Release state =====================

#[derive(Deserialize)]
pub struct ReleaseBody {
    release: String,
    /// active | archived | broken
    state: String,
    note: Option<String>,
}

/// Set a release's lifecycle state (active / archived / broken). Upserts into
/// `release_state`; read-only views can join on the release string to flag a
/// known-bad or retired release.
pub async fn release(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<ReleaseBody>,
) -> AdminResult {
    authorize(&st, &headers)?;
    if b.release.is_empty() {
        return Err(bad("release required"));
    }
    if !matches!(b.state.as_str(), "active" | "archived" | "broken") {
        return Err(bad("state must be active|archived|broken"));
    }
    sqlx::query(
        "INSERT INTO release_state (release, state, note, updated_at) VALUES ($1, $2, $3, now()) \
         ON CONFLICT (release) DO UPDATE SET state = $2, note = $3, updated_at = now()",
    )
    .bind(&b.release)
    .bind(&b.state)
    .bind(b.note.as_deref().filter(|s| !s.is_empty()))
    .execute(&st.pool)
    .await
    .map_err(db_err)?;
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "release",
        json!({ "release": b.release, "state": b.state, "note": b.note }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "release": b.release, "state": b.state })))
}
