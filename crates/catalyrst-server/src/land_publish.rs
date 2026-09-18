use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use tracing::warn;

use crate::schema_migrations::DeploymentSchema;
use crate::write_deployer::lock_deployment_pointers;

pub async fn local_entities_present<'e, E>(exec: E) -> bool
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query_scalar!(r#"SELECT to_regclass('local_entities') IS NOT NULL AS "present!""#)
        .fetch_one(exec)
        .await
        .unwrap_or(false)
}

pub async fn record_local_provenance(
    conn: &mut sqlx::PgConnection,
    entity_id: &str,
    signer: &str,
) -> Result<bool, sqlx::Error> {
    let present = local_entities_present(&mut *conn).await;
    record_local_provenance_with_schema(conn, entity_id, signer, present).await
}

pub(crate) async fn record_local_provenance_with_schema(
    conn: &mut sqlx::PgConnection,
    entity_id: &str,
    signer: &str,
    present: bool,
) -> Result<bool, sqlx::Error> {
    if !present {
        warn!(
            entity_id,
            "local_entities table missing (migration 0003 not applied); skipping provenance record"
        );
        return Ok(false);
    }
    sqlx::query!(
        "INSERT INTO local_entities (entity_id, signer) VALUES ($1, $2) \
         ON CONFLICT (entity_id) DO NOTHING",
        entity_id,
        signer.to_lowercase()
    )
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

    struct Row {
        signer: String,
        origin: String,
        published_at: f64,
        tombstoned_at: Option<f64>,
        superseded: bool,
    }

    let row: Option<Row> = sqlx::query_as!(
        Row,
        r#"
        SELECT le.signer, le.origin,
               date_part('epoch', le.published_at) * 1000 AS "published_at!",
               date_part('epoch', le.tombstoned_at) * 1000 AS tombstoned_at,
               COALESCE(d.deleter_deployment IS NOT NULL, false) AS "superseded!"
        FROM local_entities le
        LEFT JOIN deployments d ON d.entity_id = le.entity_id
        WHERE le.entity_id = $1
        "#,
        entity_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        provenance_json(
            r.signer,
            r.origin,
            r.published_at,
            r.tombstoned_at,
            r.superseded,
        )
    }))
}

pub fn provenance_json(
    signer: String,
    origin: String,
    published_at: f64,
    tombstoned_at: Option<f64>,
    superseded: bool,
) -> Value {
    let status = if tombstoned_at.is_some() {
        "unpublished"
    } else if superseded {
        "superseded"
    } else {
        "active"
    };
    json!({
        "signer": signer,
        "origin": origin,
        "publishedAt": published_at as i64,
        "tombstonedAt": tombstoned_at.map(|t| t as i64),
        "superseded": superseded,
        "status": status,
    })
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
    Ok(sqlx::query_as::<_, (String, i32, Vec<String>)>(
        r#"
        SELECT d.entity_id, d.id, d.entity_pointers
        FROM active_pointers ap
        JOIN deployments d ON d.entity_id = ap.entity_id
        JOIN local_entities le ON le.entity_id = d.entity_id AND le.tombstoned_at IS NULL
        WHERE ap.pointer = $1 AND d.entity_type = 'scene'
        "#,
    )
    .bind(pointer)
    .fetch_optional(&mut *conn)
    .await?)
}

/// One snapshot for the whole mutation: `cur` re-checks the pointer under the lock, the tombstone
/// row gates every other sub-statement, and the restored upstream rows are matched by their old
/// `deleter_deployment` because the CTE cannot see its own UPDATE.
fn unpublish_statement(pointer_entity_type: bool) -> String {
    let set_type = if pointer_entity_type {
        ", entity_type = 'scene'"
    } else {
        ""
    };
    format!(
        r#"
        WITH cur AS (
            SELECT d.id
            FROM active_pointers ap
            JOIN deployments d ON d.entity_id = ap.entity_id
            JOIN local_entities le ON le.entity_id = d.entity_id AND le.tombstoned_at IS NULL
            WHERE ap.pointer = $2 AND d.entity_type = 'scene' AND d.entity_id = $1
        ), tomb AS (
            UPDATE local_entities SET tombstoned_at = now()
            WHERE entity_id = $1 AND tombstoned_at IS NULL AND EXISTS (SELECT 1 FROM cur)
            RETURNING entity_id
        ), restored AS (
            UPDATE deployments SET deleter_deployment = NULL
            WHERE deleter_deployment = (SELECT id FROM cur) AND EXISTS (SELECT 1 FROM tomb)
            RETURNING id
        ), held AS (
            SELECT ap.pointer
            FROM active_pointers ap
            WHERE ap.entity_id = $1 AND EXISTS (SELECT 1 FROM tomb)
        ), replacement AS (
            SELECT DISTINCT ON (h.pointer) h.pointer, d.entity_id
            FROM held h
            JOIN deployments d ON h.pointer = ANY(d.entity_pointers)
            WHERE d.entity_type = 'scene'
              AND d.entity_id <> $1
              AND (d.deleter_deployment IS NULL OR d.deleter_deployment = (SELECT id FROM cur))
              AND NOT EXISTS (
                  SELECT 1 FROM local_entities le
                  WHERE le.entity_id = d.entity_id AND le.tombstoned_at IS NOT NULL)
            ORDER BY h.pointer, d.entity_timestamp DESC, lower(d.entity_id) DESC
        ), repointed AS (
            UPDATE active_pointers ap SET entity_id = r.entity_id{set_type}
            FROM replacement r
            WHERE ap.pointer = r.pointer
            RETURNING ap.pointer
        ), removed AS (
            DELETE FROM active_pointers ap
            WHERE ap.entity_id = $1
              AND ap.pointer IN (SELECT pointer FROM held)
              AND NOT EXISTS (SELECT 1 FROM replacement r WHERE r.pointer = ap.pointer)
            RETURNING ap.pointer
        ), notified AS (
            SELECT pg_notify('new_deployment', 'scene:' || $1) AS sent
        )
        SELECT (SELECT count(*) FROM tomb) AS tombstoned,
               h.pointer::text AS pointer,
               r.entity_id AS replacement
        FROM (SELECT 1) AS marker
        CROSS JOIN notified
        LEFT JOIN held h ON true
        LEFT JOIN replacement r ON r.pointer = h.pointer
        ORDER BY h.pointer
        "#
    )
}

pub async fn tombstone_and_repoint(
    pool: &PgPool,
    pointer: &str,
) -> Result<UnpublishOutcome, UnpublishError> {
    let schema = DeploymentSchema::detect(pool)
        .await
        .unwrap_or(DeploymentSchema {
            local_entities: false,
            pointer_entity_type: false,
        });
    tombstone_and_repoint_with_schema(pool, pointer, schema).await
}

pub async fn tombstone_and_repoint_with_schema(
    pool: &PgPool,
    pointer: &str,
    schema: DeploymentSchema,
) -> Result<UnpublishOutcome, UnpublishError> {
    let pointer = pointer.to_lowercase();

    if !schema.local_entities {
        return Err(UnpublishError::NotLocal(pointer));
    }

    let mut tx = pool.begin().await?;

    let Some((entity_id, _dep_id, entity_pointers)) =
        active_local_scene_at(&mut tx, &pointer).await?
    else {
        return Err(UnpublishError::NotLocal(pointer));
    };

    lock_deployment_pointers(&mut tx, &entity_pointers).await?;

    let rows = sqlx::query(sqlx::AssertSqlSafe(unpublish_statement(
        schema.pointer_entity_type,
    )))
    .bind(&entity_id)
    .bind(&pointer)
    .fetch_all(&mut *tx)
    .await?;

    let tombstoned: i64 = rows
        .first()
        .map(|r| r.try_get("tombstoned"))
        .transpose()?
        .unwrap_or(0);
    if tombstoned != 1 {
        return Err(UnpublishError::Conflict);
    }

    let mut repointed: Vec<(String, Option<String>)> = Vec::with_capacity(rows.len());
    for row in &rows {
        if let Some(pointer) = row.try_get::<Option<String>, _>("pointer")? {
            repointed.push((pointer, row.try_get("replacement")?));
        }
    }

    tx.commit().await?;

    Ok(UnpublishOutcome {
        entity_id,
        repointed,
    })
}
