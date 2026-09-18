use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

pub struct Fixture {
    admin: PgPool,
    pub pool: PgPool,
    schema: String,
}

impl Fixture {
    pub async fn new() -> Option<Self> {
        let url = catalyrst_testgate::require_pg("CATALYRST_ARCHIPELAGO_TEST_PG")?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect to configured test database");
        let schema = format!("archipelago_v4_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create isolated schema");
        let options = PgConnectOptions::from_str(&url)
            .expect("parse test database URL")
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .expect("connect isolated pool");
        Some(Self {
            admin,
            pool,
            schema,
        })
    }

    pub async fn finish(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .expect("remove this test's schema");
        self.admin.close().await;
    }
}
