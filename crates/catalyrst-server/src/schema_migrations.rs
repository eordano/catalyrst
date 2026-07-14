use sqlx::PgPool;

/// Content-DB schema migrations, applied in order on every boot. Each file is
/// idempotent (`CREATE ... IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS`), so
/// re-running the whole set self-heals a node whose out-of-band `psql -f` step
/// ever skipped one -- the failure mode that leaves `server_sync_cursors` (0004)
/// absent and silently degrades per-server sync resume to the global frontier.
/// Deliberately not `sqlx::migrate!`: the content DB carries no `_sqlx_migrations`
/// ledger, and these run under the content owner role so new objects inherit the
/// same ownership/grants as their siblings.
const CONTENT_MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_content_schema",
        include_str!("../migrations/0001_content_schema.sql"),
    ),
    (
        "0002_admin_audit",
        include_str!("../migrations/0002_admin_audit.sql"),
    ),
    (
        "0003_local_provenance",
        include_str!("../migrations/0003_local_provenance.sql"),
    ),
    (
        "0004_server_sync_cursors",
        include_str!("../migrations/0004_server_sync_cursors.sql"),
    ),
    (
        "0005_active_pointers_entity_type",
        include_str!("../migrations/0005_active_pointers_entity_type.sql"),
    ),
];

pub async fn apply_content_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    for &(name, sql) in CONTENT_MIGRATIONS {
        sqlx::raw_sql(sql).execute(pool).await.map_err(|e| {
            tracing::error!(migration = name, error = %e, "content migration failed");
            e
        })?;
    }
    tracing::info!(
        count = CONTENT_MIGRATIONS.len(),
        "content migrations applied"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::CONTENT_MIGRATIONS;

    #[test]
    fn every_ddl_statement_is_idempotent() {
        for (name, sql) in CONTENT_MIGRATIONS {
            for line in sql.lines() {
                let l = line.trim_start().to_ascii_uppercase();
                let creates = l.starts_with("CREATE TABLE")
                    || l.starts_with("CREATE INDEX")
                    || l.starts_with("CREATE UNIQUE INDEX");
                if creates {
                    assert!(
                        l.contains("IF NOT EXISTS"),
                        "{name}: non-idempotent DDL re-run on every boot: {line}"
                    );
                }
                if l.starts_with("ALTER TABLE") && l.contains("ADD COLUMN") {
                    assert!(
                        l.contains("IF NOT EXISTS"),
                        "{name}: non-idempotent ADD COLUMN: {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn migrations_are_ordered_and_complete() {
        let names: Vec<&str> = CONTENT_MIGRATIONS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names[0], "0001_content_schema");
        assert_eq!(names[3], "0004_server_sync_cursors");
        for (i, n) in names.iter().enumerate() {
            let want = format!("{:04}", i + 1);
            assert!(
                n.starts_with(&want),
                "migration {n} out of order at slot {i}"
            );
        }
    }
}
