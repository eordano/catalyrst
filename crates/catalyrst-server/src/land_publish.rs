use serde_json::{json, Value};
use sqlx::PgPool;
use tracing::warn;

pub async fn local_entities_present<'e, E>(exec: E) -> bool
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_scalar("SELECT to_regclass('local_entities') IS NOT NULL")
        .fetch_one(exec)
        .await
        .unwrap_or(false)
}

pub async fn record_local_provenance(
    conn: &mut sqlx::PgConnection,
    entity_id: &str,
    signer: &str,
) -> Result<bool, sqlx::Error> {
    if !local_entities_present(&mut *conn).await {
        warn!(
            entity_id,
            "local_entities table missing (migration 0003 not applied); skipping provenance record"
        );
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO local_entities (entity_id, signer) VALUES ($1, $2) \
         ON CONFLICT (entity_id) DO NOTHING",
    )
    .bind(entity_id)
    .bind(signer.to_lowercase())
    .execute(conn)
    .await?;
    Ok(true)
}

pub async fn local_provenance(
    pool: &PgPool,
    entity_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    if !local_entities_present(pool).await {
        return Ok(None);
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        signer: String,
        origin: String,
        published_at: f64,
        tombstoned_at: Option<f64>,
        superseded: bool,
    }

    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT le.signer, le.origin,
               date_part('epoch', le.published_at) * 1000 AS published_at,
               date_part('epoch', le.tombstoned_at) * 1000 AS tombstoned_at,
               COALESCE(d.deleter_deployment IS NOT NULL, false) AS superseded
        FROM local_entities le
        LEFT JOIN deployments d ON d.entity_id = le.entity_id
        WHERE le.entity_id = $1
        "#,
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let status = if r.tombstoned_at.is_some() {
            "unpublished"
        } else if r.superseded {
            "superseded"
        } else {
            "active"
        };
        json!({
            "signer": r.signer,
            "origin": r.origin,
            "publishedAt": r.published_at as i64,
            "tombstonedAt": r.tombstoned_at.map(|t| t as i64),
            "superseded": r.superseded,
            "status": status,
        })
    }))
}

#[derive(Debug)]
pub struct UnpublishOutcome {
    pub entity_id: String,
    pub repointed: Vec<(String, Option<String>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum UnpublishError {
    #[error("no locally published scene at {0}")]
    NotLocal(String),
    #[error("a concurrent deployment changed the parcel; retry")]
    Conflict,
    #[error("{0}")]
    Db(String),
}

impl From<sqlx::Error> for UnpublishError {
    fn from(e: sqlx::Error) -> Self {
        UnpublishError::Db(e.to_string())
    }
}

async fn active_local_scene_at(
    conn: &mut sqlx::PgConnection,
    pointer: &str,
) -> Result<Option<(String, i32, Vec<String>)>, UnpublishError> {
    let row: Option<(String, i32, Vec<String>)> = sqlx::query_as(
        r#"
        SELECT d.entity_id, d.id, d.entity_pointers
        FROM active_pointers ap
        JOIN deployments d ON d.entity_id = ap.entity_id
        WHERE ap.pointer = $1 AND d.entity_type = 'scene'
        "#,
    )
    .bind(pointer)
    .fetch_optional(&mut *conn)
    .await?;

    let Some((entity_id, dep_id, pointers)) = row else {
        return Ok(None);
    };

    let is_local: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM local_entities \
         WHERE entity_id = $1 AND tombstoned_at IS NULL)",
    )
    .bind(&entity_id)
    .fetch_one(&mut *conn)
    .await?;

    if !is_local {
        return Ok(None);
    }
    Ok(Some((entity_id, dep_id, pointers)))
}

pub async fn tombstone_and_repoint(
    pool: &PgPool,
    pointer: &str,
) -> Result<UnpublishOutcome, UnpublishError> {
    let pointer = pointer.to_lowercase();

    if !local_entities_present(pool).await {
        return Err(UnpublishError::NotLocal(pointer));
    }

    let mut tx = pool.begin().await?;

    let Some((entity_id, dep_id, entity_pointers)) =
        active_local_scene_at(&mut tx, &pointer).await?
    else {
        return Err(UnpublishError::NotLocal(pointer));
    };

    {
        let mut lock_keys: Vec<&String> = entity_pointers.iter().collect();
        lock_keys.sort();
        lock_keys.dedup();
        for p in lock_keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                .bind(p)
                .execute(&mut *tx)
                .await?;
        }
    }

    match active_local_scene_at(&mut tx, &pointer).await? {
        Some((post_lock_id, _, _)) if post_lock_id == entity_id => {}
        _ => return Err(UnpublishError::Conflict),
    }

    let tombstoned = sqlx::query(
        "UPDATE local_entities SET tombstoned_at = now() \
         WHERE entity_id = $1 AND tombstoned_at IS NULL",
    )
    .bind(&entity_id)
    .execute(&mut *tx)
    .await?;
    if tombstoned.rows_affected() != 1 {
        return Err(UnpublishError::Conflict);
    }

    sqlx::query("UPDATE deployments SET deleter_deployment = NULL WHERE deleter_deployment = $1")
        .bind(dep_id)
        .execute(&mut *tx)
        .await?;

    let held_pointers: Vec<String> =
        sqlx::query_scalar("SELECT pointer FROM active_pointers WHERE entity_id = $1")
            .bind(&entity_id)
            .fetch_all(&mut *tx)
            .await?;

    let has_type_col: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM information_schema.columns
               WHERE table_schema = current_schema()
                 AND table_name = 'active_pointers'
                 AND column_name = 'entity_type')"#,
    )
    .fetch_one(&mut *tx)
    .await?;

    let mut repointed: Vec<(String, Option<String>)> = Vec::new();
    for p in &held_pointers {
        let replacement: Option<String> = sqlx::query_scalar(
            r#"
            SELECT d.entity_id
            FROM deployments d
            WHERE d.entity_type = 'scene'
              AND $1 = ANY(d.entity_pointers)
              AND d.deleter_deployment IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM local_entities le
                  WHERE le.entity_id = d.entity_id AND le.tombstoned_at IS NOT NULL)
            ORDER BY d.entity_timestamp DESC, lower(d.entity_id) DESC
            LIMIT 1
            "#,
        )
        .bind(p)
        .fetch_optional(&mut *tx)
        .await?;

        match &replacement {
            Some(next_id) => {
                if has_type_col {
                    sqlx::query(
                        "UPDATE active_pointers \
                         SET entity_id = $2, entity_type = 'scene' WHERE pointer = $1",
                    )
                    .bind(p)
                    .bind(next_id)
                    .execute(&mut *tx)
                    .await?;
                } else {
                    sqlx::query("UPDATE active_pointers SET entity_id = $2 WHERE pointer = $1")
                        .bind(p)
                        .bind(next_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            None => {
                sqlx::query("DELETE FROM active_pointers WHERE pointer = $1 AND entity_id = $2")
                    .bind(p)
                    .bind(&entity_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        repointed.push((p.clone(), replacement));
    }

    sqlx::query("SELECT pg_notify('new_deployment', 'scene:' || $1)")
        .bind(&entity_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(UnpublishOutcome {
        entity_id,
        repointed,
    })
}
