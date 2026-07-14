//! Env-gated schema-conformance test: applies every migration to a scratch
//! schema, then PREPAREs every content-table SQL statement found in this
//! crate's sources against it. A statement referencing a column or table no
//! migration defines fails here at test time instead of per-batch in
//! production (the v0.16.0 entity_type incident: the write path carried a
//! column no migration created, and 23h of sync batches rolled back).
//!
//! Statements are extracted from the source files themselves, so new queries
//! are covered automatically; the extraction floor guards the extractor.
//! Same gating as land_publish.rs: CATALYRST_SERVER_TEST_PG, with
//! ALLOW_SKIPPED_INTEGRATION=1 downgrading an unreachable DB to a skip.

use std::path::Path;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

const PG_VAR: &str = "CATALYRST_SERVER_TEST_PG";

const CONTENT_TABLES: &[&str] = &[
    "active_pointers",
    "deployments",
    "content_files",
    "failed_deployments",
    "local_entities",
    "server_sync_cursors",
    "system_properties",
    "admin_audit",
];

const SQL_VERBS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WITH"];

// Below this the extractor is broken, not the sources clean (35 statements at
// the time of writing).
const EXTRACTION_FLOOR: usize = 20;

fn pg_url() -> String {
    catalyrst_testgate::require_pg_or(
        PG_VAR,
        "postgres://postgres:postgres@127.0.0.1:5432/postgres",
    )
}

/// String literals (raw `r#"..."#` and plain `"..."`) that read as SQL against
/// a content table. Format-built strings (containing `{`) are not static SQL
/// and are skipped.
fn extract_sql(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    let mut plain = String::new();

    while let Some(start) = rest.find("r#\"") {
        plain.push_str(&rest[..start]);
        rest = &rest[start + 3..];
        let Some(end) = rest.find("\"#") else { break };
        push_if_sql(&mut out, &rest[..end]);
        rest = &rest[end + 2..];
    }
    plain.push_str(rest);

    for piece in plain.split('"').skip(1).step_by(2) {
        if !piece.contains('\n') {
            push_if_sql(&mut out, piece);
        }
    }

    out
}

fn push_if_sql(out: &mut Vec<String>, candidate: &str) {
    let trimmed = candidate.trim();
    let upper = trimmed.to_uppercase();

    if trimmed.contains('{') {
        return;
    }
    if !SQL_VERBS.iter().any(|v| upper.starts_with(v)) {
        return;
    }
    if !CONTENT_TABLES
        .iter()
        .any(|t| upper.contains(&t.to_uppercase()))
    {
        return;
    }

    out.push(trimmed.to_string());
}

fn walk_sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("read source file");
            for sql in extract_sql(&text) {
                out.push((path.display().to_string(), sql));
            }
        }
    }
}

async fn apply_sql(pool: &PgPool, sql: &str) {
    let mut buf = String::new();
    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("migration statement failed: {e}\n{buf}"));
            buf.clear();
        }
    }
}

#[tokio::test]
async fn every_embedded_statement_prepares_against_migrated_schema() {
    let url = pg_url();
    let admin = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            let _: Option<()> =
                catalyrst_testgate::pg_unusable(PG_VAR, &format!("connect to {url} failed: {e}"));
            return;
        }
    };

    let schema = format!("test_schema_conf_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));

    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .unwrap_or_else(|e| panic!("connect to scratch schema {schema} failed: {e}"));

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut migrations: Vec<_> = std::fs::read_dir(manifest.join("migrations"))
        .expect("read migrations dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    migrations.sort();
    assert!(migrations.len() >= 5, "migrations dir looks truncated");

    for path in &migrations {
        let sql = std::fs::read_to_string(path)
            .expect("read migration")
            .replace("public.", "");
        apply_sql(&pool, &sql).await;
    }

    let mut statements = Vec::new();
    walk_sources(&manifest.join("src"), &mut statements);
    assert!(
        statements.len() >= EXTRACTION_FLOOR,
        "only {} SQL statements extracted - extractor regressed",
        statements.len()
    );

    let mut failures = Vec::new();
    for (i, (file, sql)) in statements.iter().enumerate() {
        let prepared = sqlx::query(sqlx::AssertSqlSafe(format!("PREPARE conf_{i} AS {sql}")))
            .execute(&pool)
            .await;
        if let Err(e) = prepared {
            let head: String = sql.chars().take(140).collect();
            failures.push(format!("{file}: {e}\n    {head}"));
        }
    }

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&admin)
    .await
    .ok();

    assert!(
        failures.is_empty(),
        "{} statement(s) do not conform to the migrated schema:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
