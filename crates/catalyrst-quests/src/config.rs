/// Loopback like every other catalyrst HTTP service: public traffic arrives through the
/// nginx front. Set QUESTS_BIND to bind elsewhere.
pub fn bind_addr() -> String {
    std::env::var("QUESTS_BIND").unwrap_or_else(|_| "127.0.0.1:5155".to_string())
}

pub fn database_url() -> Option<String> {
    std::env::var("QUESTS_DATABASE_URL").ok()
}

pub fn auth_window_secs() -> i64 {
    std::env::var("QUESTS_AUTH_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::auth_chain::FIVE_MINUTES_SECS)
}
