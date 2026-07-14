//! Outbound HTTP: a uniformly configured [`reqwest::Client`], SSRF guards for
//! user-supplied URLs, a size-capped body reader, and a retry ladder.

mod body;
mod client;
mod guard;
mod retry;

pub use body::read_body_capped;
pub use client::{http_client, try_http_client, HttpClientCfg, USER_AGENT};
pub use guard::{is_private_or_internal_host, is_safe_http_url, resolve_and_pin};
pub use retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
