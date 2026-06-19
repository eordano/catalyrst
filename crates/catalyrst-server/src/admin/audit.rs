//! Shared admin audit log.
//!
//! Every admin-console mutation — both the content-local handlers in
//! [`crate::admin::api`] and the cross-service proxy controls — records a row in
//! the `admin_audit` table (migration `0002_admin_audit.sql`) keyed by the
//! authenticated wallet address recovered from the [`AdminSession`] cookie. This
//! is the single "who did what, when" surface the admin-console design
//! (docs/admin-console.md §4) calls for.
//!
//! Recording is best-effort and never blocks or fails the mutation it describes:
//! a logging error is logged via `tracing` and swallowed, and when no content DB
//! pool is configured (the stub binary) `record` is a no-op. The table is
//! append-only by convention (the application only ever INSERTs).
//!
//! [`AdminSession`]: crate::admin::auth::AdminSession

use std::sync::OnceLock;

use serde_json::Value;
use sqlx::PgPool;

/// Process-global audit pool, set once at router construction. Lets handlers that
/// don't carry `State<Arc<AppState>>` (the original first-tranche proxy controls,
/// whose signatures the gate tests depend on) still write audit rows via
/// [`record_global`]. The pool is `PgPool` (cheaply cloneable; `Arc` inside).
static AUDIT_POOL: OnceLock<Option<PgPool>> = OnceLock::new();

/// Install the audit pool. Idempotent (first call wins); call from `build_router`.
pub fn set_global_pool(pool: Option<PgPool>) {
    let _ = AUDIT_POOL.set(pool);
}

fn global_pool() -> Option<&'static PgPool> {
    AUDIT_POOL.get().and_then(|o| o.as_ref())
}

/// Like [`record`] but uses the process-global pool installed by
/// [`set_global_pool`]. For handlers that don't extract `State`.
pub async fn record_global(
    admin_addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    result: &str,
) {
    record(global_pool(), admin_addr, action, target, detail, result).await
}

/// Append one audit row.
///
/// - `pool`      — the content DB pool (`AppState::audit_pool`); `None` ⇒ no-op.
/// - `admin_addr`— the authenticated admin address (`AdminSession::address`).
/// - `action`    — short verb identifying the control, e.g. `"content.read-only"`.
/// - `target`    — the object acted on (an entity id, a denylist id, …) or `None`.
/// - `detail`    — arbitrary structured context (request body echo, params, …).
/// - `result`    — outcome marker, e.g. `"ok"`, `"error"`, `"unsupported"`.
///
/// Errors are logged and swallowed so auditing can never break a mutation.
pub async fn record(
    pool: Option<&PgPool>,
    admin_addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    result: &str,
) {
    let Some(pool) = pool else {
        // No content DB (stub binary). Nothing to persist; surface in logs so
        // the action is still observable.
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
