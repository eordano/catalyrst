use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HistoricalDeploymentsRow {
    pub id: i32,
    pub deployer_address: String,
    pub version: String,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_metadata: Option<serde_json::Value>,
    pub entity_timestamp: f64,
    pub entity_pointers: Vec<String>,
    pub local_timestamp: f64,
    pub auth_chain: serde_json::Value,
    pub deleter_deployment: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalDeployment {
    pub deployment_id: i32,
    pub entity_type: String,
    pub entity_id: String,
    pub pointers: Vec<String>,
    pub entity_timestamp: f64,
    pub metadata: Option<serde_json::Value>,
    pub deployer_address: String,
    pub version: String,
    pub auth_chain: serde_json::Value,
    pub local_timestamp: f64,
    pub overwritten_by: Option<String>,
}

impl From<HistoricalDeploymentsRow> for HistoricalDeployment {
    fn from(row: HistoricalDeploymentsRow) -> Self {
        let metadata = row
            .entity_metadata
            .as_ref()
            .and_then(|m| m.get("v").cloned());

        Self {
            deployment_id: row.id,
            entity_type: row.entity_type,
            entity_id: row.entity_id,
            pointers: row.entity_pointers,
            entity_timestamp: row.entity_timestamp,
            metadata,
            deployer_address: row.deployer_address,
            version: row.version,
            auth_chain: row.auth_chain,
            local_timestamp: row.local_timestamp,
            overwritten_by: None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityById {
    #[sqlx(rename = "entityId")]
    pub entity_id: String,
    #[sqlx(rename = "localTimestamp")]
    pub local_timestamp: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortingField {
    LocalTimestamp,
    EntityTimestamp,
}

impl SortingField {
    pub fn as_column(&self) -> &'static str {
        match self {
            Self::LocalTimestamp => "local_timestamp",
            Self::EntityTimestamp => "entity_timestamp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortingOrder {
    Ascending,
    Descending,
}

impl SortingOrder {
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::Ascending => "ASC",
            Self::Descending => "DESC",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentSorting {
    pub field: Option<SortingField>,
    pub order: Option<SortingOrder>,
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentFilters {
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub entity_types: Option<Vec<String>>,
    pub entity_ids: Option<Vec<String>>,
    pub pointers: Option<Vec<String>>,
    pub only_currently_pointed: Option<bool>,
}

pub async fn deployment_exists(pool: &PgPool, entity_id: &str) -> Result<bool, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM deployments WHERE entity_id = $1")
        .bind(entity_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.is_some())
}

pub async fn stream_all_entity_ids_in_time_range(
    pool: &PgPool,
    init_timestamp_ms: f64,
    end_timestamp_ms: f64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT entity_id
        FROM deployments
        WHERE entity_timestamp
            BETWEEN to_timestamp($1 / 1000.0)
            AND to_timestamp($2 / 1000.0)
        "#,
    )
    .bind(init_timestamp_ms)
    .bind(end_timestamp_ms)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn stream_all_distinct_entity_ids(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT entity_id FROM deployments")
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn get_historical_deployments(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    filters: Option<&DeploymentFilters>,
    sort_by: Option<&DeploymentSorting>,
    last_id: Option<&str>,
) -> Result<Vec<HistoricalDeployment>, sqlx::Error> {
    let sorting_field = sort_by
        .and_then(|s| s.field)
        .unwrap_or(SortingField::LocalTimestamp);
    let sorting_order = sort_by
        .and_then(|s| s.order)
        .unwrap_or(SortingOrder::Descending);

    let ts_col = sorting_field.as_column();
    let order = sorting_order.as_sql();

    let mut sql = String::from(
        r#"
        SELECT
            dep1.id,
            dep1.entity_type,
            dep1.entity_id,
            dep1.entity_pointers,
            date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
            dep1.entity_metadata,
            dep1.deployer_address,
            dep1.version,
            dep1.auth_chain,
            date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
            dep1.deleter_deployment
        FROM deployments AS dep1
        "#,
    );

    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx: usize = 1;

    if let Some(f) = filters {
        if f.from.is_some() {
            if last_id.is_some() && sorting_order == SortingOrder::Ascending {
                conditions.push(format!(
                    "((LOWER(dep1.entity_id) > LOWER(${next}) AND dep1.{ts_col} = to_timestamp(${ts} / 1000.0)) OR (dep1.{ts_col} > to_timestamp(${ts} / 1000.0)))",
                    next = param_idx,
                    ts = param_idx + 1,
                ));
                param_idx += 2;
            } else {
                conditions.push(format!(
                    "dep1.{ts_col} >= to_timestamp(${param_idx} / 1000.0)"
                ));
                param_idx += 1;
            }
        }
        if f.to.is_some() {
            if last_id.is_some() && sorting_order == SortingOrder::Descending {
                conditions.push(format!(
                    "((LOWER(dep1.entity_id) < LOWER(${next}) AND dep1.{ts_col} = to_timestamp(${ts} / 1000.0)) OR (dep1.{ts_col} < to_timestamp(${ts} / 1000.0)))",
                    next = param_idx,
                    ts = param_idx + 1,
                ));
                param_idx += 2;
            } else {
                conditions.push(format!(
                    "dep1.{ts_col} <= to_timestamp(${param_idx} / 1000.0)"
                ));
                param_idx += 1;
            }
        }

        if let Some(ref types) = f.entity_types {
            if !types.is_empty() {
                conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
                param_idx += 1;
            }
        }

        if let Some(ref ids) = f.entity_ids {
            if !ids.is_empty() {
                conditions.push(format!("dep1.entity_id = ANY(${})", param_idx));
                param_idx += 1;
            }
        }

        if f.only_currently_pointed == Some(true) {
            conditions.push("dep1.deleter_deployment IS NULL".into());
        }

        if let Some(ref ptrs) = f.pointers {
            if !ptrs.is_empty() {
                conditions.push(format!("dep1.entity_pointers && ${}", param_idx));
                param_idx += 1;
            }
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(&format!(
        " ORDER BY dep1.\"{}\" {}, LOWER(dep1.entity_id) {}",
        ts_col, order, order,
    ));

    sql.push_str(&format!(" LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));

    let mut query = sqlx::query_as::<_, HistoricalDeploymentsRow>(&sql);

    if let Some(f) = filters {
        if let Some(from) = f.from {
            if let (Some(lid), SortingOrder::Ascending) = (last_id, sorting_order) {
                query = query.bind(lid.to_string());
            }
            query = query.bind(from);
        }
        if let Some(to) = f.to {
            if let (Some(lid), SortingOrder::Descending) = (last_id, sorting_order) {
                query = query.bind(lid.to_string());
            }
            query = query.bind(to);
        }

        if let Some(ref types) = f.entity_types {
            if !types.is_empty() {
                query = query.bind(types.clone());
            }
        }
        if let Some(ref ids) = f.entity_ids {
            if !ids.is_empty() {
                query = query.bind(ids.clone());
            }
        }
        if let Some(ref ptrs) = f.pointers {
            if !ptrs.is_empty() {
                let lower: Vec<String> = ptrs.iter().map(|p| p.to_lowercase()).collect();
                query = query.bind(lower);
            }
        }
    }

    query = query.bind(limit).bind(offset);

    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(HistoricalDeployment::from).collect())
}

pub async fn get_active_deployments_by_content_hash(
    pool: &PgPool,
    content_hash: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT deployment.entity_id
        FROM deployments AS deployment
        INNER JOIN content_files ON content_files.deployment = deployment.id
        WHERE content_hash = $1
          AND deployment.deleter_deployment IS NULL
        "#,
    )
    .bind(content_hash)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn get_entity_by_id(
    pool: &PgPool,
    entity_id: &str,
) -> Result<Option<EntityById>, sqlx::Error> {
    sqlx::query_as::<_, EntityById>(
        r#"
        SELECT
            entity_id AS "entityId",
            date_part('epoch', d.local_timestamp) * 1000 AS "localTimestamp"
        FROM deployments d
        WHERE entity_id = $1
        LIMIT 1
        "#,
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await
}

pub async fn save_deployment(
    pool: &PgPool,
    deployer_address: &str,
    version: &str,
    entity_type: &str,
    entity_id: &str,
    entity_timestamp_ms: f64,
    entity_pointers: &[String],
    metadata: Option<&serde_json::Value>,
    local_timestamp_ms: f64,
    auth_chain_json: &str,
    overwritten_by: Option<i32>,
) -> Result<i32, sqlx::Error> {
    let wrapped_metadata: Option<serde_json::Value> =
        metadata.map(|m| serde_json::json!({ "v": m }));

    let row: (i32,) = sqlx::query_as(
        r#"
        INSERT INTO deployments
            (deployer_address, version, entity_type, entity_id, entity_timestamp,
             entity_pointers, entity_metadata, local_timestamp, auth_chain, deleter_deployment)
        VALUES
            ($1, $2, $3, $4, to_timestamp($5 / 1000.0),
             $6, $7, to_timestamp($8 / 1000.0), $9::json, $10)
        RETURNING id
        "#,
    )
    .bind(deployer_address)
    .bind(version)
    .bind(entity_type)
    .bind(entity_id)
    .bind(entity_timestamp_ms)
    .bind(entity_pointers)
    .bind(wrapped_metadata)
    .bind(local_timestamp_ms)
    .bind(auth_chain_json)
    .bind(overwritten_by)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn get_deployments(
    pool: &PgPool,
    deployment_ids: &[i32],
) -> Result<Vec<(i32, Vec<String>)>, sqlx::Error> {
    if deployment_ids.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        id: i32,
        pointers: Vec<String>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, entity_pointers AS pointers FROM deployments WHERE id = ANY($1)",
    )
    .bind(deployment_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| (r.id, r.pointers)).collect())
}

pub async fn set_entities_as_overwritten<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    overwritten_ids: &[i32],
    overwritten_by: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE deployments SET deleter_deployment = $1 WHERE id = ANY($2)")
        .bind(overwritten_by)
        .bind(overwritten_ids)
        .execute(executor)
        .await?;

    Ok(())
}

pub async fn calculate_overwrote(
    pool: &PgPool,
    entity_type: &str,
    entity_pointers: &[String],
    entity_timestamp_ms: f64,
    entity_id: &str,
) -> Result<Vec<i32>, sqlx::Error> {
    let rows: Vec<(i32,)> = sqlx::query_as(
        r#"
        SELECT dep1.id
        FROM deployments AS dep1
        LEFT JOIN deployments AS dep2 ON dep1.deleter_deployment = dep2.id
        WHERE dep1.entity_type = $1
          AND dep1.entity_pointers && $2
          AND (dep1.entity_timestamp < to_timestamp($3 / 1000.0)
               OR (dep1.entity_timestamp = to_timestamp($3 / 1000.0) AND dep1.entity_id < $4))
          AND (dep2.id IS NULL
               OR dep2.entity_timestamp > to_timestamp($3 / 1000.0)
               OR (dep2.entity_timestamp = to_timestamp($3 / 1000.0) AND dep2.entity_id > $4))
        ORDER BY dep1.entity_timestamp DESC, dep1.entity_id DESC
        "#,
    )
    .bind(entity_type)
    .bind(entity_pointers)
    .bind(entity_timestamp_ms)
    .bind(entity_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn calculate_overwritten_by_many_fast(
    pool: &PgPool,
    entity_type: &str,
    entity_pointers: &[String],
    entity_timestamp_ms: f64,
    entity_id: &str,
) -> Result<Vec<i32>, sqlx::Error> {
    if entity_pointers.is_empty() {
        return Ok(Vec::new());
    }

    let rows: Vec<(i32,)> = sqlx::query_as(
        r#"
        SELECT deployments.id
        FROM active_pointers AS ap
        INNER JOIN deployments ON ap.entity_id = deployments.entity_id
        WHERE ap.pointer = ANY($1)
          AND deployments.entity_type = $2
          AND (deployments.entity_timestamp > to_timestamp($3 / 1000.0)
               OR (deployments.entity_timestamp = to_timestamp($3 / 1000.0)
                   AND deployments.entity_id > $4))
        ORDER BY deployments.entity_timestamp, deployments.entity_id
        LIMIT 1
        "#,
    )
    .bind(entity_pointers)
    .bind(entity_type)
    .bind(entity_timestamp_ms)
    .bind(entity_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[derive(Debug, Clone)]
pub struct DeploymentRow {
    pub deployer_address: String,
    pub version: String,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_timestamp_ms: f64,
    pub entity_pointers: Vec<String>,
    pub entity_metadata: Option<serde_json::Value>,
    pub local_timestamp_ms: f64,
    pub auth_chain_json: String,
    pub overwritten_by: Option<i32>,
}

pub async fn batch_save_deployments(
    pool: &PgPool,
    deployments: &[DeploymentRow],
) -> Result<Vec<i32>, sqlx::Error> {
    if deployments.is_empty() {
        return Ok(Vec::new());
    }

    let n = deployments.len();

    let mut deployer_addresses: Vec<&str> = Vec::with_capacity(n);
    let mut versions: Vec<&str> = Vec::with_capacity(n);
    let mut entity_types: Vec<&str> = Vec::with_capacity(n);
    let mut entity_ids: Vec<&str> = Vec::with_capacity(n);
    let mut entity_timestamps: Vec<f64> = Vec::with_capacity(n);
    let mut entity_pointers: Vec<&[String]> = Vec::with_capacity(n);
    let mut entity_metadatas: Vec<Option<serde_json::Value>> = Vec::with_capacity(n);
    let mut local_timestamps: Vec<f64> = Vec::with_capacity(n);
    let mut auth_chains: Vec<&str> = Vec::with_capacity(n);
    let mut deleter_deployments: Vec<Option<i32>> = Vec::with_capacity(n);

    for d in deployments {
        deployer_addresses.push(&d.deployer_address);
        versions.push(&d.version);
        entity_types.push(&d.entity_type);
        entity_ids.push(&d.entity_id);
        entity_timestamps.push(d.entity_timestamp_ms);
        entity_pointers.push(d.entity_pointers.as_slice());
        entity_metadatas.push(
            d.entity_metadata
                .as_ref()
                .map(|m| serde_json::json!({ "v": m })),
        );
        local_timestamps.push(d.local_timestamp_ms);
        auth_chains.push(&d.auth_chain_json);
        deleter_deployments.push(d.overwritten_by);
    }

    let pointer_jsons: Vec<serde_json::Value> = deployments
        .iter()
        .map(|d| {
            serde_json::Value::Array(
                d.entity_pointers
                    .iter()
                    .map(|p| serde_json::Value::String(p.clone()))
                    .collect(),
            )
        })
        .collect();

    let mut tx = pool.begin().await?;

    let rows: Vec<(i32,)> = sqlx::query_as(
        r#"
        INSERT INTO deployments
            (deployer_address, version, entity_type, entity_id, entity_timestamp,
             entity_pointers, entity_metadata, local_timestamp, auth_chain, deleter_deployment)
        SELECT d.deployer_address, d.version, d.entity_type, d.entity_id,
               d.entity_timestamp,
               ARRAY(SELECT jsonb_array_elements_text(d.ptrs_json)),
               d.entity_metadata::json, d.local_timestamp, d.auth_chain::json,
               d.deleter_deployment
        FROM (
            SELECT unnest($1::text[]) AS deployer_address,
                   unnest($2::text[]) AS version,
                   unnest($3::text[]) AS entity_type,
                   unnest($4::text[]) AS entity_id,
                   to_timestamp(unnest($5::float8[]) / 1000.0) AS entity_timestamp,
                   unnest($6::jsonb[]) AS ptrs_json,
                   unnest($7::jsonb[]) AS entity_metadata,
                   to_timestamp(unnest($8::float8[]) / 1000.0) AS local_timestamp,
                   unnest($9::text[]) AS auth_chain,
                   unnest($10::int4[]) AS deleter_deployment
        ) d
        RETURNING id
        "#,
    )
    .bind(&deployer_addresses)
    .bind(&versions)
    .bind(&entity_types)
    .bind(&entity_ids)
    .bind(&entity_timestamps)
    .bind(&pointer_jsons)
    .bind(&entity_metadatas)
    .bind(&local_timestamps)
    .bind(&auth_chains)
    .bind(&deleter_deployments)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn calculate_overwritten_by_slow(
    pool: &PgPool,
    entity_type: &str,
    entity_pointers: &[String],
    entity_timestamp_ms: f64,
    entity_id: &str,
) -> Result<Vec<i32>, sqlx::Error> {
    let rows: Vec<(i32,)> = sqlx::query_as(
        r#"
        SELECT deployments.id
        FROM deployments
        WHERE deployments.entity_type = $1
          AND deployments.entity_pointers && $2
          AND (deployments.entity_timestamp > to_timestamp($3 / 1000.0)
               OR (deployments.entity_timestamp = to_timestamp($3 / 1000.0)
                   AND deployments.entity_id > $4))
        ORDER BY deployments.entity_timestamp, deployments.entity_id
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_pointers)
    .bind(entity_timestamp_ms)
    .bind(entity_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}
