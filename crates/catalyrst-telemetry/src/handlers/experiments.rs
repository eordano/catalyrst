use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::handlers::dashboard;
use crate::AppState;

use super::db_err;

// SQL math must stay identical to the deploy tree's metabase
// experiment-readout query (the Metabase cards + story-readout.ts share it);
// port changes there first.

const SYNTHETIC_PREFIXES: [&str; 4] = [
    "readout_probe_",
    "verify_readout_",
    "verify_throwaway",
    "pm_smoke",
];

#[derive(Deserialize)]
pub struct ListQuery {
    key: Option<String>,

    #[serde(default)]
    user: Option<String>,
}

#[derive(Default)]
struct ExpAgg {
    exposures: i64,
    variants: Vec<String>,
    metrics: BTreeMap<String, i64>,
}

pub async fn list(
    State(st): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if q.key.as_deref().is_some_and(|k| !k.is_empty()) {
        return dashboard::experiments_get(
            State(st),
            Query(dashboard::ExperimentsQuery {
                key: q.key,
                user: q.user,
            }),
        )
        .await;
    }
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
        "SELECT e.body->'properties'->>'exp_key' AS exp_key, \
                e.body->'properties'->>'variant' AS variant, \
                e.body->>'event' AS event, \
                count(*) AS n \
         FROM telemetry.telemetry_events e \
         WHERE e.source = 'segment' \
           AND e.body->'properties'->>'exp_key' IS NOT NULL \
         GROUP BY 1, 2, 3",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| db_err("telemetry experiments", e))?;

    let mut agg: BTreeMap<String, ExpAgg> = BTreeMap::new();
    for (exp_key, variant, event, n) in rows {
        let a = agg.entry(exp_key).or_default();
        if let Some(v) = variant {
            if !a.variants.contains(&v) {
                a.variants.push(v);
            }
        }
        match event.as_deref() {
            Some("experiment_exposed") => a.exposures += n,
            Some(ev) => *a.metrics.entry(ev.to_string()).or_insert(0) += n,
            None => {}
        }
    }

    let mut experiments = Vec::new();
    let mut unreadable = Vec::new();
    for (exp_key, mut a) in agg {
        a.variants.sort();
        let reason = if SYNTHETIC_PREFIXES.iter().any(|p| exp_key.starts_with(p)) {
            Some("synthetic key (matches exclusion pattern)")
        } else if a.exposures == 0 {
            Some("no experiment_exposed events")
        } else if a.variants.len() < 2 {
            Some("fewer than 2 variants")
        } else {
            None
        };
        let entry = json!({
            "exp_key": exp_key,
            "exposures": a.exposures,
            "variants": a.variants,
            "metrics": a.metrics.iter()
                .map(|(event, count)| json!({ "event": event, "count": count }))
                .collect::<Vec<_>>(),
        });
        match reason {
            Some(r) => {
                let mut e = entry;
                e["reason"] = json!(r);
                unreadable.push(e);
            }
            None => {
                let control = if a.variants.iter().any(|v| v == "control") {
                    "control".to_string()
                } else {
                    a.variants[0].clone()
                };
                let mut e = entry;
                e["control"] = json!(control);
                experiments.push(e);
            }
        }
    }
    Ok(Json(
        json!({ "experiments": experiments, "unreadable": unreadable }),
    ))
}

#[derive(Deserialize)]
pub struct ReadoutQuery {
    exp_key: String,
    metric: String,
    control: String,
    alpha: Option<f64>,
    min_sample: Option<f64>,
}

#[derive(sqlx::FromRow, Serialize)]
struct ReadoutRow {
    variant: String,
    n_exposures: i64,
    successes: i64,
    rate: f64,
    control_rate: f64,
    diff: f64,
    z: f64,
    p_value: f64,
    significant: bool,
    bayes_mean: f64,
    bayes_control_mean: f64,
    bayes_ci_low: f64,
    bayes_ci_high: f64,
    p_beats_control: f64,
    verdict: String,
}

const READOUT_SQL: &str = r#"
WITH params AS (
  SELECT
    $1::text                                AS exp_key,
    $2::text                                AS metric,
    $3::text                                AS control,
    COALESCE($4::double precision, 0.05)    AS alpha,
    COALESCE($5::double precision, 0)       AS min_sample
),
cfg AS (
  SELECT
    exp_key, control, alpha, min_sample,
    CASE WHEN metric LIKE '%\_rate' THEN left(metric, length(metric) - 5)
         ELSE metric END AS num_event
  FROM params
),
scoped AS (
  SELECT
    e.body->'properties'->>'variant' AS variant,
    e.body->>'event'                 AS event,
    COALESCE(e.body->'user'->>'id', e.body->'user'->>'username',
             e.body->>'userId', e.body->>'anonymousId') AS user_key
  FROM telemetry.telemetry_events e, cfg
  WHERE e.source = 'segment'
    AND e.body->'properties'->>'exp_key' = cfg.exp_key
    AND e.body->'properties'->>'variant' IS NOT NULL
    AND COALESCE(e.body->'user'->>'id', e.body->'user'->>'username',
                 e.body->>'userId', e.body->>'anonymousId') IS NOT NULL
),
per_variant AS (
  SELECT
    s.variant,
    count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed') AS n_exposures,
    count(DISTINCT s.user_key) FILTER (WHERE s.event = (SELECT num_event FROM cfg)) AS successes
  FROM scoped s
  GROUP BY s.variant
),
control_arm AS (
  SELECT pv.n_exposures AS c_n, pv.successes AS c_x
  FROM per_variant pv, cfg
  WHERE pv.variant = cfg.control
)
SELECT
  pv.variant,
  pv.n_exposures::bigint                                              AS n_exposures,
  pv.successes::bigint                                                AS successes,
  CASE WHEN pv.n_exposures > 0 THEN pv.successes::double precision / pv.n_exposures ELSE 0 END AS rate,
  CASE WHEN c.c_n > 0 THEN c.c_x::double precision / c.c_n ELSE 0 END AS control_rate,
  (CASE WHEN pv.n_exposures > 0 THEN pv.successes::double precision / pv.n_exposures ELSE 0 END)
    - (CASE WHEN c.c_n > 0 THEN c.c_x::double precision / c.c_n ELSE 0 END)            AS diff,
  ext.two_prop_z(c.c_x, c.c_n, pv.successes, pv.n_exposures)          AS z,
  ext.two_prop_p(c.c_x, c.c_n, pv.successes, pv.n_exposures)          AS p_value,
  (ext.two_prop_p(c.c_x, c.c_n, pv.successes, pv.n_exposures) < cfg.alpha) AS significant,
  ext.beta_post_mean(pv.successes, pv.n_exposures)                    AS bayes_mean,
  ext.beta_post_mean(c.c_x, c.c_n)                                    AS bayes_control_mean,
  greatest(0.0, ext.beta_post_mean(pv.successes, pv.n_exposures)
    - 1.96 * sqrt(ext.beta_post_var(pv.successes, pv.n_exposures)))   AS bayes_ci_low,
  least(1.0, ext.beta_post_mean(pv.successes, pv.n_exposures)
    + 1.96 * sqrt(ext.beta_post_var(pv.successes, pv.n_exposures)))   AS bayes_ci_high,
  ext.p_beats(c.c_x, c.c_n, pv.successes, pv.n_exposures)             AS p_beats_control,
  CASE
    WHEN ext.two_prop_p(c.c_x, c.c_n, pv.successes, pv.n_exposures) < cfg.alpha
         AND pv.successes::double precision / nullif(pv.n_exposures,0)
             - c.c_x::double precision / nullif(c.c_n,0) > 0
         AND least(pv.n_exposures, c.c_n) >= cfg.min_sample
      THEN 'SHIP'
    WHEN ext.two_prop_p(c.c_x, c.c_n, pv.successes, pv.n_exposures) < cfg.alpha
         AND pv.successes::double precision / nullif(pv.n_exposures,0)
             - c.c_x::double precision / nullif(c.c_n,0) < 0
      THEN 'KILL'
    ELSE 'KEEP RUNNING'
  END                                                                AS verdict
FROM per_variant pv
CROSS JOIN cfg
CROSS JOIN control_arm c
WHERE pv.variant <> cfg.control
ORDER BY pv.variant
"#;

pub async fn readout(
    State(st): State<AppState>,
    Query(p): Query<ReadoutQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, ReadoutRow>(READOUT_SQL)
        .bind(&p.exp_key)
        .bind(&p.metric)
        .bind(&p.control)
        .bind(p.alpha)
        .bind(p.min_sample)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| db_err("telemetry experiments", e))?;
    Ok(Json(json!({
        "exp_key": p.exp_key,
        "metric": p.metric,
        "control": p.control,
        "alpha": p.alpha.unwrap_or(0.05),
        "min_sample": p.min_sample.unwrap_or(0.0),
        "rows": rows,
    })))
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    exp_key: String,
    metric: String,
}

#[derive(sqlx::FromRow, Serialize)]
struct TimeseriesRow {
    day: String,
    variant: String,
    exposures: i64,
    conversions: i64,
}

const TIMESERIES_SQL: &str = r#"
WITH cfg AS (
  SELECT
    $1::text AS exp_key,
    CASE WHEN $2::text LIKE '%\_rate'
         THEN left($2::text, length($2::text) - 5)
         ELSE $2::text END AS num_event
)
SELECT
  to_char(e.received_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')             AS day,
  e.body->'properties'->>'variant'                                    AS variant,
  count(DISTINCT (COALESCE(e.body->'user'->>'id', e.body->'user'->>'username',
                 e.body->>'userId', e.body->>'anonymousId')))
    FILTER (WHERE e.body->>'event' = 'experiment_exposed')            AS exposures,
  count(DISTINCT (COALESCE(e.body->'user'->>'id', e.body->'user'->>'username',
                 e.body->>'userId', e.body->>'anonymousId')))
    FILTER (WHERE e.body->>'event' = (SELECT num_event FROM cfg))     AS conversions
FROM telemetry.telemetry_events e, cfg
WHERE e.source = 'segment'
  AND e.body->'properties'->>'exp_key' = cfg.exp_key
  AND e.body->'properties'->>'variant' IS NOT NULL
GROUP BY 1, 2
ORDER BY 1, 2
"#;

pub async fn timeseries(
    State(st): State<AppState>,
    Query(p): Query<SeriesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, TimeseriesRow>(TIMESERIES_SQL)
        .bind(&p.exp_key)
        .bind(&p.metric)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| db_err("telemetry experiments", e))?;
    Ok(Json(
        json!({ "exp_key": p.exp_key, "metric": p.metric, "rows": rows }),
    ))
}

#[derive(sqlx::FromRow, Serialize)]
struct RateRow {
    variant: String,
    exposures: i64,
    successes: i64,
    rate: f64,
}

const RATES_SQL: &str = r#"
WITH cfg AS (
  SELECT
    $1::text AS exp_key,
    CASE WHEN $2::text LIKE '%\_rate'
         THEN left($2::text, length($2::text) - 5)
         ELSE $2::text END AS num_event
),
scoped AS (
  SELECT
    e.body->'properties'->>'variant' AS variant,
    e.body->>'event'                 AS event,
    COALESCE(e.body->'user'->>'id', e.body->'user'->>'username',
             e.body->>'userId', e.body->>'anonymousId') AS user_key
  FROM telemetry.telemetry_events e, cfg
  WHERE e.source = 'segment'
    AND e.body->'properties'->>'exp_key' = cfg.exp_key
    AND e.body->'properties'->>'variant' IS NOT NULL
)
SELECT
  s.variant,
  count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed') AS exposures,
  count(DISTINCT s.user_key) FILTER (WHERE s.event = (SELECT num_event FROM cfg)) AS successes,
  CASE WHEN count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed') > 0
       THEN count(DISTINCT s.user_key) FILTER (WHERE s.event = (SELECT num_event FROM cfg))::double precision
            / count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed')
       ELSE 0 END AS rate
FROM scoped s
GROUP BY s.variant
ORDER BY s.variant
"#;

pub async fn rates(
    State(st): State<AppState>,
    Query(p): Query<SeriesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, RateRow>(RATES_SQL)
        .bind(&p.exp_key)
        .bind(&p.metric)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| db_err("telemetry experiments", e))?;
    Ok(Json(
        json!({ "exp_key": p.exp_key, "metric": p.metric, "rows": rows }),
    ))
}
