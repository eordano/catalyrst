use sqlx::PgPool;

pub struct SystemProperty<T> {
    pub key: &'static str,
    pub to_string: fn(&T) -> String,
    pub from_string: fn(&str) -> Option<T>,
}

pub mod properties {
    use super::SystemProperty;

    pub static LAST_GARBAGE_COLLECTION_TIME: SystemProperty<i64> = SystemProperty {
        key: "last_garbage_collection_time",
        to_string: |v| v.to_string(),
        from_string: |s| s.parse::<i64>().ok(),
    };
}

async fn get_raw(pool: &PgPool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_properties WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|r| r.0))
}

async fn set_raw(pool: &PgPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO system_properties (key, value) VALUES ($1, $2)
        ON CONFLICT ON CONSTRAINT system_properties_pkey
        DO UPDATE SET value = $2
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get<T>(pool: &PgPool, property: &SystemProperty<T>) -> Result<Option<T>, sqlx::Error> {
    let raw = get_raw(pool, property.key).await?;
    Ok(raw.and_then(|s| (property.from_string)(&s)))
}

pub async fn set<T>(
    pool: &PgPool,
    property: &SystemProperty<T>,
    value: &T,
) -> Result<(), sqlx::Error> {
    let s = (property.to_string)(value);
    set_raw(pool, property.key, &s).await
}
