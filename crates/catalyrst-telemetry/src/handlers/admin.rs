use std::sync::atomic::Ordering;

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};

use crate::AppState;

use super::db_err;

type AdminResult = Result<Json<Value>, (StatusCode, String)>;

fn bad(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

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

fn token_ok(expected: Option<&str>, presented: Option<&str>) -> bool {
    match (expected, presented) {
        (Some(e), Some(p)) => timing_safe_eq(p, e),
        _ => false,
    }
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get("cookie")?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == super::login::COOKIE_NAME && !value.is_empty()).then(|| value.to_string())
    })
}

pub(crate) fn token_matches(state: &AppState, presented: &str) -> bool {
    token_ok(state.admin_token.as_deref(), Some(presented))
}

// Reads and SSR pages accept the browser session cookie set by /login as well as
// the bearer; mutations stay bearer-only so a cookie-bearing browser can never be
// cross-site-driven into flipping operator state.
pub(crate) fn authorize_read(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    if state.admin_token.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            "admin disabled (CATALYRST_TELEMETRY_ADMIN_TOKEN unset)".into(),
        ));
    }
    let presented = bearer_token(headers).or_else(|| cookie_token(headers));
    if token_ok(state.admin_token.as_deref(), presented.as_deref()) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "invalid admin bearer".into()))
    }
}

pub(crate) fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
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

/// The environment variable that names telemetry's admin bearer secret. Server-chosen; it is
/// the same string the legacy `authorize` refusal printed, and the shared chokepoint uses it
/// to build its audit label and 503 detail.
const ADMIN_TOKEN_ENV: &str = "CATALYRST_TELEMETRY_ADMIN_TOKEN";

/// Telemetry's admin gate, expressed as an unforgeable extractor rather than a forgettable
/// `authorize(&st, &headers)?` at the top of a handler body.
///
/// Naming `TelemetryAdmin` in a handler signature makes the bearer check a term in the type
/// the router demands: axum refuses the handler unless the argument resolves, and the only
/// way it resolves at request time is [`TelemetryAdmin::from_request_parts`], which runs the
/// shared [`AuthenticatedAdminIdentity`] chokepoint. There is no other constructor -- the
/// field is private and the type derives nothing -- so the check can no longer be deleted from
/// a body.
///
/// Wire behaviour is preserved byte-for-byte. The shared extractor answers **401** for a
/// missing or mismatched secret and **503** for an unconfigured one; telemetry has always
/// answered **403** for both, and the `/dash` route-layer middleware (`require_telemetry_admin`,
/// left as a documented follow-on) still does, so [`TelemetryAdmin::from_request_parts`] maps
/// the refusal back onto this crate's exact 403 responses.
pub struct TelemetryAdmin {
    // Held only as evidence that the chokepoint ran; deliberately unread. The audit actor is
    // still taken from the request (`actor_of`) rather than from this verified identity, so the
    // `admin_audit` rows and the /dash/admin/audit response stay byte-identical.
    #[allow(dead_code)]
    identity: AuthenticatedAdminIdentity,
}

/// Map the shared extractor's refusal back onto telemetry's historical wire responses, so the
/// migration changes no status code or body a client can observe.
fn admin_rejection_as_legacy_forbidden(rejection: &AdminAuthRejection) -> (StatusCode, String) {
    match rejection.refusal().http_status() {
        // Unconfigured secret <=> `admin_token` is `None` (lib.rs filters empty to `None`, so the
        // chokepoint's empty-as-unconfigured case cannot arise here). Legacy: the unset branch.
        503 => (
            StatusCode::FORBIDDEN,
            "admin disabled (CATALYRST_TELEMETRY_ADMIN_TOKEN unset)".into(),
        ),
        // Missing or mismatched bearer (401), collapsed to the legacy 403 like the old
        // `authorize`.
        _ => (StatusCode::FORBIDDEN, "invalid admin bearer".into()),
    }
}

impl FromRequestParts<AppState> for TelemetryAdmin {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let secret = ConfiguredAdminBearerSecret {
            environment_variable: ADMIN_TOKEN_ENV,
            configured: state.admin_token.clone(),
        };
        // `ConfiguredAdminBearerSecret` stands in as the extractor's state via axum's blanket
        // `impl<T: Clone> FromRef<T> for T`; the secret is the whole state the extractor reads.
        match AuthenticatedAdminIdentity::from_request_parts(parts, &secret).await {
            Ok(identity) => Ok(Self { identity }),
            Err(rejection) => Err(admin_rejection_as_legacy_forbidden(&rejection)),
        }
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
    fn cookie_token_parses_session_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            "other=1; telemetry_admin=tok123; more=2".parse().unwrap(),
        );
        assert_eq!(cookie_token(&h).as_deref(), Some("tok123"));
        let mut empty_val = HeaderMap::new();
        empty_val.insert("cookie", "telemetry_admin=".parse().unwrap());
        assert_eq!(cookie_token(&empty_val), None);
        assert_eq!(cookie_token(&HeaderMap::new()), None);
    }

    #[test]
    fn actor_prefers_console_header_over_query() {
        let mut h = HeaderMap::new();
        h.insert("x-catalyrst-admin", "alice".parse().unwrap());
        let q = ActorQuery {
            actor: Some("mallory".into()),
        };

        assert_eq!(actor_of(&h, &q), "alice");
    }

    #[test]
    fn actor_falls_back_to_query_when_header_absent() {
        let h = HeaderMap::new();
        let q = ActorQuery {
            actor: Some("bob".into()),
        };
        assert_eq!(actor_of(&h, &q), "bob");
    }

    #[test]
    fn actor_defaults_to_loopback() {
        let h = HeaderMap::new();
        let q = ActorQuery::default();
        assert_eq!(actor_of(&h, &q), "loopback");

        let mut h2 = HeaderMap::new();
        h2.insert("x-catalyrst-admin", "   ".parse().unwrap());
        let q2 = ActorQuery {
            actor: Some("  ".into()),
        };
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

#[derive(Deserialize, Default)]
pub struct ActorQuery {
    #[serde(default)]
    actor: Option<String>,
}

fn clean_actor(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

fn header_actor(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-catalyrst-admin")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_actor)
}

fn actor_of(headers: &HeaderMap, q: &ActorQuery) -> String {
    header_actor(headers)
        .or_else(|| q.actor.as_deref().and_then(clean_actor))
        .unwrap_or_else(|| "loopback".to_string())
}

pub(crate) async fn audit(state: &AppState, actor: &str, action: &str, detail: Value) {
    let _ = sqlx::query("INSERT INTO admin_audit (actor, action, detail) VALUES ($1, $2, $3)")
        .bind(actor)
        .bind(action)
        .bind(detail)
        .execute(&state.pool)
        .await;
}

#[derive(Deserialize)]
pub struct PurgeBody {
    older_than_days: i64,

    source: Option<String>,
    project: Option<String>,
}

pub async fn purge(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<PurgeBody>,
) -> AdminResult {
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
    .map_err(|e| db_err("telemetry admin", e))?;
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

#[derive(Deserialize)]
pub struct IngestBody {
    enabled: bool,
}

pub async fn ingest_toggle(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<IngestBody>,
) -> AdminResult {
    sqlx::query(
        "INSERT INTO admin_settings (key, value, updated_at) VALUES ('ingest_enabled', $1, now()) \
         ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = now()",
    )
    .bind(if b.enabled { "true" } else { "false" })
    .execute(&st.pool)
    .await
    .map_err(|e| db_err("telemetry admin", e))?;
    st.ingest.enabled.store(b.enabled, Ordering::Relaxed);
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "ingest_toggle",
        json!({ "enabled": b.enabled }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "enabled": b.enabled })))
}

#[derive(Deserialize)]
pub struct QuotaBody {
    project: String,

    daily_limit: Option<i64>,
}

pub async fn quota(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<QuotaBody>,
) -> AdminResult {
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
            .map_err(|e| db_err("telemetry admin", e))?;
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
                .map_err(|e| db_err("telemetry admin", e))?;
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
    Ok(Json(
        json!({ "ok": true, "project": b.project, "daily_limit": b.daily_limit }),
    ))
}

#[derive(Deserialize)]
pub struct BulkFilter {
    source: Option<String>,
    project: Option<String>,
    fingerprint: Option<String>,

    before: Option<String>,

    level: Option<String>,
}

impl BulkFilter {
    fn require_some(&self) -> Result<(), (StatusCode, String)> {
        if self.source.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.project.as_deref().filter(|s| !s.is_empty()).is_none()
            && self
                .fingerprint
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_none()
            && self.before.as_deref().filter(|s| !s.is_empty()).is_none()
            && self.level.as_deref().filter(|s| !s.is_empty()).is_none()
        {
            return Err(bad(
                "refusing an unfiltered bulk operation; specify at least one of source/project/fingerprint/before/level",
            ));
        }
        Ok(())
    }

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

const BULK_WHERE: &str = "($1::text IS NULL OR source = $1) \
     AND ($2::text IS NULL OR project = $2) \
     AND ($3::text IS NULL OR fingerprint = $3) \
     AND ($4::text IS NULL OR received_at < $4::timestamptz) \
     AND ($5::text IS NULL OR body->>'level' = $5)";

pub async fn bulk_delete(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(f): Json<BulkFilter>,
) -> AdminResult {
    f.require_some()?;
    let sql = format!("DELETE FROM telemetry_events WHERE {BULK_WHERE}");
    let [b1, b2, b3, b4, b5] = f.binds();
    let res = sqlx::query(sqlx::AssertSqlSafe(sql))
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

#[derive(Deserialize)]
pub struct ExportBody {
    #[serde(flatten)]
    filter: BulkFilter,

    limit: Option<i64>,
}

pub async fn export(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<ExportBody>,
) -> AdminResult {
    let limit = b.limit.unwrap_or(100).clamp(1, 10_000);

    let sql = format!(
        "SELECT to_jsonb(t) AS row FROM ( \
           SELECT id, source, project, event_kind, fingerprint, \
             to_char(received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS received_at, \
             body \
           FROM telemetry_events WHERE {BULK_WHERE} \
           ORDER BY telemetry_events.received_at DESC LIMIT {limit} \
         ) t"
    );
    let [b1, b2, b3, b4, b5] = b.filter.binds();
    let rows: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
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
    Ok(Json(
        json!({ "ok": true, "count": rows.len(), "truncated": truncated, "events": rows }),
    ))
}

#[derive(Deserialize)]
pub struct AuditQuery {
    fingerprint: Option<String>,

    action: Option<String>,
    #[serde(default = "d_audit_limit")]
    limit: i64,
}
fn d_audit_limit() -> i64 {
    200
}

pub async fn audit_list(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> AdminResult {
    let limit = q.limit.clamp(1, 1000);
    let sql = format!(
        "SELECT id, to_char(at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS at, \
           actor, action, detail \
         FROM admin_audit \
         WHERE ($1::text IS NULL OR detail->>'fingerprint' = $1) \
           AND ($2::text IS NULL OR action = $2) \
         ORDER BY at DESC LIMIT {limit}"
    );
    let rows = sqlx::query_as::<_, (i64, String, String, String, Value)>(sqlx::AssertSqlSafe(sql))
        .bind(q.fingerprint.as_deref().filter(|s| !s.is_empty()))
        .bind(q.action.as_deref().filter(|s| !s.is_empty()))
        .fetch_all(&st.pool)
        .await
        .map_err(|e| db_err("telemetry admin", e))?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, at, actor, action, detail)| {
            json!({ "id": id, "at": at, "actor": actor, "action": action, "detail": detail })
        })
        .collect();
    Ok(Json(
        json!({ "ok": true, "count": items.len(), "items": items }),
    ))
}

#[derive(Deserialize)]
pub struct RegroupBody {
    sources: Vec<String>,

    canonical: String,
}

pub async fn regroup(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<RegroupBody>,
) -> AdminResult {
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
        return Err(bad(
            "at least one source fingerprint (distinct from canonical) required",
        ));
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
        .map_err(|e| db_err("telemetry admin", e))?;
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
    Ok(Json(
        json!({ "ok": true, "canonical": b.canonical, "merged": merged }),
    ))
}

#[derive(Deserialize)]
pub struct ReleaseBody {
    release: String,

    state: String,
    note: Option<String>,
}

pub async fn release(
    _admin: TelemetryAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(aq): Query<ActorQuery>,
    Json(b): Json<ReleaseBody>,
) -> AdminResult {
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
    .map_err(|e| db_err("telemetry admin", e))?;
    let actor = actor_of(&headers, &aq);
    audit(
        &st,
        &actor,
        "release",
        json!({ "release": b.release, "state": b.state, "note": b.note }),
    )
    .await;
    Ok(Json(
        json!({ "ok": true, "release": b.release, "state": b.state }),
    ))
}
