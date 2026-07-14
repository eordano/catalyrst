use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::db_err;
use crate::AppState;

fn flags_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

#[derive(Deserialize)]
pub struct FlagsQuery {
    pub user: Option<String>,
}

pub async fn flags(
    State(st): State<AppState>,
    Query(p): Query<FlagsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let url = std::env::var("FLAGS_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:5137/explorer.json".to_string());
    let config: Value = match flags_client()
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
        .fetch_all(&st.pool).await.map_err(|e| db_err("telemetry dashboard", e))?;

    let mut overrides = load_flag_overrides(&st.pool)
        .await
        .map_err(|e| db_err("telemetry dashboard", e))?;
    // A matching group target beats the global override, so "off globally,
    // on for internal" resolves the way an operator reads it.
    let user_key = p.user.unwrap_or_default();
    let groups = if user_key.is_empty() {
        Vec::new()
    } else {
        crate::handlers::groups::groups_for_user(&st.pool, &user_key).await
    };
    let targeted = crate::handlers::groups::flag_targets_for(&st.pool, &groups).await;
    for (flag, (state, variant)) in &targeted {
        match overrides.iter_mut().find(|(f, _, _)| f == flag) {
            Some(row) => *row = (flag.clone(), state.clone(), variant.clone()),
            None => overrides.push((flag.clone(), state.clone(), variant.clone())),
        }
    }
    let (overrides_json, merged) = merge_flags(&config, &overrides);
    Ok(Json(json!({
        "config": config,
        "flags": merged,
        "overrides": overrides_json,
        "observed": observed.into_iter().map(|(k,c)| json!([k.unwrap_or_default(), c])).collect::<Vec<_>>(),
        "source_url": url,
        "user": user_key,
        "groups": groups,
        "group_targeted": targeted.keys().collect::<Vec<_>>(),
        "areas": crate::handlers::groups::areas(&st.pool, "flag").await,
    })))
}

type FlagOverride = (String, String, Option<String>);

async fn load_flag_overrides(pool: &sqlx::PgPool) -> Result<Vec<FlagOverride>, sqlx::Error> {
    sqlx::query_as::<_, FlagOverride>(
        "SELECT flag, state, forced_variant FROM telemetry.flag_overrides ORDER BY flag",
    )
    .fetch_all(pool)
    .await
}

// Each merged entry marks `overridden` so the /flags page and dashboard can tell
// an operator-forced value from the upstream one.
fn merge_flags(config: &Value, overrides: &[FlagOverride]) -> (Value, Value) {
    let up_flags = config
        .get("flags")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default();
    let up_variants = config
        .get("variants")
        .and_then(|x| x.as_object())
        .cloned()
        .unwrap_or_default();
    let ov: std::collections::HashMap<&str, (&str, Option<&str>)> = overrides
        .iter()
        .map(|(f, s, v)| (f.as_str(), (s.as_str(), v.as_deref())))
        .collect();

    let mut names: std::collections::BTreeSet<String> = up_flags.keys().cloned().collect();
    for (f, _, _) in overrides {
        names.insert(f.clone());
    }

    let mut merged = serde_json::Map::new();
    for name in &names {
        let key = name.as_str();
        let upstream_present = up_flags.contains_key(key);
        let upstream_value = up_flags.get(key).and_then(|x| x.as_bool());
        let upstream_variant = up_variants
            .get(key)
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let (value, variant, overridden, override_state) = match ov.get(key).copied() {
            Some((state, forced_variant)) => {
                let (v, var) = match state {
                    "off" => (false, upstream_variant.clone()),
                    "forced" => (
                        true,
                        forced_variant
                            .map(|s| s.to_string())
                            .or_else(|| upstream_variant.clone()),
                    ),
                    _ => (true, upstream_variant.clone()),
                };
                (v, var, true, Some(state.to_string()))
            }
            None => (
                upstream_value.unwrap_or(false),
                upstream_variant.clone(),
                false,
                None,
            ),
        };
        let upstream_json = if upstream_present {
            json!(upstream_value.unwrap_or(false))
        } else {
            Value::Null
        };
        merged.insert(
            name.clone(),
            json!({
                "value": value,
                "variant": variant,
                "upstream_value": upstream_json,
                "overridden": overridden,
                "override_state": override_state,
            }),
        );
    }

    let overrides_json: serde_json::Map<String, Value> = overrides
        .iter()
        .map(|(f, s, v)| (f.clone(), json!({ "state": s, "variant": v })))
        .collect();
    (Value::Object(overrides_json), Value::Object(merged))
}

#[derive(Deserialize)]
pub struct FlagSetBody {
    flag: String,

    #[serde(default)]
    state: Option<String>,

    #[serde(default)]
    variant: Option<String>,

    #[serde(default)]
    clear: bool,
}

pub async fn flag_set(
    State(st): State<AppState>,
    Json(b): Json<FlagSetBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if b.flag.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "flag required".into()));
    }
    let state = b.state.clone().unwrap_or_else(|| "on".to_string());
    if !b.clear && !matches!(state.as_str(), "on" | "off" | "forced") {
        return Err((
            StatusCode::BAD_REQUEST,
            "state must be on|off|forced".into(),
        ));
    }
    if b.clear {
        sqlx::query("DELETE FROM telemetry.flag_overrides WHERE flag = $1")
            .bind(&b.flag)
            .execute(&st.pool)
            .await
            .map_err(|e| db_err("telemetry dashboard", e))?;
    } else {
        let variant = b.variant.as_deref().filter(|s| !s.is_empty());
        sqlx::query(
            "INSERT INTO telemetry.flag_overrides (flag, state, forced_variant, updated_at) \
             VALUES ($1, $2, $3, now()) \
             ON CONFLICT (flag) DO UPDATE SET \
               state = $2, forced_variant = $3, updated_at = now()",
        )
        .bind(&b.flag)
        .bind(&state)
        .bind(variant)
        .execute(&st.pool)
        .await
        .map_err(|e| db_err("telemetry dashboard", e))?;
    }
    let action = if b.clear { "flag.clear" } else { "flag.set" };
    let detail = json!({
        "flag": b.flag,
        "state": state,
        "variant": b.variant,
        "clear": b.clear,
    });
    crate::handlers::admin::audit(&st, "loopback", action, detail).await;
    Ok(Json(json!({ "ok": true, "flag": b.flag })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov(flag: &str, state: &str, variant: Option<&str>) -> FlagOverride {
        (flag.into(), state.into(), variant.map(|s| s.into()))
    }

    #[test]
    fn merge_forces_value_and_marks_override() {
        let config = json!({ "flags": { "a": false, "b": true } });
        let (overrides, merged) = merge_flags(&config, &[ov("a", "on", None)]);

        assert_eq!(merged["a"]["value"], json!(true));
        assert_eq!(merged["a"]["overridden"], json!(true));
        assert_eq!(merged["a"]["override_state"], json!("on"));
        assert_eq!(merged["a"]["upstream_value"], json!(false));

        assert_eq!(merged["b"]["value"], json!(true));
        assert_eq!(merged["b"]["overridden"], json!(false));

        assert_eq!(overrides["a"]["state"], json!("on"));
    }

    #[test]
    fn merge_off_forces_false() {
        let config = json!({ "flags": { "a": true } });
        let (_, merged) = merge_flags(&config, &[ov("a", "off", None)]);
        assert_eq!(merged["a"]["value"], json!(false));
        assert_eq!(merged["a"]["override_state"], json!("off"));
    }

    #[test]
    fn merge_forced_pins_variant() {
        let config = json!({ "flags": { "a": true }, "variants": { "a": { "name": "control" } } });
        let (_, merged) = merge_flags(&config, &[ov("a", "forced", Some("guided"))]);
        assert_eq!(merged["a"]["value"], json!(true));
        assert_eq!(merged["a"]["variant"], json!("guided"));
        assert_eq!(merged["a"]["override_state"], json!("forced"));
    }

    #[test]
    fn merge_surfaces_override_only_flag() {
        let config = json!({ "flags": { "a": true } });
        let (_, merged) = merge_flags(&config, &[ov("z", "on", None)]);
        assert!(merged.get("z").is_some());
        assert_eq!(merged["z"]["upstream_value"], Value::Null);
        assert_eq!(merged["z"]["overridden"], json!(true));
    }
}
