use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::audited;
use crate::AppState;

fn err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(sqlx::FromRow, Clone, Deserialize)]
pub struct Group {
    pub name: String,
    pub description: String,
    pub members: Vec<String>,
    pub rollout_pct: i32,
    pub priority: i32,
}

fn bucket(group: &str, user_key: &str) -> u32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in group
        .bytes()
        .chain(b":".iter().copied())
        .chain(user_key.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    (hash % 100) as u32
}

pub fn matches(group: &Group, user_key: &str) -> bool {
    if group.members.iter().any(|m| m == user_key) {
        return true;
    }
    group.rollout_pct > 0 && bucket(&group.name, user_key) < group.rollout_pct as u32
}

async fn load_groups(pool: &sqlx::PgPool) -> Result<Vec<Group>, sqlx::Error> {
    sqlx::query_as::<_, Group>(
        "SELECT name, description, members, rollout_pct, priority \
         FROM telemetry.flag_groups ORDER BY priority DESC, name",
    )
    .fetch_all(pool)
    .await
}

/// `flag_groups` as one jsonb column, in `load_groups` order, for folding into a wider statement.
pub const GROUPS_JSON: &str = "(SELECT COALESCE(jsonb_agg(jsonb_build_object( \
      'name', name, 'description', description, 'members', to_jsonb(members), \
      'rollout_pct', rollout_pct, 'priority', priority) ORDER BY priority DESC, name), '[]'::jsonb) \
    FROM telemetry.flag_groups)";

pub fn groups_for(groups: Vec<Group>, user_key: &str) -> Vec<String> {
    if user_key.is_empty() {
        return Vec::new();
    }
    groups
        .into_iter()
        .filter(|g| matches(g, user_key))
        .map(|g| g.name)
        .collect()
}

pub type FlagTarget = (String, String, String, Option<String>);

/// Highest-priority group (first in `groups`) wins each flag.
pub fn rank_flag_targets(
    rows: Vec<FlagTarget>,
    groups: &[String],
) -> std::collections::HashMap<String, (String, Option<String>)> {
    let mut out = std::collections::HashMap::new();
    if groups.is_empty() {
        return out;
    }
    let rank: std::collections::HashMap<&str, usize> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.as_str(), i))
        .collect();
    let mut winner: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (group_name, flag, state, variant) in rows {
        let r = match rank.get(group_name.as_str()) {
            Some(r) => *r,
            None => continue,
        };
        if winner.get(&flag).is_some_and(|best| *best <= r) {
            continue;
        }
        winner.insert(flag.clone(), r);
        out.insert(flag, (state, variant));
    }
    out
}

fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, (StatusCode, String)> {
    serde_json::from_value(v).map_err(err)
}

pub async fn list(State(st): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let (groups, flag_targets, exp_targets, area_rows): (Value, Value, Value, Value) =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {GROUPS_JSON} AS groups, \
               (SELECT COALESCE(jsonb_agg(jsonb_build_array(group_name, flag, state, forced_variant) \
                  ORDER BY group_name, flag), '[]'::jsonb) FROM telemetry.flag_group_targets) AS flag_targets, \
               (SELECT COALESCE(jsonb_agg(jsonb_build_array(group_name, exp_key, killed, forced_variant) \
                  ORDER BY group_name, exp_key), '[]'::jsonb) FROM telemetry.experiment_group_targets) AS exp_targets, \
               (SELECT COALESCE(jsonb_agg(jsonb_build_array(kind, name, area) ORDER BY kind, name), '[]'::jsonb) \
                  FROM telemetry.product_areas) AS areas"
        )))
        .fetch_one(&st.pool)
        .await
        .map_err(err)?;
    let groups: Vec<Group> = decode(groups)?;
    let flag_targets: Vec<(String, String, String, Option<String>)> = decode(flag_targets)?;
    let exp_targets: Vec<(String, String, bool, Option<String>)> = decode(exp_targets)?;
    let area_rows: Vec<(String, String, String)> = decode(area_rows)?;

    Ok(Json(json!({
        "groups": groups.iter().map(|g| json!({
            "name": g.name,
            "description": g.description,
            "members": g.members,
            "member_count": g.members.len(),
            "rollout_pct": g.rollout_pct,
            "priority": g.priority,
            "flag_targets": flag_targets.iter().filter(|t| t.0 == g.name)
                .map(|(_, flag, state, variant)| json!({
                    "flag": flag, "state": state, "variant": variant
                })).collect::<Vec<_>>(),
            "experiment_targets": exp_targets.iter().filter(|t| t.0 == g.name)
                .map(|(_, exp_key, killed, variant)| json!({
                    "exp_key": exp_key, "killed": killed, "variant": variant
                })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "areas": area_rows.iter().map(|(kind, name, area)| json!({
            "kind": kind, "name": name, "area": area
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct ResolveQuery {
    user: Option<String>,
}

pub async fn resolve(
    State(st): State<AppState>,
    Query(p): Query<ResolveQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_key = p.user.unwrap_or_default();
    if user_key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "user required".into()));
    }
    let groups = load_groups(&st.pool).await.map_err(err)?;
    Ok(Json(json!({
        "user": user_key,
        "groups": groups.iter().filter(|g| matches(g, &user_key)).map(|g| json!({
            "name": g.name,
            "priority": g.priority,
            "via": if g.members.iter().any(|m| m == &user_key) { "member" } else { "rollout" },
        })).collect::<Vec<_>>(),
        "bucket": groups.iter().map(|g| json!({
            "group": g.name, "bucket": bucket(&g.name, &user_key), "rollout_pct": g.rollout_pct
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct GroupSetBody {
    name: String,

    #[serde(default)]
    description: Option<String>,

    #[serde(default)]
    members: Option<Vec<String>>,

    #[serde(default)]
    rollout_pct: Option<i32>,

    #[serde(default)]
    priority: Option<i32>,

    #[serde(default)]
    clear: bool,
}

pub async fn set(
    State(st): State<AppState>,
    Json(b): Json<GroupSetBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if b.name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name required".into()));
    }
    if let Some(pct) = b.rollout_pct {
        if !(0..=100).contains(&pct) {
            return Err((StatusCode::BAD_REQUEST, "rollout_pct must be 0..100".into()));
        }
    }
    let action = if b.clear { "group.clear" } else { "group.set" };
    let detail = json!({
        "name": b.name,
        "members": b.members.as_ref().map(|m| m.len()),
        "rollout_pct": b.rollout_pct,
        "priority": b.priority,
    });
    if b.clear {
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "DELETE FROM telemetry.flag_groups WHERE name = $1",
            1,
        )))
        .bind(&b.name)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(err)?;
    } else {
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "INSERT INTO telemetry.flag_groups \
               (name, description, members, rollout_pct, priority, updated_at) \
             VALUES ($1, COALESCE($2, ''), COALESCE($3, '{}'::text[]), \
                     COALESCE($4, 0), COALESCE($5, 0), now()) \
             ON CONFLICT (name) DO UPDATE SET \
               description = COALESCE($2, telemetry.flag_groups.description), \
               members = COALESCE($3, telemetry.flag_groups.members), \
               rollout_pct = COALESCE($4, telemetry.flag_groups.rollout_pct), \
               priority = COALESCE($5, telemetry.flag_groups.priority), \
               updated_at = now()",
            5,
        )))
        .bind(&b.name)
        .bind(b.description.as_deref())
        .bind(b.members.as_deref())
        .bind(b.rollout_pct)
        .bind(b.priority)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(err)?;
    }
    Ok(Json(json!({ "ok": true, "name": b.name })))
}

#[derive(Deserialize)]
pub struct TargetSetBody {
    group: String,

    #[serde(default)]
    flag: Option<String>,

    #[serde(default)]
    exp_key: Option<String>,

    #[serde(default)]
    state: Option<String>,

    #[serde(default)]
    killed: bool,

    #[serde(default)]
    variant: Option<String>,

    #[serde(default)]
    clear: bool,
}

pub async fn set_target(
    State(st): State<AppState>,
    Json(b): Json<TargetSetBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if b.group.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "group required".into()));
    }
    let action = if b.clear {
        "group.target.clear"
    } else {
        "group.target.set"
    };
    let detail = json!({
        "group": b.group, "flag": b.flag, "exp_key": b.exp_key,
        "state": b.state, "killed": b.killed, "variant": b.variant,
    });
    match (b.flag.as_deref(), b.exp_key.as_deref()) {
        (Some(flag), None) if !flag.is_empty() => {
            let state = b.state.clone().unwrap_or_else(|| "on".to_string());
            if !["on", "off", "forced"].contains(&state.as_str()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "state must be on|off|forced".into(),
                ));
            }
            if b.clear {
                sqlx::query(sqlx::AssertSqlSafe(audited(
                    "DELETE FROM telemetry.flag_group_targets \
                     WHERE flag = $1 AND group_name = $2",
                    2,
                )))
                .bind(flag)
                .bind(&b.group)
                .bind("loopback")
                .bind(action)
                .bind(detail)
                .execute(&st.pool)
                .await
                .map_err(err)?;
            } else {
                sqlx::query(sqlx::AssertSqlSafe(audited(
                    "INSERT INTO telemetry.flag_group_targets \
                       (flag, group_name, state, forced_variant, updated_at) \
                     VALUES ($1, $2, $3, $4, now()) \
                     ON CONFLICT (flag, group_name) DO UPDATE SET \
                       state = $3, forced_variant = $4, updated_at = now()",
                    4,
                )))
                .bind(flag)
                .bind(&b.group)
                .bind(&state)
                .bind(&b.variant)
                .bind("loopback")
                .bind(action)
                .bind(detail)
                .execute(&st.pool)
                .await
                .map_err(err)?;
            }
        }
        (None, Some(exp_key)) if !exp_key.is_empty() => {
            if b.clear {
                sqlx::query(sqlx::AssertSqlSafe(audited(
                    "DELETE FROM telemetry.experiment_group_targets \
                     WHERE exp_key = $1 AND group_name = $2",
                    2,
                )))
                .bind(exp_key)
                .bind(&b.group)
                .bind("loopback")
                .bind(action)
                .bind(detail)
                .execute(&st.pool)
                .await
                .map_err(err)?;
            } else {
                sqlx::query(sqlx::AssertSqlSafe(audited(
                    "INSERT INTO telemetry.experiment_group_targets \
                       (exp_key, group_name, killed, forced_variant, updated_at) \
                     VALUES ($1, $2, $3, $4, now()) \
                     ON CONFLICT (exp_key, group_name) DO UPDATE SET \
                       killed = $3, forced_variant = $4, updated_at = now()",
                    4,
                )))
                .bind(exp_key)
                .bind(&b.group)
                .bind(b.killed)
                .bind(&b.variant)
                .bind("loopback")
                .bind(action)
                .bind(detail)
                .execute(&st.pool)
                .await
                .map_err(err)?;
            }
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "exactly one of flag or exp_key required".into(),
            ))
        }
    }
    Ok(Json(json!({ "ok": true, "group": b.group })))
}

#[derive(Deserialize)]
pub struct AreaSetBody {
    kind: String,
    name: String,

    #[serde(default)]
    area: Option<String>,

    #[serde(default)]
    clear: bool,
}

pub async fn set_area(
    State(st): State<AppState>,
    Json(b): Json<AreaSetBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if !["flag", "experiment"].contains(&b.kind.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "kind must be flag|experiment".into(),
        ));
    }
    if b.name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name required".into()));
    }
    let action = if b.clear { "area.clear" } else { "area.set" };
    let detail = json!({ "kind": b.kind, "name": b.name, "area": b.area });
    if b.clear {
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "DELETE FROM telemetry.product_areas WHERE kind = $1 AND name = $2",
            2,
        )))
        .bind(&b.kind)
        .bind(&b.name)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(err)?;
    } else {
        let area = b.area.clone().unwrap_or_default();
        if area.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "area required".into()));
        }
        sqlx::query(sqlx::AssertSqlSafe(audited(
            "INSERT INTO telemetry.product_areas (kind, name, area, updated_at) \
             VALUES ($1, $2, $3, now()) \
             ON CONFLICT (kind, name) DO UPDATE SET area = $3, updated_at = now()",
            3,
        )))
        .bind(&b.kind)
        .bind(&b.name)
        .bind(&area)
        .bind("loopback")
        .bind(action)
        .bind(detail)
        .execute(&st.pool)
        .await
        .map_err(err)?;
    }
    Ok(Json(json!({ "ok": true, "name": b.name })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(name: &str, members: &[&str], pct: i32) -> Group {
        Group {
            name: name.to_string(),
            description: String::new(),
            members: members.iter().map(|s| s.to_string()).collect(),
            rollout_pct: pct,
            priority: 0,
        }
    }

    #[test]
    fn explicit_member_matches() {
        let g = group("internal", &["0xabc"], 0);
        assert!(matches(&g, "0xabc"));
        assert!(!matches(&g, "0xdef"));
    }

    #[test]
    fn zero_rollout_matches_nobody() {
        let g = group("beta", &[], 0);
        for u in ["a", "b", "c", "0xabc", "user-42"] {
            assert!(!matches(&g, u));
        }
    }

    #[test]
    fn full_rollout_matches_everyone() {
        let g = group("all", &[], 100);
        for u in ["a", "b", "c", "0xabc", "user-42"] {
            assert!(matches(&g, u));
        }
    }

    #[test]
    fn bucket_is_stable_and_spread() {
        assert_eq!(bucket("beta", "0xabc"), bucket("beta", "0xabc"));
        assert_ne!(bucket("beta", "0xabc"), bucket("gamma", "0xabc"));
        let hits = (0..1000)
            .filter(|i| bucket("beta", &format!("user-{i}")) < 10)
            .count();
        assert!((40..160).contains(&hits), "10% rollout hit {hits}/1000");
    }
}
