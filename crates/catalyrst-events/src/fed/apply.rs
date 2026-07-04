use catalyrst_fed::Signed;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::fed::ids::{schedule_id, signature_hash_hex};
use crate::fed::messages::{ProfileSettingsUpdate, ScheduleUpsert};
use crate::http::response::ApiError;

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

pub struct Applied {
    pub signature_hash: String,

    pub fresh: bool,
}

async fn record_action(
    pool: &PgPool,
    sig_hash: &str,
    signer: &str,
    action_type: &str,
    payload: &Value,
    signed_at: i64,
    origin_peer: Option<&str>,
) -> Result<bool, ApiError> {
    let res = sqlx::query(
        "INSERT INTO signed_actions_events \
            (signature_hash, signer, action_type, message_payload, signed_at, received_at, origin_peer) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(sig_hash)
    .bind(signer.to_ascii_lowercase())
    .bind(action_type)
    .bind(payload)
    .bind(signed_at)
    .bind(now_secs())
    .bind(origin_peer)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn apply_profile_settings(
    pool: &PgPool,
    signed: &Signed<ProfileSettingsUpdate>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<(Applied, Value), ApiError> {
    let m = &signed.message;
    let target = m.target.to_ascii_lowercase();
    let sig_hash = signature_hash_hex(&signed.hash());
    let payload = serde_json::to_value(m).unwrap_or_else(|_| json!({}));

    let fresh = record_action(
        pool,
        &sig_hash,
        signer,
        "ProfileSettingsUpdate",
        &payload,
        signed.signed_at,
        origin_peer,
    )
    .await?;

    let permissions = m.permissions.clone().map(|p| json!(p));
    sqlx::query(
        r#"INSERT INTO event_profile_settings
            ("user", email, email_verified, use_local_time, notify_by_email, notify_by_browser, permissions, updated_at)
           VALUES ($1, $2, COALESCE($3, false), COALESCE($4, true), COALESCE($5, false), COALESCE($6, false), COALESCE($7, '[]'::jsonb), now())
           ON CONFLICT ("user") DO UPDATE SET
             email = COALESCE($2, event_profile_settings.email),
             email_verified = COALESCE($3, event_profile_settings.email_verified),
             use_local_time = COALESCE($4, event_profile_settings.use_local_time),
             notify_by_email = COALESCE($5, event_profile_settings.notify_by_email),
             notify_by_browser = COALESCE($6, event_profile_settings.notify_by_browser),
             permissions = COALESCE($7, event_profile_settings.permissions),
             updated_at = now()"#,
    )
    .bind(&target)
    .bind(m.email.as_deref())
    .bind(m.email_verified)
    .bind(m.use_local_time)
    .bind(m.notify_by_email)
    .bind(m.notify_by_browser)
    .bind(permissions)
    .execute(pool)
    .await?;

    let settings = load_settings(pool, &target).await?;
    Ok((
        Applied {
            signature_hash: sig_hash,
            fresh,
        },
        settings,
    ))
}

pub async fn apply_schedule(
    pool: &PgPool,
    signed: &Signed<ScheduleUpsert>,
    signer: &str,
    origin_peer: Option<&str>,
) -> Result<(Applied, Value), ApiError> {
    let m = &signed.message;
    let sig_hash = signature_hash_hex(&signed.hash());
    let id = m
        .schedule_id
        .clone()
        .unwrap_or_else(|| schedule_id(signer, &m.name, &signed.nonce));
    let payload = serde_json::to_value(m).unwrap_or_else(|_| json!({}));

    let fresh = record_action(
        pool,
        &sig_hash,
        signer,
        "ScheduleUpsert",
        &payload,
        signed.signed_at,
        origin_peer,
    )
    .await?;

    let active_since = chrono::DateTime::from_timestamp(m.active_since, 0);
    let active_until = chrono::DateTime::from_timestamp(m.active_until, 0);
    sqlx::query(
        r#"INSERT INTO schedules_local
            (id, name, description, image, theme, background, active_since, active_until, active, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
           ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             description = EXCLUDED.description,
             image = EXCLUDED.image,
             theme = EXCLUDED.theme,
             background = EXCLUDED.background,
             active_since = EXCLUDED.active_since,
             active_until = EXCLUDED.active_until,
             active = EXCLUDED.active,
             updated_at = now()"#,
    )
    .bind(&id)
    .bind(&m.name)
    .bind(m.description.as_deref())
    .bind(m.image.as_deref())
    .bind(m.theme.as_deref())
    .bind(json!(m.background))
    .bind(active_since)
    .bind(active_until)
    .bind(m.active)
    .execute(pool)
    .await?;

    let schedule = load_schedule(pool, &id).await?.unwrap_or_else(|| json!({}));
    Ok((
        Applied {
            signature_hash: sig_hash,
            fresh,
        },
        schedule,
    ))
}

#[derive(sqlx::FromRow)]
struct SettingsRow {
    email: Option<String>,
    email_verified: bool,
    use_local_time: bool,
    notify_by_email: bool,
    notify_by_browser: bool,
    permissions: Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
    id: String,
    name: String,
    description: Option<String>,
    image: Option<String>,
    theme: Option<String>,
    background: Value,
    active_since: Option<chrono::DateTime<chrono::Utc>>,
    active_until: Option<chrono::DateTime<chrono::Utc>>,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn load_settings(pool: &PgPool, user: &str) -> Result<Value, ApiError> {
    let user = user.to_ascii_lowercase();
    let row: Option<SettingsRow> = sqlx::query_as(
        r#"SELECT email, email_verified, use_local_time, notify_by_email, notify_by_browser,
                  permissions, created_at, updated_at
             FROM event_profile_settings WHERE "user" = $1"#,
    )
    .bind(&user)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(r) => json!({
            "user": user,
            "email": r.email,
            "email_verified": r.email_verified,
            "use_local_time": r.use_local_time,
            "notify_by_email": r.notify_by_email,
            "notify_by_browser": r.notify_by_browser,
            "permissions": r.permissions,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
        }),
        None => json!({
            "user": user,
            "email": null,
            "email_verified": false,
            "use_local_time": true,
            "notify_by_email": false,
            "notify_by_browser": false,
            "permissions": [],
            "created_at": "1970-01-01T00:00:00.000Z",
            "updated_at": "1970-01-01T00:00:00.000Z",
        }),
    })
}

pub async fn load_schedule(pool: &PgPool, id: &str) -> Result<Option<Value>, ApiError> {
    let row: Option<ScheduleRow> = sqlx::query_as(
        "SELECT id, name, description, image, theme, background, active_since, active_until, \
                active, created_at, updated_at FROM schedules_local WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        json!({
            "id": r.id,
            "name": r.name,
            "description": r.description,
            "image": r.image,
            "theme": r.theme,
            "background": r.background,
            "active_since": r.active_since,
            "active_until": r.active_until,
            "active": r.active,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
        })
    }))
}

pub async fn list_settings(pool: &PgPool) -> Result<Vec<Value>, ApiError> {
    let rows: Vec<(String,)> =
        sqlx::query_as(r#"SELECT "user" FROM event_profile_settings ORDER BY "user" ASC"#)
            .fetch_all(pool)
            .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (user,) in rows {
        out.push(load_settings(pool, &user).await?);
    }
    Ok(out)
}
