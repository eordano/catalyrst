use std::collections::HashMap;

use sqlx::{PgPool, Postgres};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContentFilesRow {
    pub deployment: i32,
    pub key: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct DeploymentContent {
    pub key: String,
    pub hash: String,
}

pub async fn get_content_files(
    pool: &PgPool,
    deployment_ids: &[i32],
) -> Result<HashMap<i32, Vec<DeploymentContent>>, sqlx::Error> {
    if deployment_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<ContentFilesRow> = sqlx::query_as(
        "SELECT deployment, key, content_hash FROM content_files WHERE deployment = ANY($1) ORDER BY ctid",
    )
    .bind(deployment_ids)
    .fetch_all(pool)
    .await?;

    let mut result: HashMap<i32, Vec<DeploymentContent>> = HashMap::new();
    for row in rows {
        result
            .entry(row.deployment)
            .or_default()
            .push(DeploymentContent {
                key: row.key,
                hash: row.content_hash,
            });
    }

    Ok(result)
}

pub async fn save_content_files<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    deployment_id: i32,
    content: &[(String, String)],
) -> Result<(), sqlx::Error> {
    if content.is_empty() {
        return Ok(());
    }

    let keys: Vec<&str> = content.iter().map(|(k, _)| k.as_str()).collect();
    let hashes: Vec<&str> = content.iter().map(|(_, h)| h.as_str()).collect();
    let dep_ids: Vec<i32> = vec![deployment_id; content.len()];

    sqlx::query(
        r#"
        INSERT INTO content_files (deployment, key, content_hash)
        SELECT unnest($1::int[]), unnest($2::text[]), unnest($3::text[])
        "#,
    )
    .bind(&dep_ids)
    .bind(&keys)
    .bind(&hashes)
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn batch_save_content_files<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    files: &[(i32, &str, &str)],
) -> Result<(), sqlx::Error> {
    if files.is_empty() {
        return Ok(());
    }

    let mut dep_ids: Vec<i32> = Vec::with_capacity(files.len());
    let mut hashes: Vec<&str> = Vec::with_capacity(files.len());
    let mut keys: Vec<&str> = Vec::with_capacity(files.len());

    for (dep_id, hash, key) in files {
        dep_ids.push(*dep_id);
        hashes.push(hash);
        keys.push(key);
    }

    sqlx::query(
        r#"
        INSERT INTO content_files (deployment, key, content_hash)
        SELECT unnest($1::int[]), unnest($2::text[]), unnest($3::text[])
        "#,
    )
    .bind(&dep_ids)
    .bind(&keys)
    .bind(&hashes)
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn find_content_hashes_not_being_used_anymore(
    pool: &PgPool,
    last_gc_timestamp_ms: f64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT content_files.content_hash
        FROM content_files
        INNER JOIN deployments ON content_files.deployment = id
        LEFT JOIN deployments AS dd ON deployments.deleter_deployment = dd.id
        WHERE dd.local_timestamp IS NULL
           OR dd.local_timestamp > to_timestamp($1 / 1000.0)
        GROUP BY content_files.content_hash
        HAVING bool_or(deployments.deleter_deployment IS NULL) = FALSE
        "#,
    )
    .bind(last_gc_timestamp_ms)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub const GC_UNUSED_HASHES_BATCH_SIZE: i64 = 1000;

pub async fn find_content_hashes_not_being_used_anymore_batch(
    pool: &PgPool,
    last_gc_timestamp_ms: f64,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT content_files.content_hash
        FROM content_files
        INNER JOIN deployments ON content_files.deployment = id
        LEFT JOIN deployments AS dd ON deployments.deleter_deployment = dd.id
        WHERE (dd.local_timestamp IS NULL
               OR dd.local_timestamp > to_timestamp($1 / 1000.0))
          AND ($2::text IS NULL OR content_files.content_hash > $2)
        GROUP BY content_files.content_hash
        HAVING bool_or(deployments.deleter_deployment IS NULL) = FALSE
        ORDER BY content_files.content_hash
        LIMIT $3
        "#,
    )
    .bind(last_gc_timestamp_ms)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn stream_all_distinct_content_file_hashes(
    pool: &PgPool,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT content_hash FROM content_files")
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}
