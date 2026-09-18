use std::collections::BTreeMap;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::handlers::dashboard;
use crate::AppState;

use super::db_err;

const SYNTHETIC_PREFIXES: [&str; 4] = [
    "readout_probe_",
    "verify_readout_",
    "verify_throwaway",
    "pm_smoke",
];

/// How long one `GET /experiments` aggregate is served from memory. The route
/// is polled every ~30 s (5,992 calls in 50 h), each a scan of every segment
/// event; a 60 s memo caps that at one scan per minute per process, however
/// many pollers there are.
pub const LIST_TTL: Duration = Duration::from_secs(60);

/// Predicate and grouped expressions match `migrations/0010_experiments_index.sql`
/// verbatim so the partial index serves the scan.
pub const LIST_SQL: &str = "SELECT e.body->'properties'->>'exp_key' AS exp_key, \
            e.body->'properties'->>'variant' AS variant, \
            e.body->>'event' AS event, \
            count(*) AS n \
     FROM telemetry.telemetry_events e \
     WHERE e.source = 'segment' \
       AND e.body->'properties'->>'exp_key' IS NOT NULL \
     GROUP BY 1, 2, 3";

type ListRow = (String, Option<String>, Option<String>, i64);

#[derive(Deserialize)]
pub struct ListQuery {
    key: Option<String>,

    #[serde(default)]
    user: Option<String>,

    #[serde(default)]
    fresh: Option<String>,
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
    let pool = &st.pool;
    let body = st
        .experiments_cache
        .get_or_refresh(cache_ttl(q.fresh.as_deref()), move || async move {
            sqlx::query_as::<_, ListRow>(LIST_SQL)
                .fetch_all(pool)
                .await
                .map(aggregate)
        })
        .await
        .map_err(|e| db_err("telemetry experiments", e))?;
    Ok(Json(body))
}

/// `fresh=1` (or `true`) turns the memo off for that call: a zero TTL makes the
/// cell reload unconditionally, still single-flight, and the reload refreshes
/// the slot for everyone else.
fn cache_ttl(fresh: Option<&str>) -> Duration {
    match fresh {
        Some("1" | "true") => Duration::ZERO,
        _ => LIST_TTL,
    }
}

fn aggregate(rows: Vec<ListRow>) -> Value {
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
    json!({ "experiments": experiments, "unreadable": unreadable })
}

#[derive(Deserialize)]
pub struct ReadoutQuery {
    exp_key: String,
    metric: String,
    control: String,
    alpha: Option<f64>,
    min_sample: Option<f64>,

    #[serde(default)]
    fresh: Option<String>,
}

/// `(exp_key, metric, control, alpha bits, min_sample bits)`.
pub type ReadoutKey = (String, String, String, Option<u64>, Option<u64>);

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct ReadoutRow {
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
    let key: ReadoutKey = (
        p.exp_key.clone(),
        p.metric.clone(),
        p.control.clone(),
        p.alpha.map(f64::to_bits),
        p.min_sample.map(f64::to_bits),
    );
    if cache_ttl(p.fresh.as_deref()).is_zero() {
        st.readout_cache.invalidate(&key);
    }
    let pool = &st.pool;
    let rows = st
        .readout_cache
        .get_or_fetch(key, || async {
            sqlx::query_as::<_, ReadoutRow>(READOUT_SQL)
                .bind(&p.exp_key)
                .bind(&p.metric)
                .bind(&p.control)
                .bind(p.alpha)
                .bind(p.min_sample)
                .fetch_all(pool)
                .await
        })
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

    #[serde(default)]
    fresh: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TimeseriesRow {
    day: String,
    variant: String,
    exposures: i64,
    conversions: i64,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RateRow {
    variant: String,
    exposures: i64,
    successes: i64,
    rate: f64,
}

/// `/timeseries` and `/rates` read the same scoped scan, so one statement computes both and
/// the memo serves whichever the dashboard asks for second.
#[derive(Clone)]
pub struct SeriesBundle {
    pub timeseries: Vec<TimeseriesRow>,
    pub rates: Vec<RateRow>,
}

const SERIES_SQL: &str = r#"
WITH cfg AS (
  SELECT
    $1::text AS exp_key,
    CASE WHEN $2::text LIKE '%\_rate'
         THEN left($2::text, length($2::text) - 5)
         ELSE $2::text END AS num_event
),
scoped AS (
  SELECT
    e.received_at,
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
  (SELECT COALESCE(jsonb_agg(jsonb_build_object(
      'day', t.day, 'variant', t.variant, 'exposures', t.exposures, 'conversions', t.conversions)
      ORDER BY t.day, t.variant), '[]'::jsonb)
   FROM (
     SELECT
       to_char(s.received_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day,
       s.variant,
       count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed') AS exposures,
       count(DISTINCT s.user_key) FILTER (WHERE s.event = (SELECT num_event FROM cfg)) AS conversions
     FROM scoped s
     GROUP BY 1, 2
   ) t) AS timeseries,
  (SELECT COALESCE(jsonb_agg(jsonb_build_object(
      'variant', r.variant, 'exposures', r.exposures, 'successes', r.successes,
      'rate', CASE WHEN r.exposures > 0 THEN r.successes::double precision / r.exposures ELSE 0 END)
      ORDER BY r.variant), '[]'::jsonb)
   FROM (
     SELECT
       s.variant,
       count(DISTINCT s.user_key) FILTER (WHERE s.event = 'experiment_exposed') AS exposures,
       count(DISTINCT s.user_key) FILTER (WHERE s.event = (SELECT num_event FROM cfg)) AS successes
     FROM scoped s
     GROUP BY s.variant
   ) r) AS rates
"#;

async fn series(st: &AppState, p: &SeriesQuery) -> Result<SeriesBundle, (StatusCode, String)> {
    let key = (p.exp_key.clone(), p.metric.clone());
    if cache_ttl(p.fresh.as_deref()).is_zero() {
        st.series_cache.invalidate(&key);
    }
    let pool = &st.pool;
    st.series_cache
        .get_or_fetch(key, || async {
            let (timeseries, rates): (Value, Value) = sqlx::query_as(SERIES_SQL)
                .bind(&p.exp_key)
                .bind(&p.metric)
                .fetch_one(pool)
                .await?;
            Ok(SeriesBundle {
                timeseries: serde_json::from_value(timeseries).map_err(sqlx::Error::decode)?,
                rates: serde_json::from_value(rates).map_err(sqlx::Error::decode)?,
            })
        })
        .await
        .map_err(|e| db_err("telemetry experiments", e))
}

pub async fn timeseries(
    State(st): State<AppState>,
    Query(p): Query<SeriesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = series(&st, &p).await?.timeseries;
    Ok(Json(
        json!({ "exp_key": p.exp_key, "metric": p.metric, "rows": rows }),
    ))
}

pub async fn rates(
    State(st): State<AppState>,
    Query(p): Query<SeriesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = series(&st, &p).await?.rates;
    Ok(Json(
        json!({ "exp_key": p.exp_key, "metric": p.metric, "rows": rows }),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::Arc;

    use catalyrst_commons::cache::TtlCell;

    use super::*;

    fn row(key: &str, variant: Option<&str>, event: Option<&str>, n: i64) -> ListRow {
        (
            key.to_string(),
            variant.map(str::to_string),
            event.map(str::to_string),
            n,
        )
    }

    fn counting(
        loads: &AtomicUsize,
    ) -> impl Fn() -> std::future::Ready<Result<Value, String>> + '_ {
        move || std::future::ready(Ok(json!(loads.fetch_add(1, SeqCst) + 1)))
    }

    #[test]
    fn aggregate_keeps_the_response_shape() {
        let out = aggregate(vec![
            row("hud_cta", Some("control"), Some("experiment_exposed"), 40),
            row("hud_cta", Some("blue"), Some("experiment_exposed"), 42),
            row("hud_cta", Some("blue"), Some("cta_clicked"), 7),
            row("hud_cta", Some("control"), Some("cta_clicked"), 5),
            row("hud_cta", None, None, 1),
            row("no_control", Some("b"), Some("experiment_exposed"), 3),
            row("no_control", Some("a"), Some("experiment_exposed"), 3),
            row(
                "readout_probe_x",
                Some("control"),
                Some("experiment_exposed"),
                9,
            ),
            row("single", Some("control"), Some("experiment_exposed"), 9),
            row("unexposed", Some("a"), Some("clicked"), 9),
            row("unexposed", Some("b"), Some("clicked"), 9),
        ]);
        assert_eq!(
            out,
            json!({
                "experiments": [
                    {"exp_key": "hud_cta", "exposures": 82, "variants": ["blue", "control"],
                     "metrics": [{"event": "cta_clicked", "count": 12}], "control": "control"},
                    {"exp_key": "no_control", "exposures": 6, "variants": ["a", "b"],
                     "metrics": [], "control": "a"},
                ],
                "unreadable": [
                    {"exp_key": "readout_probe_x", "exposures": 9, "variants": ["control"],
                     "metrics": [], "reason": "synthetic key (matches exclusion pattern)"},
                    {"exp_key": "single", "exposures": 9, "variants": ["control"],
                     "metrics": [], "reason": "fewer than 2 variants"},
                    {"exp_key": "unexposed", "exposures": 0, "variants": ["a", "b"],
                     "metrics": [{"event": "clicked", "count": 18}],
                     "reason": "no experiment_exposed events"},
                ]
            })
        );
    }

    #[test]
    fn list_sql_matches_the_partial_index_definition() {
        const MIGRATION: &str = include_str!("../../migrations/0010_experiments_index.sql");
        for needle in [
            "source = 'segment'",
            "body->'properties'->>'exp_key' IS NOT NULL",
            "body->'properties'->>'exp_key'",
            "body->'properties'->>'variant'",
            "body->>'event'",
        ] {
            assert!(LIST_SQL.contains(needle), "query lacks {needle}");
            assert!(MIGRATION.contains(needle), "index lacks {needle}");
        }
    }

    #[test]
    fn fresh_param_zeroes_the_ttl() {
        assert_eq!(cache_ttl(None), LIST_TTL);
        assert_eq!(cache_ttl(Some("0")), LIST_TTL);
        assert_eq!(cache_ttl(Some("1")), Duration::ZERO);
        assert_eq!(cache_ttl(Some("true")), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn polls_within_the_ttl_share_one_aggregate() {
        let cell = TtlCell::<Value>::new("t");
        let loads = AtomicUsize::new(0);
        let load = counting(&loads);

        let a = cell.get_or_refresh(cache_ttl(None), &load).await.unwrap();
        let b = cell.get_or_refresh(cache_ttl(None), &load).await.unwrap();
        assert_eq!((a, b), (json!(1), json!(1)));

        tokio::time::advance(LIST_TTL - Duration::from_secs(1)).await;
        assert_eq!(
            cell.get_or_refresh(cache_ttl(None), &load).await.unwrap(),
            json!(1)
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            cell.get_or_refresh(cache_ttl(None), &load).await.unwrap(),
            json!(2)
        );
        assert_eq!(loads.load(SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn fresh_reloads_and_refreshes_the_memo() {
        let cell = TtlCell::<Value>::new("t");
        let loads = AtomicUsize::new(0);
        let load = counting(&loads);

        assert_eq!(
            cell.get_or_refresh(cache_ttl(None), &load).await.unwrap(),
            json!(1)
        );
        assert_eq!(
            cell.get_or_refresh(cache_ttl(Some("1")), &load)
                .await
                .unwrap(),
            json!(2)
        );
        assert_eq!(
            cell.get_or_refresh(cache_ttl(None), &load).await.unwrap(),
            json!(2)
        );
        assert_eq!(loads.load(SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_polls_do_not_stampede() {
        let cell = Arc::new(TtlCell::<Value>::new("t"));
        let loads = Arc::new(AtomicUsize::new(0));
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let cell = cell.clone();
                let loads = loads.clone();
                tokio::spawn(async move {
                    cell.get_or_refresh(cache_ttl(None), move || async move {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok::<_, String>(json!(loads.fetch_add(1, SeqCst) + 1))
                    })
                    .await
                    .unwrap()
                })
            })
            .collect();
        for t in tasks {
            assert_eq!(t.await.unwrap(), json!(1));
        }
        assert_eq!(loads.load(SeqCst), 1);
    }
}
