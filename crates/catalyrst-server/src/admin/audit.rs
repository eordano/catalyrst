use std::sync::OnceLock;

use serde_json::Value;
use sqlx::PgPool;

static AUDIT_POOL: OnceLock<Option<PgPool>> = OnceLock::new();

pub fn set_global_pool(pool: Option<PgPool>) {
    let _ = AUDIT_POOL.set(pool);
}

fn global_pool() -> Option<&'static PgPool> {
    AUDIT_POOL.get().and_then(|o| o.as_ref())
}

pub async fn record_global(
    admin_addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    result: &str,
) {
    record(global_pool(), admin_addr, action, target, detail, result).await
}

pub async fn record(
    pool: Option<&PgPool>,
    admin_addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    result: &str,
) {
    let Some(pool) = pool else {
        tracing::info!(
            admin = %admin_addr,
            action = %action,
            target = ?target,
            result = %result,
            "admin mutation (no audit pool; not persisted)"
        );
        return;
    };

    let res = sqlx::query(
        r#"
        INSERT INTO admin_audit (admin_address, action, target, detail, result)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(admin_addr)
    .bind(action)
    .bind(target)
    .bind(detail)
    .bind(result)
    .execute(pool)
    .await;

    if let Err(e) = res {
        tracing::error!(
            error = %e,
            admin = %admin_addr,
            action = %action,
            "failed to write admin_audit row (mutation already applied)"
        );
    }
}
