//! Runtime configuration for catalyrst-quests.

/// Address the REST/WS HTTP server binds to (`QUESTS_BIND`, default `0.0.0.0:5155`).
pub fn bind_addr() -> String {
    std::env::var("QUESTS_BIND").unwrap_or_else(|_| "0.0.0.0:5155".to_string())
}

/// Postgres DSN for the quests database (`QUESTS_DATABASE_URL`).
pub fn database_url() -> Option<String> {
    std::env::var("QUESTS_DATABASE_URL").ok()
}

/// Signed-auth-chain handshake window in seconds (`QUESTS_AUTH_WINDOW_SECS`,
/// default 300 — matches upstream's `authenticate_dcl_user_with_signed_headers`
/// 30s server tolerance plus the 5-minute signature freshness window).
pub fn auth_window_secs() -> i64 {
    std::env::var("QUESTS_AUTH_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::auth_chain::FIVE_MINUTES_SECS)
}
