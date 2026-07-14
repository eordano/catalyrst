use sqlx::PgConnection;

/// Advisory-locked quota core shared by the economy meta-tx reservation and
/// the comms report/ban rate limits: takes the transaction-scoped
/// `pg_advisory_xact_lock(hashtext($1))` on `lock_key`, then runs
/// `count_sql` (its own `$1` bound to the same `lock_key`) and returns the
/// count. Never commits or rolls back -- callers compare against their own
/// threshold and keep using the same transaction for whatever comes next.
///
/// `lock_key` is `&str`, not a pre-hashed `i64`: every call site locks on
/// postgres's own `hashtext()` of a plain address string, and re-deriving
/// that hash in Rust would risk not matching it.
pub async fn advisory_locked_count(
    conn: &mut PgConnection,
    lock_key: &str,
    count_sql: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(lock_key)
        .execute(&mut *conn)
        .await?;

    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql))
        .bind(lock_key)
        .fetch_one(&mut *conn)
        .await
}
