use sqlx::Postgres;

pub async fn update_active_deployments<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    pointers: &[String],
    entity_id: &str,
) -> Result<(), sqlx::Error> {
    if pointers.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO active_pointers (pointer, entity_id)
        SELECT unnest($1::text[]), $2
        ON CONFLICT (pointer) DO UPDATE SET entity_id = $2
        "#,
    )
    .bind(pointers)
    .bind(entity_id)
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn remove_active_deployments<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    pointers: &[String],
) -> Result<(), sqlx::Error> {
    if pointers.is_empty() {
        return Ok(());
    }

    sqlx::query("DELETE FROM active_pointers WHERE pointer = ANY($1)")
        .bind(pointers)
        .execute(executor)
        .await?;

    Ok(())
}

pub async fn batch_update_active_pointers<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    updates: &[(&str, &str, &str)],
) -> Result<(), sqlx::Error> {
    if updates.is_empty() {
        return Ok(());
    }

    let mut pointers: Vec<&str> = Vec::with_capacity(updates.len());
    let mut entity_ids: Vec<&str> = Vec::with_capacity(updates.len());

    for (pointer, entity_id, _entity_type) in updates {
        pointers.push(pointer);
        entity_ids.push(entity_id);
    }

    sqlx::query(
        r#"
        INSERT INTO active_pointers (pointer, entity_id)
        SELECT unnest($1::text[]), unnest($2::text[])
        ON CONFLICT (pointer) DO UPDATE SET entity_id = EXCLUDED.entity_id
        "#,
    )
    .bind(&pointers)
    .bind(&entity_ids)
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn batch_clear_active_pointers<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    pointers: &[&str],
) -> Result<(), sqlx::Error> {
    if pointers.is_empty() {
        return Ok(());
    }

    sqlx::query("DELETE FROM active_pointers WHERE pointer = ANY($1)")
        .bind(pointers)
        .execute(executor)
        .await?;

    Ok(())
}
