use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::audited;
use crate::handlers::db_err;
use crate::AppState;

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
    let mut tx = st
        .pool
        .begin_with("BEGIN READ ONLY; SET LOCAL statement_timeout = 15000")
        .await
        .map_err(|e| db_err("telemetry dashboard", e))?;
    let run = sqlx::query_scalar::<_, Value>(sqlx::AssertSqlSafe(wrapped))
        .fetch_all(&mut *tx)
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
    .map_err(|e| db_err("telemetry dashboard", e))?;
    Ok(Json(json!({
        "ok": true,
        "fingerprint": b.fingerprint,
        "status": row.0,
        "assignee": row.1,
    })))
}

#[derive(Deserialize)]
pub struct ExperimentsQuery {
    pub key: Option<String>,

    #[serde(default)]
    pub user: Option<String>,
}

pub async fn experiments_get(
    State(st): State<AppState>,
    Query(p): Query<ExperimentsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if let Some(key) = p.key.filter(|v| !v.is_empty()) {
        let user_key = p.user.unwrap_or_default();
        let (groups, targets, row): (Value, Value, Option<Value>) =
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT CASE WHEN $2::text = '' THEN '[]'::jsonb ELSE {groups_json} END AS groups, \
                   (SELECT COALESCE(jsonb_agg(jsonb_build_array(group_name, killed, forced_variant, flags)), '[]'::jsonb) \
                      FROM telemetry.experiment_group_targets WHERE exp_key = $1 AND $2::text <> '') AS targets, \
                   (SELECT jsonb_build_array(killed, forced_variant, flags) \
                      FROM telemetry.experiment_overrides WHERE exp_key = $1) AS override",
                groups_json = crate::handlers::groups::GROUPS_JSON,
            )))
            .bind(&key)
            .bind(&user_key)
            .fetch_one(&st.pool)
            .await
            .map_err(|e| db_err("telemetry dashboard", e))?;
        let decode_err =
            |e: serde_json::Error| db_err("telemetry dashboard", sqlx::Error::decode(e));
        let groups = crate::handlers::groups::groups_for(
            serde_json::from_value(groups).map_err(decode_err)?,
            &user_key,
        );
        let targets: Vec<(String, bool, Option<String>, Value)> =
            serde_json::from_value(targets).map_err(decode_err)?;
        for name in &groups {
            if let Some((_, killed, variant, flags)) = targets.iter().find(|r| &r.0 == name) {
                return Ok(Json(
                    json!({ "killed": killed, "variant": variant, "flags": flags }),
                ));
            }
        }
        let row: Option<(bool, Option<String>, Value)> = match row {
            Some(v) => Some(serde_json::from_value(v).map_err(decode_err)?),
            None => None,
        };
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
        .map_err(|e| db_err("telemetry dashboard", e))?;
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
    if b.clear {
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "DELETE FROM telemetry.experiment_overrides WHERE exp_key = $1",
            1,
        )))
        .bind(&b.exp_key)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(|e| db_err("telemetry dashboard", e))?;
    } else {
        let flags = b.flags.clone().unwrap_or_else(|| json!({}));
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "INSERT INTO telemetry.experiment_overrides (exp_key, killed, forced_variant, flags, updated_at) \
             VALUES ($1, $2, $3, $4, now()) \
             ON CONFLICT (exp_key) DO UPDATE SET \
               killed = $2, forced_variant = $3, flags = $4, updated_at = now()",
            4,
        )))
        .bind(&b.exp_key)
        .bind(b.killed)
        .bind(&b.variant)
        .bind(flags)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(|e| db_err("telemetry dashboard", e))?;
    }
    Ok(Json(json!({ "ok": true, "exp_key": b.exp_key })))
}
