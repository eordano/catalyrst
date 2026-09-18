use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::ports::{lists::ListsComponent, places::PlacesComponent};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn existing_lists_and_shared_nonce_index_do_not_require_table_ownership() {
    let Some(scratch) = ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "places_roles").await
    else {
        return;
    };
    let role = format!("places_role_{}", std::process::id());
    let setup = format!(
        "CREATE ROLE {role}; GRANT USAGE ON SCHEMA {} TO {role}; \
         CREATE TABLE lists_poi (coord text PRIMARY KEY); \
         CREATE TABLE lists_banned_name (name text PRIMARY KEY); \
         CREATE TABLE seen_nonces (signer text, nonce text, expires_at bigint, PRIMARY KEY(signer, nonce)); \
         CREATE INDEX idx_seen_nonces_expires ON seen_nonces(expires_at); \
         GRANT SELECT ON ALL TABLES IN SCHEMA {} TO {role}",
        scratch.schema, scratch.schema,
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(setup))
        .execute(&scratch.pool)
        .await
        .unwrap();
    let set_role = format!("SET ROLE {role}");
    let restricted = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _| {
            let command = set_role.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(command))
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&scratch.url())
        .await
        .unwrap();
    let lists_result = ListsComponent::new(restricted.clone())
        .ensure_schema()
        .await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT CREATE ON SCHEMA {} TO {role}",
        scratch.schema
    )))
    .execute(&scratch.pool)
    .await
    .unwrap();
    let writer_result = PlacesComponent::new(restricted.clone())
        .with_writer(restricted.clone())
        .ensure_local_schema()
        .await;
    restricted.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&scratch.pool)
    .await
    .unwrap();
    scratch.drop().await;
    lists_result.expect("preprovisioned lists require no CREATE or table ownership");
    writer_result.expect("shared replay protection index requires no table ownership");
}
