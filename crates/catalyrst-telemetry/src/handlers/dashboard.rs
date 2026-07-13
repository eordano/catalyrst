use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AppState;

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
    Html(include_str!("../dashboard.html"))
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
            .map_err(err)?;
        Ok(Json(json!({ "group": true, "items": rows })))
    } else {
        let sql = format!(
            "SELECT id, {TS} AS received_at, \
               COALESCE(NULLIF(body#>>'{{exception,values,0,type}}',''), event_kind) AS kind, source, project, \
               body->>'level' AS level, {TITLE1} AS title \
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
            .map_err(err)?;
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
    .map_err(err)?;
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

    let group_count = |col: &str| {
        format!("SELECT {col} AS k, count(*) AS c FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 2 DESC")
    };
    async fn counts(
        pool: &sqlx::PgPool,
        sql: &str,
        hours: i64,
        fp: &Option<String>,
        src: &Option<String>,
    ) -> Result<Vec<(Option<String>, i64)>, sqlx::Error> {
        sqlx::query_as::<_, (Option<String>, i64)>(sqlx::AssertSqlSafe(sql))
            .bind(hours)
            .bind(fp)
            .bind(src)
            .fetch_all(pool)
            .await
    }
    let by_level = counts(
        &st.pool,
        &group_count("COALESCE(body->>'level','(none)')"),
        hours,
        &fp,
        &src,
    )
    .await
    .map_err(err)?;
    let by_kind = counts(&st.pool, &group_count("event_kind"), hours, &fp, &src)
        .await
        .map_err(err)?;
    let by_source = counts(&st.pool, &group_count("source"), hours, &fp, &src)
        .await
        .map_err(err)?;

    let by_env = counts(&st.pool, &format!("SELECT body->>'environment' AS k, count(*) AS c FROM telemetry.telemetry_events WHERE {win} AND body->>'environment' IS NOT NULL GROUP BY 1 ORDER BY 2 DESC"), hours, &fp, &src).await.map_err(err)?;
    let by_release = counts(&st.pool, &format!("SELECT body->>'release' AS k, count(*) AS c FROM telemetry.telemetry_events WHERE {win} AND body->>'release' IS NOT NULL GROUP BY 1 ORDER BY 2 DESC"), hours, &fp, &src).await.map_err(err)?;

    let bucket = if hours <= 48 { "hour" } else { "day" };
    let series = sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI') AS b, \
           count(*) AS c FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 1"
    )))
    .bind(hours)
    .bind(&fp)
    .bind(&src)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;

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
    let by_status = sqlx::query_as::<_, (Option<String>, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT COALESCE(NULLIF(body->>'status',''),'ok') AS k, count(*) c \
         FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 2 DESC"
    )))
    .bind(hours)
    .bind(&rel)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
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

    let (total_users, crashed_users) = sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT count(DISTINCT body->>'did') AS total_users, \
           count(DISTINCT body->>'did') FILTER (WHERE body->>'status' = 'crashed') AS crashed_users \
         FROM telemetry.telemetry_events WHERE {win}")))
        .bind(hours).bind(&rel).fetch_one(&st.pool).await.map_err(err)?;
    let crash_free_users = if total_users > 0 {
        (1.0 - crashed_users as f64 / total_users as f64) * 100.0
    } else {
        100.0
    };
    let by_release = sqlx::query_as::<_, (Option<String>, i64, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT body->'attrs'->>'release' AS rel, count(*) total, \
           count(*) FILTER (WHERE body->>'status' = 'crashed') bad \
         FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 2 DESC LIMIT 30"
    )))
    .bind(hours)
    .bind(&rel)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    let bucket = if hours <= 48 { "hour" } else { "day" };
    let series = sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI') b, count(*) c \
         FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 1")))
        .bind(hours).bind(&rel).fetch_all(&st.pool).await.map_err(err)?;
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
    .map_err(err)?;
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
            .bind(hours).bind(&event).fetch_all(&st.pool).await.map_err(err)?;
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
    .map_err(err)?;
    Ok(Json(json!({ "prop": prop, "rows": rows.into_iter()
        .map(|(v,c,u)| json!([v.unwrap_or_else(|| "(null)".into()), c, u])).collect::<Vec<_>>() })))
}

const USERKEY: &str =
    "COALESCE(body->'user'->>'id', body->'user'->>'username', body->>'userId', body->>'anonymousId')";

#[derive(sqlx::FromRow, Serialize)]
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
    let anchor: Option<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USERKEY} FROM telemetry.telemetry_events WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .map_err(err)?;
    let user_key = match anchor {
        None => return Err((StatusCode::NOT_FOUND, "no such event".into())),
        Some((uk,)) if !uk.is_empty() => uk,
        Some(_) => return Ok(Json(json!({ "user": null, "utm": null, "events": [] }))),
    };
    let uk_t = USERKEY.replace("body", "t.body");
    let title_t = TITLE1.replace("body", "t.body");
    let events = sqlx::query_as::<_, StoryRow>(sqlx::AssertSqlSafe(format!(
        "SELECT t.id, \
           to_char(t.received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS received_at, \
           t.source, t.event_kind AS kind, t.body->>'level' AS level, {title_t} AS title, \
           (t.id = $2) AS current \
         FROM telemetry.telemetry_events t, \
              (SELECT received_at AS ts FROM telemetry.telemetry_events WHERE id = $2) a \
         WHERE {uk_t} = $1 \
           AND t.received_at BETWEEN a.ts - interval '6 hours' AND a.ts + interval '1 hour' \
         ORDER BY t.received_at LIMIT 200"
    )))
    .bind(&user_key)
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;

    let utm: Option<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COALESCE(body->'context'->'campaign', body->'properties'->'campaign') \
         FROM telemetry.telemetry_events WHERE {USERKEY} = $1 \
           AND COALESCE(body->'context'->'campaign', body->'properties'->'campaign') IS NOT NULL \
         ORDER BY received_at DESC LIMIT 1"
    )))
    .bind(&user_key)
    .fetch_optional(&st.pool)
    .await
    .map_err(err)?
    .flatten();
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
    async fn q(
        pool: &sqlx::PgPool,
        sql: &str,
        hours: i64,
    ) -> Result<Vec<(Option<String>, i64)>, sqlx::Error> {
        sqlx::query_as::<_, (Option<String>, i64)>(sqlx::AssertSqlSafe(sql))
            .bind(hours)
            .fetch_all(pool)
            .await
    }
    let by_event = q(
        &st.pool,
        &format!(
            "SELECT body->>'event' AS k, count(*) AS c FROM telemetry.telemetry_events \
         WHERE {win} AND body->>'event' IS NOT NULL GROUP BY 1 ORDER BY 2 DESC LIMIT 50"
        ),
        hours,
    )
    .await
    .map_err(err)?;
    let by_type = q(&st.pool, &format!(
        "SELECT event_kind AS k, count(*) AS c FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 2 DESC"), hours).await.map_err(err)?;
    let total: i64 = by_type.iter().map(|(_, c)| c).sum();
    let users: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(DISTINCT COALESCE(body->>'userId', body->>'anonymousId')) \
         FROM telemetry.telemetry_events WHERE {win}"
    )))
    .bind(hours)
    .fetch_one(&st.pool)
    .await
    .map_err(err)?;
    let bucket = if hours <= 48 { "hour" } else { "day" };
    let series = sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT to_char(date_trunc('{bucket}', received_at AT TIME ZONE 'UTC'),'YYYY-MM-DD\"T\"HH24:MI') AS b, \
           count(*) AS c FROM telemetry.telemetry_events WHERE {win} GROUP BY 1 ORDER BY 1")))
        .bind(hours).fetch_all(&st.pool).await.map_err(err)?;
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

#[derive(sqlx::FromRow, Serialize)]
struct SessEvent {
    id: i64,
    received_at: String,
    level: Option<String>,
    title: Option<String>,
    kind: String,
}

pub async fn session(
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let anchor = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT body->'user'->>'id', body->'contexts'->'app'->>'app_start_time' \
         FROM telemetry.telemetry_events WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .map_err(err)?;
    let Some((Some(user), app_start)) = anchor else {
        return Ok(Json(json!({ "user": null, "events": [] })));
    };

    let cond = "source='sentry' AND event_kind='event' AND body->'user'->>'id' = $1 \
                AND ($2::text IS NULL OR body->'contexts'->'app'->>'app_start_time' = $2)";
    let events = sqlx::query_as::<_, SessEvent>(sqlx::AssertSqlSafe(format!(
        "SELECT id, {TS} AS received_at, body->>'level' AS level, {TITLE1} AS title, \
           COALESCE(NULLIF(body#>>'{{exception,values,0,type}}',''), event_kind) AS kind \
         FROM telemetry.telemetry_events WHERE {cond} ORDER BY received_at ASC LIMIT 1000"
    )))
    .bind(&user)
    .bind(&app_start)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    let (total, errors, first, last): (i64, i64, Option<String>, Option<String>) =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT count(*), count(*) FILTER (WHERE body->>'level' IN ('error','fatal')), \
               to_char(min(received_at) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), \
               to_char(max(received_at) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
             FROM telemetry.telemetry_events WHERE {cond}"
        )))
        .bind(&user)
        .bind(&app_start)
        .fetch_one(&st.pool)
        .await
        .map_err(err)?;
    let by_level = sqlx::query_as::<_, (Option<String>, i64)>(sqlx::AssertSqlSafe(format!(
        "SELECT COALESCE(body->>'level','(none)'), count(*) FROM telemetry.telemetry_events \
         WHERE {cond} GROUP BY 1 ORDER BY 2 DESC"
    )))
    .bind(&user)
    .bind(&app_start)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    Ok(Json(json!({
        "user": user, "app_start": app_start, "anchor": id,
        "total": total, "errors": errors, "first": first, "last": last,
        "by_level": by_level.into_iter().map(|(k,c)| json!([k.unwrap_or_default(), c])).collect::<Vec<_>>(),
        "events": events,
    })))
}

pub async fn flags(State(st): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let url = std::env::var("FLAGS_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:5137/explorer.json".to_string());
    let config: Value = match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(_) => Value::Null,
    };

    let observed = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT f->>'flag' AS k, count(*) c FROM telemetry.telemetry_events, \
           jsonb_array_elements(CASE WHEN jsonb_typeof(body->'contexts'->'flags'->'values')='array' \
             THEN body->'contexts'->'flags'->'values' ELSE '[]'::jsonb END) f \
         WHERE source='sentry' AND body->'contexts'->'flags'->'values' IS NOT NULL \
         GROUP BY 1 ORDER BY 2 DESC LIMIT 200")
        .fetch_all(&st.pool).await.map_err(err)?;
    Ok(Json(json!({
        "config": config,
        "observed": observed.into_iter().map(|(k,c)| json!([k.unwrap_or_default(), c])).collect::<Vec<_>>(),
        "source_url": url,
    })))
}

#[derive(Deserialize)]
pub struct SqlBody {
    sql: String,
}

pub async fn sql_query(
    State(st): State<AppState>,
    Json(b): Json<SqlBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let raw = b.sql.trim().trim_end_matches(';').trim();
    let low = raw.to_lowercase();
    if !(low.starts_with("select") || low.starts_with("with")) {
        return Err((
            StatusCode::BAD_REQUEST,
            "only SELECT / WITH queries are allowed".into(),
        ));
    }
    if raw.contains(';') {
        return Err((
            StatusCode::BAD_REQUEST,
            "one statement only (no ';')".into(),
        ));
    }
    let wrapped = format!("SELECT to_jsonb(t) AS row FROM ( {raw} ) t LIMIT 1000");
    let mut tx = st.pool.begin().await.map_err(err)?;
    let run = async {
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = 15000")
            .execute(&mut *tx)
            .await?;
        sqlx::query_scalar::<_, Value>(sqlx::AssertSqlSafe(wrapped))
            .fetch_all(&mut *tx)
            .await
    }
    .await;
    let _ = tx.rollback().await;
    let rows = run.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let truncated = rows.len() >= 1000;

    let cols: Vec<String> = rows
        .first()
        .and_then(|r| r.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    Ok(Json(
        json!({ "columns": cols, "rows": rows, "truncated": truncated }),
    ))
}

fn deserialize_some<'de, T, D>(d: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(d).map(Some)
}

#[derive(Deserialize)]
pub struct IssueStateBody {
    fingerprint: String,

    status: Option<String>,

    #[serde(default, deserialize_with = "deserialize_some")]
    assignee: Option<Option<String>>,
    note: Option<String>,
}

pub async fn set_issue_state(
    State(st): State<AppState>,
    Json(b): Json<IssueStateBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if b.fingerprint.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "fingerprint required".into()));
    }
    if let Some(s) = b.status.as_deref() {
        if !matches!(s, "unresolved" | "resolved" | "ignored") {
            return Err((
                StatusCode::BAD_REQUEST,
                "status must be unresolved|resolved|ignored".into(),
            ));
        }
    }

    let assignee_present = b.assignee.is_some();

    let assignee_val = b.assignee.flatten();

    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "INSERT INTO telemetry.issue_state (fingerprint, status, assignee, note, updated_at) \
         VALUES ($1, COALESCE($2, 'unresolved'), $3, $5, now()) \
         ON CONFLICT (fingerprint) DO UPDATE SET \
           status = COALESCE($2, telemetry.issue_state.status), \
           assignee = CASE WHEN $4 THEN $3 ELSE telemetry.issue_state.assignee END, \
           note = COALESCE($5, telemetry.issue_state.note), \
           updated_at = now() \
         RETURNING status, assignee",
    )
    .bind(&b.fingerprint)
    .bind(&b.status)
    .bind(&assignee_val)
    .bind(assignee_present)
    .bind(&b.note)
    .fetch_one(&st.pool)
    .await
    .map_err(err)?;
    Ok(Json(json!({
        "ok": true,
        "fingerprint": b.fingerprint,
        "status": row.0,
        "assignee": row.1,
    })))
}

#[derive(Deserialize)]
pub struct ExperimentsQuery {
    key: Option<String>,
}

pub async fn experiments_get(
    State(st): State<AppState>,
    Query(p): Query<ExperimentsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if let Some(key) = p.key.filter(|v| !v.is_empty()) {
        let row = sqlx::query_as::<_, (bool, Option<String>, Value)>(
            "SELECT killed, forced_variant, flags \
             FROM telemetry.experiment_overrides WHERE exp_key = $1",
        )
        .bind(&key)
        .fetch_optional(&st.pool)
        .await
        .map_err(err)?;
        Ok(Json(match row {
            Some((killed, variant, flags)) => {
                json!({ "killed": killed, "variant": variant, "flags": flags })
            }
            None => json!({}),
        }))
    } else {
        let rows = sqlx::query_as::<_, (String, bool, Option<String>, Value)>(
            "SELECT exp_key, killed, forced_variant, flags \
             FROM telemetry.experiment_overrides ORDER BY exp_key",
        )
        .fetch_all(&st.pool)
        .await
        .map_err(err)?;
        let mut out = serde_json::Map::new();
        for (exp_key, killed, variant, flags) in rows {
            out.insert(
                exp_key,
                json!({ "killed": killed, "variant": variant, "flags": flags }),
            );
        }
        Ok(Json(Value::Object(out)))
    }
}

#[derive(Deserialize)]
pub struct ExperimentSetBody {
    exp_key: String,

    #[serde(default)]
    killed: bool,

    #[serde(default)]
    variant: Option<String>,

    #[serde(default)]
    flags: Option<Value>,

    #[serde(default)]
    clear: bool,
}

pub async fn experiment_set(
    State(st): State<AppState>,
    Json(b): Json<ExperimentSetBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if b.exp_key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "exp_key required".into()));
    }
    if b.clear {
        sqlx::query("DELETE FROM telemetry.experiment_overrides WHERE exp_key = $1")
            .bind(&b.exp_key)
            .execute(&st.pool)
            .await
            .map_err(err)?;
    } else {
        let flags = b.flags.clone().unwrap_or_else(|| json!({}));
        sqlx::query(
            "INSERT INTO telemetry.experiment_overrides (exp_key, killed, forced_variant, flags, updated_at) \
             VALUES ($1, $2, $3, $4, now()) \
             ON CONFLICT (exp_key) DO UPDATE SET \
               killed = $2, forced_variant = $3, flags = $4, updated_at = now()",
        )
        .bind(&b.exp_key)
        .bind(b.killed)
        .bind(&b.variant)
        .bind(flags)
        .execute(&st.pool)
        .await
        .map_err(err)?;
    }
    let action = if b.clear {
        "experiment.clear"
    } else {
        "experiment.set"
    };
    let detail = json!({
        "exp_key": b.exp_key,
        "killed": b.killed,
        "variant": b.variant,
        "flags": b.flags,
        "clear": b.clear,
    });
    crate::handlers::admin::audit(&st, "loopback", action, detail).await;
    Ok(Json(json!({ "ok": true, "exp_key": b.exp_key })))
}

fn err(e: sqlx::Error) -> (StatusCode, String) {
    tracing::error!(error = %e, "telemetry dashboard db error");
    (StatusCode::INTERNAL_SERVER_ERROR, "database error".into())
}
