use sqlx::PgPool;

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
        LIMIT 15000
        "#,
    )
    .bind(content_hash)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}
