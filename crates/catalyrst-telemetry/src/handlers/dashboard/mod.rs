use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AppState;

use super::db_err;

mod admin;
mod flags;

pub use admin::{
    experiment_set, experiments_get, set_issue_state, sql_query, ExperimentSetBody,
    ExperimentsQuery, IssueStateBody, SqlBody,
};
pub use flags::{flag_set, flags, FlagSetBody, FlagsQuery};

const TITLE1: &str = "split_part(COALESCE(\
    NULLIF(body->>'message',''), \
    NULLIF(body#>>'{logentry,message}',''), \
    NULLIF(body#>>'{exception,values,0,type}','') || COALESCE(': ' || (body#>>'{exception,values,0,value}'), ''), \
    NULLIF(body->>'transaction',''), \
    NULLIF(body->>'event',''), \
    CASE WHEN event_kind = 'session' THEN 'session (' || COALESCE(NULLIF(body->>'status',''), CASE WHEN (body->>'init')::boolean THEN 'started' ELSE 'update' END) || ')' END, \
    CASE WHEN body->>'userId' IS NOT NULL THEN 'identify ' || (body->>'userId') END, \
    '(' || event_kind || ')'), E'\\n', 1)";

fn filters() -> String {
    format!(
        "($1::text IS NULL OR source = $1) \
         AND ($2::text IS NULL OR event_kind = $2) \
         AND ($3::text IS NULL OR body->>'level' = $3) \
         AND ($4::text IS NULL OR {TITLE1} ILIKE '%'||$4||'%' OR body::text ILIKE '%'||$4||'%') \
         AND received_at > now() - make_interval(hours => $5::int) \
         AND ($6::text IS NULL OR fingerprint = $6) \
         AND ($9::text IS NULL OR body->>'environment' = $9) \
         AND ($10::text IS NULL OR body->>'release' = $10) \
         AND ($12::text IS NULL OR body->'tags'->>$12 = $13) \
         AND ($14::text IS NULL OR body->'properties'->>$14 = $15)"
    )
}

fn blank(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|v| !v.is_empty()).cloned()
}

fn split_tag(s: &Option<String>) -> (Option<String>, Option<String>) {
    match blank(s).and_then(|t| {
        t.split_once(':')
            .map(|(k, v)| (k.to_string(), v.to_string()))
    }) {
        Some((k, v)) if !k.is_empty() => (Some(k), Some(v)),
        _ => (None, None),
    }
}

#[derive(Deserialize)]
pub struct ListParams {
    source: Option<String>,
    kind: Option<String>,
    level: Option<String>,
    q: Option<String>,

    fingerprint: Option<String>,
    environment: Option<String>,
    release: Option<String>,

    tag: Option<String>,

    prop: Option<String>,

    sort: Option<String>,

    status: Option<String>,
    #[serde(default = "d_hours")]
    hours: i64,
    #[serde(default = "d_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    group: i64,
}
fn d_hours() -> i64 {
    24
}
fn d_limit() -> i64 {
    100
}

type Norm = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
);
fn norm(p: &ListParams) -> Norm {
    (
        blank(&p.source),
        blank(&p.kind),
        blank(&p.level),
        blank(&p.q),
        blank(&p.fingerprint),
        p.hours.clamp(1, 24 * 365),
        p.limit.clamp(1, 500),
        p.offset.max(0),
    )
}

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../../dashboard.html"))
}

#[derive(sqlx::FromRow, Serialize)]
struct EventRow {
    id: i64,
    received_at: String,
    kind: String,
    source: String,
    project: String,
    level: Option<String>,
    title: Option<String>,
    properties: Option<Value>,
}

#[derive(sqlx::FromRow, Serialize)]
struct IssueRow {
    fingerprint: Option<String>,
    count: i64,
    last_seen: String,
    first_seen: String,
    title: Option<String>,
    level: Option<String>,
    kind: Option<String>,
    sample_id: i64,
    users: i64,
    status: Option<String>,
    assignee: Option<String>,
}

const TS: &str = "to_char(received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')";

pub async fn events(
    State(st): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let (source, kind, level, q, fingerprint, hours, limit, offset) = norm(&p);
    let environment = blank(&p.environment);
    let release = blank(&p.release);
    let status = blank(&p.status);
    let (tag_key, tag_val) = split_tag(&p.tag);
    let (prop_key, prop_val) = split_tag(&p.prop);

    if p.group == 1 {
        let sql = format!(
            "WITH agg AS ( \
               SELECT fingerprint, count(*) AS count, \
                 count(DISTINCT body->'user'->>'id') AS users, \
                 max(received_at) AS last_seen, min(received_at) AS first_seen, \
                 (array_agg(id ORDER BY received_at DESC))[1] AS sample_id \
               FROM telemetry.telemetry_events WHERE {filters} GROUP BY fingerprint), \
             g AS ( \
               SELECT a.*, st.assignee, \
                 CASE WHEN st.status = 'resolved' AND a.last_seen > st.updated_at THEN 'unresolved' \
                      ELSE COALESCE(st.status,'unresolved') END AS status \
               FROM agg a LEFT JOIN telemetry.issue_state st ON st.fingerprint = a.fingerprint) \
             SELECT g.fingerprint, g.count, g.users, \
               to_char(g.last_seen AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_seen, \
               to_char(g.first_seen AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS first_seen, \
               {sample_title} AS title, \
               s.body->>'level' AS level, \
               COALESCE(NULLIF(s.body#>>'{{exception,values,0,type}}',''), s.event_kind) AS kind, g.sample_id, \
               g.status, g.assignee \
             FROM g JOIN telemetry.telemetry_events s ON s.id = g.sample_id \
             WHERE ($11::text IS NULL OR g.status = $11) \
             ORDER BY g.{order_col} LIMIT $7 OFFSET $8",
            filters = filters(),
            sample_title = TITLE1.replace("body", "s.body"),
            order_col = if p.sort.as_deref() == Some("frequent") { "count DESC" } else { "last_seen DESC" },
        );
        let rows = sqlx::query_as::<_, IssueRow>(sqlx::AssertSqlSafe(sql))
            .bind(&source)
            .bind(&kind)
            .bind(&level)
            .bind(&q)
            .bind(hours)
            .bind(&fingerprint)
            .bind(limit)
            .bind(offset)
            .bind(&environment)
            .bind(&release)
            .bind(&status)
            .bind(&tag_key)
            .bind(&tag_val)
            .bind(&prop_key)
            .bind(&prop_val)
            .fetch_all(&st.pool)
            .await
            .map_err(|e| db_err("telemetry dashboard", e))?;
        Ok(Json(json!({ "group": true, "items": rows })))
    } else {
        let sql = format!(
            "SELECT id, {TS} AS received_at, \
               COALESCE(NULLIF(body#>>'{{exception,values,0,type}}',''), event_kind) AS kind, source, project, \
               body->>'level' AS level, {TITLE1} AS title, \
               body->'properties' AS properties \
             FROM telemetry.telemetry_events WHERE {filters} \
             ORDER BY received_at DESC LIMIT $7 OFFSET $8",
            filters = filters(),
        );

        let rows = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(sql))
            .bind(&source)
            .bind(&kind)
            .bind(&level)
            .bind(&q)
            .bind(hours)
            .bind(&fingerprint)
            .bind(limit)
            .bind(offset)
            .bind(&environment)
            .bind(&release)
            .bind(None::<String>)
            .bind(&tag_key)
            .bind(&tag_val)
            .bind(&prop_key)
            .bind(&prop_val)
            .fetch_all(&st.pool)
            .await
            .map_err(|e| db_err("telemetry dashboard", e))?;
        Ok(Json(json!({ "group": false, "items": rows })))
    }
}

pub async fn event_detail(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row = sqlx::query_as::<_, (i64, String, String, String, String, Value)>(
        sqlx::AssertSqlSafe(format!(
            "SELECT id, source, project, event_kind, {TS} AS received_at, body \
         FROM telemetry.telemetry_events WHERE id = $1"
        )),
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;
    match row {
        Some((id, source, project, kind, received_at, body)) => Ok(Json(json!({
            "id": id, "source": source, "project": project, "kind": kind,
            "received_at": received_at, "body": body
        }))),
        None => Err((StatusCode::NOT_FOUND, "no such event".into())),
    }
}

#[derive(Deserialize)]
pub struct StatsParams {
    #[serde(default = "d_hours")]
    hours: i64,

    fingerprint: Option<String>,

    source: Option<String>,
}

pub async fn stats(
    State(st): State<AppState>,
    Query(p): Query<StatsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hours = p.hours.clamp(1, 24 * 365);
    let fp = p.fingerprint.filter(|v| !v.is_empty());
    let src = p.source.filter(|v| !v.is_empty());

    let win = "received_at > now() - make_interval(hours => $1::int) \
               AND ($2::text IS NULL OR fingerprint = $2) \
               AND ($3::text IS NULL OR source = $3)";
    let bucket = if hours <= 48 { "hour" } else { "day" };
    let rows = sqlx::query_as::<_, (String, Option<String>, i64)>(sqlx::AssertSqlSafe(format!(
        "WITH w AS (SELECT event_kind, source, body->>'level' AS level, body->>'environment' AS env, \
                      body->>'release' AS rel, received_at \
                    FROM telemetry.telemetry_events WHERE {win}) \
         SELECT dim, k, c FROM ( \
           SELECT 'level' AS dim, COALESCE(level,'(none)') AS k, count(*) AS c FROM w GROUP BY 2 \
           UNION ALL SELECT 'kind', event_kind, count(*) FROM w GROUP BY 2 \
           UNION ALL SELECT 'source', source, count(*) FROM w GROUP BY 2 \
           UNION ALL SELECT 'env', env, count(*) FROM w WHERE env IS NOT NULL GROUP BY 2 \
           UNION ALL SELECT 'release', rel, count(*) FROM w WHERE rel IS NOT NULL GROUP BY 2 \
           UNION ALL SELECT 'series', to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI'), count(*) FROM w GROUP BY 2 \
         ) x ORDER BY dim, CASE WHEN dim = 'series' THEN k END, c DESC"
    )))
    .bind(hours)
    .bind(&fp)
    .bind(&src)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;

    let mut by_level = Vec::new();
    let mut by_kind = Vec::new();
    let mut by_source = Vec::new();
    let mut by_env = Vec::new();
    let mut by_release = Vec::new();
    let mut series = Vec::new();
    for (dim, k, c) in rows {
        match dim.as_str() {
            "level" => by_level.push((k, c)),
            "kind" => by_kind.push((k, c)),
            "source" => by_source.push((k, c)),
            "env" => by_env.push((k, c)),
            "release" => by_release.push((k, c)),
            "series" => series.push((k.unwrap_or_default(), c)),
            _ => {}
        }
    }

    let total: i64 = by_kind.iter().map(|(_, c)| c).sum();
    let pair = |v: Vec<(Option<String>, i64)>| -> Vec<Value> {
        v.into_iter()
            .map(|(k, c)| json!([k.unwrap_or_default(), c]))
            .collect()
    };
    Ok(Json(json!({
        "total": total,
        "hours": hours,
        "bucket": bucket,
        "by_level": pair(by_level),
        "by_kind": pair(by_kind),
        "by_source": pair(by_source),
        "by_env": pair(by_env),
        "by_release": pair(by_release),
        "series": series.into_iter().map(|(b, c)| json!([b, c])).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct HealthParams {
    #[serde(default = "d_hours")]
    hours: i64,
    release: Option<String>,
}

pub async fn health(
    State(st): State<AppState>,
    Query(p): Query<HealthParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hours = p.hours.clamp(1, 24 * 365);
    let rel = p.release.filter(|v| !v.is_empty());
    let win = "source='sentry' AND event_kind='session' \
               AND received_at > now() - make_interval(hours => $1::int) \
               AND ($2::text IS NULL OR body->'attrs'->>'release' = $2)";
    let bucket = if hours <= 48 { "hour" } else { "day" };
    let rows = sqlx::query_as::<_, (String, Option<String>, i64, i64)>(sqlx::AssertSqlSafe(format!(
        "WITH w AS (SELECT body->>'status' AS status, body->>'did' AS did, \
                      body->'attrs'->>'release' AS rel, received_at \
                    FROM telemetry.telemetry_events WHERE {win}) \
         SELECT dim, k, c, c2 FROM ( \
           SELECT 'status' AS dim, COALESCE(NULLIF(status,''),'ok') AS k, count(*) AS c, 0::bigint AS c2 FROM w GROUP BY 2 \
           UNION ALL SELECT 'users', NULL, count(DISTINCT did), count(DISTINCT did) FILTER (WHERE status = 'crashed') FROM w \
           UNION ALL (SELECT 'release', rel, count(*), count(*) FILTER (WHERE status = 'crashed') FROM w GROUP BY 2 ORDER BY 3 DESC LIMIT 30) \
           UNION ALL SELECT 'series', to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI'), count(*), 0 FROM w GROUP BY 2 \
         ) x ORDER BY dim, CASE WHEN dim = 'series' THEN k END, c DESC"
    )))
    .bind(hours)
    .bind(&rel)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;

    let mut by_status: Vec<(Option<String>, i64)> = Vec::new();
    let mut by_release: Vec<(Option<String>, i64, i64)> = Vec::new();
    let mut series: Vec<(String, i64)> = Vec::new();
    let (mut total_users, mut crashed_users) = (0i64, 0i64);
    for (dim, k, c, c2) in rows {
        match dim.as_str() {
            "status" => by_status.push((k, c)),
            "users" => (total_users, crashed_users) = (c, c2),
            "release" => by_release.push((k, c, c2)),
            "series" => series.push((k.unwrap_or_default(), c)),
            _ => {}
        }
    }
    let total: i64 = by_status.iter().map(|(_, c)| c).sum();
    let unhealthy: i64 = by_status
        .iter()
        .filter(|(k, _)| {
            matches!(
                k.as_deref(),
                Some("crashed" | "abnormal" | "unhandled" | "errored")
            )
        })
        .map(|(_, c)| c)
        .sum();
    let crashed: i64 = by_status
        .iter()
        .filter(|(k, _)| k.as_deref() == Some("crashed"))
        .map(|(_, c)| c)
        .sum();
    let crash_free = if total > 0 {
        (1.0 - crashed as f64 / total as f64) * 100.0
    } else {
        100.0
    };
    let healthy_rate = if total > 0 {
        (1.0 - unhealthy as f64 / total as f64) * 100.0
    } else {
        100.0
    };
    let crash_free_users = if total_users > 0 {
        (1.0 - crashed_users as f64 / total_users as f64) * 100.0
    } else {
        100.0
    };
    Ok(Json(json!({
        "total": total, "crash_free_rate": crash_free, "healthy_rate": healthy_rate,
        "crashed": crashed, "unhealthy": unhealthy, "hours": hours,
        "total_users": total_users, "crashed_users": crashed_users,
        "crash_free_users_rate": crash_free_users,
        "by_status": by_status.into_iter().map(|(k,c)| json!([k.unwrap_or_default(), c])).collect::<Vec<_>>(),
        "by_release": by_release.into_iter().map(|(r,t,b)| json!({
            "release": r.unwrap_or_default(), "sessions": t,
            "crash_free": if t>0 {(1.0 - b as f64/t as f64)*100.0} else {100.0}})).collect::<Vec<_>>(),
        "series": series.into_iter().map(|(b,c)| json!([b,c])).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct FunnelParams {
    #[serde(default = "d_hours")]
    hours: i64,

    steps: Option<String>,

    prop: Option<String>,
}

pub async fn funnel(
    State(st): State<AppState>,
    Query(p): Query<FunnelParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hours = p.hours.clamp(1, 24 * 365);
    let steps: Vec<String> = p
        .steps
        .unwrap_or_default()
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if steps.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            "need >=2 steps (pipe-separated)".into(),
        ));
    }
    let (prop_key, prop_val) = split_tag(&p.prop);

    let rows = sqlx::query_as::<_, (Option<String>, String, String)>(sqlx::AssertSqlSafe(format!(
        "SELECT {USERKEY} AS uk, body->>'event' AS ev, \
           to_char(min(received_at) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS') AS t \
         FROM telemetry.telemetry_events \
         WHERE source='segment' AND received_at > now() - make_interval(hours => $1::int) \
           AND body->>'event' = ANY($2) AND {USERKEY} IS NOT NULL \
           AND ($3::text IS NULL OR body->'properties'->>$3 = $4) \
         GROUP BY 1,2"
    )))
    .bind(hours)
    .bind(&steps)
    .bind(&prop_key)
    .bind(&prop_val)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;
    use std::collections::HashMap;
    let mut per_user: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (uk, ev, t) in rows {
        if let Some(uk) = uk {
            per_user.entry(uk).or_default().insert(ev, t);
        }
    }

    let mut counts = vec![0i64; steps.len()];
    for evs in per_user.values() {
        let mut last: Option<&String> = None;
        for (i, step) in steps.iter().enumerate() {
            match evs.get(step) {
                Some(t) if last.is_none_or(|l| t >= l) => {
                    counts[i] += 1;
                    last = Some(t);
                }
                _ => break,
            }
        }
    }
    let first = counts.first().copied().unwrap_or(0).max(1);
    let result: Vec<Value> = steps
        .iter()
        .zip(&counts)
        .enumerate()
        .map(|(i, (step, &c))| {
            let prev = if i == 0 { first } else { counts[i - 1].max(1) };
            json!({ "step": step, "users": c,
            "pct_of_first": (c as f64 / first as f64) * 100.0,
            "pct_of_prev": (c as f64 / prev as f64) * 100.0 })
        })
        .collect();
    Ok(Json(json!({ "hours": hours, "steps": result })))
}

#[derive(Deserialize)]
pub struct BreakdownParams {
    #[serde(default = "d_hours")]
    hours: i64,
    event: Option<String>,
    prop: Option<String>,
}

pub async fn breakdown(
    State(st): State<AppState>,
    Query(p): Query<BreakdownParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hours = p.hours.clamp(1, 24 * 365);
    let event = p.event.filter(|v| !v.is_empty());
    let prop = p.prop.filter(|v| !v.is_empty());
    let win = "source='segment' AND received_at > now() - make_interval(hours => $1::int) \
               AND ($2::text IS NULL OR body->>'event' = $2)";
    let Some(prop) = prop else {
        let keys = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(format!(
            "SELECT DISTINCT jsonb_object_keys(body->'properties') k \
             FROM telemetry.telemetry_events WHERE {win} AND jsonb_typeof(body->'properties')='object' ORDER BY 1 LIMIT 100")))
            .bind(hours).bind(&event).fetch_all(&st.pool).await.map_err(|e| db_err("telemetry dashboard", e))?;
        return Ok(Json(
            json!({ "props": keys.into_iter().map(|(k,)| k).collect::<Vec<_>>(), "rows": [] }),
        ));
    };
    let rows = sqlx::query_as::<_, (Option<String>, i64, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT body->'properties'->>$3 AS v, count(*) c, \
           count(DISTINCT {USERKEY}) u \
         FROM telemetry.telemetry_events WHERE {win} AND body->'properties' ? $3 \
         GROUP BY 1 ORDER BY 2 DESC LIMIT 100"
    )))
    .bind(hours)
    .bind(&event)
    .bind(&prop)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;
    Ok(Json(json!({ "prop": prop, "rows": rows.into_iter()
        .map(|(v,c,u)| json!([v.unwrap_or_else(|| "(null)".into()), c, u])).collect::<Vec<_>>() })))
}

const USERKEY: &str =
    "COALESCE(body->'user'->>'id', body->'user'->>'username', body->>'userId', body->>'anonymousId')";

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct StoryRow {
    id: i64,
    received_at: String,
    source: String,
    kind: String,
    level: Option<String>,
    title: Option<String>,
    current: bool,
}

pub async fn story(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let uk_t = USERKEY.replace("body", "t.body");
    let title_t = TITLE1.replace("body", "t.body");
    let row: Option<(Option<String>, Value, Option<Value>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH a AS (SELECT {USERKEY} AS uk, received_at AS ts FROM telemetry.telemetry_events WHERE id = $1), \
         ev AS (SELECT t.id, t.received_at AS ts, t.source, t.event_kind AS kind, t.body->>'level' AS level, \
                  {title_t} AS title, (t.id = $1) AS current \
                FROM telemetry.telemetry_events t, a \
                WHERE a.uk <> '' AND {uk_t} = a.uk \
                  AND t.received_at BETWEEN a.ts - interval '6 hours' AND a.ts + interval '1 hour' \
                ORDER BY t.received_at LIMIT 200) \
         SELECT a.uk, \
           (SELECT COALESCE(jsonb_agg(jsonb_build_object( \
              'id', id, 'received_at', to_char(ts AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), \
              'source', source, 'kind', kind, 'level', level, 'title', title, 'current', current) \
              ORDER BY ts), '[]'::jsonb) FROM ev) AS events, \
           (SELECT COALESCE(body->'context'->'campaign', body->'properties'->'campaign') \
            FROM telemetry.telemetry_events WHERE a.uk <> '' AND {USERKEY} = a.uk \
              AND COALESCE(body->'context'->'campaign', body->'properties'->'campaign') IS NOT NULL \
            ORDER BY received_at DESC LIMIT 1) AS utm \
         FROM a"
    )))
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;
    let (user_key, events, utm) = match row {
        None => return Err((StatusCode::NOT_FOUND, "no such event".into())),
        Some((Some(uk), events, utm)) if !uk.is_empty() => (uk, events, utm),
        Some(_) => return Ok(Json(json!({ "user": null, "utm": null, "events": [] }))),
    };
    let events: Vec<StoryRow> = serde_json::from_value(events)
        .map_err(|e| db_err("telemetry dashboard", sqlx::Error::decode(e)))?;
    Ok(Json(
        json!({ "user": user_key, "utm": utm, "count": events.len(), "events": events }),
    ))
}

pub async fn metrics(
    State(st): State<AppState>,
    Query(p): Query<StatsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let hours = p.hours.clamp(1, 24 * 365);
    let win = "source = 'segment' AND received_at > now() - make_interval(hours => $1::int)";
    let bucket = if hours <= 48 { "hour" } else { "day" };
    let rows = sqlx::query_as::<_, (String, Option<String>, i64)>(sqlx::AssertSqlSafe(format!(
        "WITH w AS (SELECT body->>'event' AS ev, event_kind, \
                      COALESCE(body->>'userId', body->>'anonymousId') AS uk, received_at \
                    FROM telemetry.telemetry_events WHERE {win}) \
         SELECT dim, k, c FROM ( \
           SELECT 'type' AS dim, event_kind AS k, count(*) AS c FROM w GROUP BY 2 \
           UNION ALL (SELECT 'event', ev, count(*) FROM w WHERE ev IS NOT NULL GROUP BY 2 ORDER BY 3 DESC LIMIT 50) \
           UNION ALL SELECT 'users', NULL, count(DISTINCT uk) FROM w \
           UNION ALL SELECT 'series', to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI'), count(*) FROM w GROUP BY 2 \
         ) x ORDER BY dim, CASE WHEN dim = 'series' THEN k END, c DESC"
    )))
    .bind(hours)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;

    let mut by_event = Vec::new();
    let mut by_type = Vec::new();
    let mut series = Vec::new();
    let mut users = 0i64;
    for (dim, k, c) in rows {
        match dim.as_str() {
            "event" => by_event.push((k, c)),
            "type" => by_type.push((k, c)),
            "users" => users = c,
            "series" => series.push((k.unwrap_or_default(), c)),
            _ => {}
        }
    }
    let total: i64 = by_type.iter().map(|(_, c)| c).sum();
    let pair = |v: Vec<(Option<String>, i64)>| -> Vec<Value> {
        v.into_iter()
            .map(|(k, c)| json!([k.unwrap_or_default(), c]))
            .collect()
    };
    Ok(Json(json!({
        "total": total, "users": users, "hours": hours, "bucket": bucket,
        "by_event": pair(by_event), "by_type": pair(by_type),
        "series": series.into_iter().map(|(b, c)| json!([b, c])).collect::<Vec<_>>(),
    })))
}

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct SessEvent {
    id: i64,
    received_at: String,
    level: Option<String>,
    title: Option<String>,
    kind: String,
}

type SessionRow = (
    Option<String>,
    Option<String>,
    Value,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Value,
);

pub async fn session(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row: Option<SessionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH a AS (SELECT body->'user'->>'id' AS uid, body->'contexts'->'app'->>'app_start_time' AS app_start \
                    FROM telemetry.telemetry_events WHERE id = $1), \
         s AS (SELECT id, received_at, body->>'level' AS level, {TITLE1} AS title, \
                 COALESCE(NULLIF(body#>>'{{exception,values,0,type}}',''), event_kind) AS kind \
               FROM telemetry.telemetry_events, a \
               WHERE source='sentry' AND event_kind='event' AND body->'user'->>'id' = a.uid \
                 AND (a.app_start IS NULL OR body->'contexts'->'app'->>'app_start_time' = a.app_start)) \
         SELECT a.uid, a.app_start, \
           (SELECT COALESCE(jsonb_agg(jsonb_build_object('id', e.id, 'received_at', {ts_e}, \
              'level', e.level, 'title', e.title, 'kind', e.kind) ORDER BY e.received_at), '[]'::jsonb) \
            FROM (SELECT * FROM s ORDER BY received_at ASC LIMIT 1000) e) AS events, \
           (SELECT count(*) FROM s) AS total, \
           (SELECT count(*) FILTER (WHERE level IN ('error','fatal')) FROM s) AS errors, \
           (SELECT to_char(min(received_at) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') FROM s) AS first, \
           (SELECT to_char(max(received_at) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') FROM s) AS last, \
           (SELECT COALESCE(jsonb_agg(jsonb_build_array(k, c) ORDER BY c DESC), '[]'::jsonb) \
            FROM (SELECT COALESCE(level,'(none)') AS k, count(*) AS c FROM s GROUP BY 1) g) AS by_level \
         FROM a",
        ts_e = TS.replace("received_at", "e.received_at"),
    )))
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| db_err("telemetry dashboard", e))?;
    let Some((Some(user), app_start, events, total, errors, first, last, by_level)) = row else {
        return Ok(Json(json!({ "user": null, "events": [] })));
    };
    let events: Vec<SessEvent> = serde_json::from_value(events)
        .map_err(|e| db_err("telemetry dashboard", sqlx::Error::decode(e)))?;
    let by_level: Vec<(Option<String>, i64)> = serde_json::from_value(by_level)
        .map_err(|e| db_err("telemetry dashboard", sqlx::Error::decode(e)))?;
    Ok(Json(json!({
        "user": user, "app_start": app_start, "anchor": id,
        "total": total, "errors": errors, "first": first, "last": last,
        "by_level": by_level.into_iter().map(|(k,c)| json!([k.unwrap_or_default(), c])).collect::<Vec<_>>(),
        "events": events,
    })))
}
