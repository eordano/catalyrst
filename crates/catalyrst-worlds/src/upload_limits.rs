//! Process-wide multipart upload budgets shared by every multipart route: admission takes a concurrency slot plus a zero-byte lease grown from actual parsed payload bytes -- never from Content-Length, which includes multipart framing.

use std::sync::atomic::{AtomicU64, Ordering};

use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::handlers::deploy::DeployRejection;

pub const DEFAULT_MAX_IN_FLIGHT_UPLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub const DEFAULT_MAX_CONCURRENT_UPLOADS: u64 = 40;

pub const DEFAULT_MAX_IN_FLIGHT_UPLOAD_FILES: u64 = 40_000;

pub const DEFAULT_MULTIPART_UPLOAD_TIMEOUT_MS: u64 = 300_000;

pub const DEFAULT_DEPLOYMENT_PROCESSING_TIMEOUT_MS: u64 = 300_000;

pub const BYTES_SHED_MESSAGE: &str = "Server is buffering too many uploads, please retry shortly.";
pub const CONCURRENCY_SHED_MESSAGE: &str =
    "Server is handling too many concurrent uploads, please retry shortly.";
pub const FILES_SHED_MESSAGE: &str =
    "Server is buffering too many upload files, please retry shortly.";
pub const MULTIPART_TIMEOUT_MESSAGE: &str = "The multipart upload timed out.";
pub const PAYLOAD_TOO_LARGE_MESSAGE: &str = "The multipart request is too large.";
pub const INVALID_CONTENT_LENGTH_MESSAGE: &str = "Invalid Content-Length header.";
pub const TOO_MANY_FIELDS_MESSAGE: &str = "The multipart request has too many fields.";
pub const TOO_MANY_PARTS_MESSAGE: &str = "The multipart request has too many parts.";

pub const MAX_MULTIPART_FIELD_VALUE_BYTES: usize = 1024 * 1024;

pub const MAX_MULTIPART_FIELDS: usize = 100;

pub const MAX_MULTIPART_PARTS: usize = 10_100;

static IN_FLIGHT_UPLOAD_BYTES: AtomicU64 = AtomicU64::new(0);
static IN_FLIGHT_UPLOAD_FILES: AtomicU64 = AtomicU64::new(0);
static ACTIVE_UPLOADS: AtomicU64 = AtomicU64::new(0);

fn try_resize_counter(counter: &AtomicU64, lease: &mut u64, new_total: u64, max: u64) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let others = current.saturating_sub(*lease);
        let Some(next) = others.checked_add(new_total) else {
            return false;
        };
        if next > max {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                *lease = new_total;
                return true;
            }
            Err(actual) => current = actual,
        }
    }
}

pub struct InFlightBytesGuard(u64);

impl InFlightBytesGuard {
    pub fn reserved(&self) -> u64 {
        self.0
    }

    /// Re-targets the lease at `new_total` reserved bytes; on failure the reservation is unchanged.
    pub fn try_resize(&mut self, new_total: u64, max: u64) -> bool {
        try_resize_counter(&IN_FLIGHT_UPLOAD_BYTES, &mut self.0, new_total, max)
    }
}

impl Drop for InFlightBytesGuard {
    fn drop(&mut self) {
        IN_FLIGHT_UPLOAD_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

pub fn reserve_in_flight() -> InFlightBytesGuard {
    InFlightBytesGuard(0)
}

pub fn in_flight_upload_bytes() -> u64 {
    IN_FLIGHT_UPLOAD_BYTES.load(Ordering::Acquire)
}

pub struct InFlightFilesGuard(u64);

impl InFlightFilesGuard {
    pub fn reserved(&self) -> u64 {
        self.0
    }

    /// Re-targets the lease at `new_total` reserved files; on failure the reservation is unchanged.
    pub fn try_resize(&mut self, new_total: u64, max: u64) -> bool {
        try_resize_counter(&IN_FLIGHT_UPLOAD_FILES, &mut self.0, new_total, max)
    }
}

impl Drop for InFlightFilesGuard {
    fn drop(&mut self) {
        IN_FLIGHT_UPLOAD_FILES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

pub fn reserve_in_flight_files() -> InFlightFilesGuard {
    InFlightFilesGuard(0)
}

pub fn in_flight_upload_files() -> u64 {
    IN_FLIGHT_UPLOAD_FILES.load(Ordering::Acquire)
}

pub enum PayloadAccountError {
    PayloadTooLarge,
    BudgetExhausted,
}

/// Accounts newly parsed payload bytes, growing `lease` to the running total; the per-request payload cap is checked before the shared budget.
pub fn account_payload_bytes(
    total_bytes: &mut usize,
    added: usize,
    max_payload_bytes: usize,
    lease: &mut InFlightBytesGuard,
    max_in_flight_bytes: u64,
) -> Result<(), PayloadAccountError> {
    *total_bytes = total_bytes.saturating_add(added);
    if *total_bytes > max_payload_bytes {
        return Err(PayloadAccountError::PayloadTooLarge);
    }
    if !lease.try_resize(*total_bytes as u64, max_in_flight_bytes) {
        return Err(PayloadAccountError::BudgetExhausted);
    }
    Ok(())
}

/// A request's declared Content-Length, distinguishing an absent header from a malformed one.
pub enum DeclaredContentLength {
    Absent,
    /// Present but not matching `^[0-9]+$`.
    Invalid,
    Bytes(u64),
}

pub fn declared_content_length(headers: &HeaderMap) -> DeclaredContentLength {
    let Some(value) = headers.get(header::CONTENT_LENGTH) else {
        return DeclaredContentLength::Absent;
    };
    let Ok(s) = value.to_str() else {
        return DeclaredContentLength::Invalid;
    };
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return DeclaredContentLength::Invalid;
    }
    match s.parse::<u64>() {
        Ok(n) => DeclaredContentLength::Bytes(n),
        Err(_) => DeclaredContentLength::Bytes(u64::MAX),
    }
}

pub struct UploadSlotGuard(());

impl Drop for UploadSlotGuard {
    fn drop(&mut self) {
        ACTIVE_UPLOADS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Admits an upload only while fewer than `max` uploads are active, independently of the byte budget.
pub fn try_acquire_upload_slot(max: u64) -> Option<UploadSlotGuard> {
    let mut current = ACTIVE_UPLOADS.load(Ordering::Acquire);
    loop {
        if current >= max {
            return None;
        }
        match ACTIVE_UPLOADS.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(UploadSlotGuard(())),
            Err(actual) => current = actual,
        }
    }
}

pub fn active_uploads() -> u64 {
    ACTIVE_UPLOADS.load(Ordering::Acquire)
}

pub fn shed_response(message: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "5")],
        Json(DeployRejection {
            error: "Service Unavailable".to_string(),
            message: message.to_string(),
        }),
    )
        .into_response()
}

pub fn timeout_response(message: &str) -> Response {
    (
        StatusCode::REQUEST_TIMEOUT,
        Json(DeployRejection {
            error: "Request Timeout".to_string(),
            message: message.to_string(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The statics are process-wide and cargo runs tests concurrently, so every
    // test here must fully release what it acquires and assert only on deltas
    // it created itself (never on absolute counter values).

    #[test]
    fn upload_slot_cap_is_enforced_and_released_on_drop() {
        let a = try_acquire_upload_slot(u64::MAX).expect("first slot");
        let base = active_uploads();
        let b = try_acquire_upload_slot(base + 1).expect("slot under cap");
        assert!(try_acquire_upload_slot(active_uploads()).is_none());
        drop(b);
        drop(a);
    }

    #[test]
    fn byte_lease_admission_is_free_and_grows_from_parsed_bytes() {
        let mut a = reserve_in_flight();
        assert_eq!(a.reserved(), 0);
        assert!(a.try_resize(100, u64::MAX));
        assert_eq!(a.reserved(), 100);
        assert!(a.try_resize(40, u64::MAX));
        assert_eq!(a.reserved(), 40);
        assert!(in_flight_upload_bytes() >= 40);

        let cap = in_flight_upload_bytes();
        let mut b = reserve_in_flight();
        assert!(!b.try_resize(cap.saturating_add(1), cap));
        assert_eq!(b.reserved(), 0);
        assert!(b.try_resize(1, u64::MAX));
        drop(b);
        drop(a);
    }

    #[test]
    fn resize_rejects_sums_that_would_overflow_even_under_a_max_budget() {
        let mut a = reserve_in_flight();
        assert!(a.try_resize(2, u64::MAX));
        let mut b = reserve_in_flight();
        assert!(!b.try_resize(u64::MAX, u64::MAX));
        assert_eq!(b.reserved(), 0);
        drop(a);
    }

    #[test]
    fn busboy_default_limits_and_messages_match_upstream() {
        assert_eq!(MAX_MULTIPART_FIELD_VALUE_BYTES, 1024 * 1024);
        assert_eq!(MAX_MULTIPART_FIELDS, 100);
        assert_eq!(MAX_MULTIPART_PARTS, 10_100);
        assert_eq!(
            TOO_MANY_FIELDS_MESSAGE,
            "The multipart request has too many fields."
        );
        assert_eq!(
            TOO_MANY_PARTS_MESSAGE,
            "The multipart request has too many parts."
        );
    }

    #[test]
    fn files_lease_bounds_the_aggregate_and_message_matches_upstream() {
        assert_eq!(
            FILES_SHED_MESSAGE,
            "Server is buffering too many upload files, please retry shortly."
        );
        let mut a = reserve_in_flight_files();
        assert!(a.try_resize(5, u64::MAX));
        assert_eq!(a.reserved(), 5);
        let cap = in_flight_upload_files();
        let mut b = reserve_in_flight_files();
        assert!(!b.try_resize(cap.saturating_add(1), cap));
        assert_eq!(b.reserved(), 0);
        assert!(b.try_resize(1, u64::MAX));
        drop(b);
        drop(a);
    }

    #[test]
    fn payload_accounting_checks_payload_cap_before_budget() {
        let mut lease = reserve_in_flight();
        let mut total = 0usize;
        assert!(account_payload_bytes(&mut total, 10, 100, &mut lease, u64::MAX).is_ok());
        assert_eq!(total, 10);
        assert_eq!(lease.reserved(), 10);

        assert!(matches!(
            account_payload_bytes(&mut total, 1000, 100, &mut lease, 0),
            Err(PayloadAccountError::PayloadTooLarge)
        ));

        let mut lease2 = reserve_in_flight();
        let mut total2 = 0usize;
        assert!(matches!(
            account_payload_bytes(&mut total2, 10, 100, &mut lease2, 0),
            Err(PayloadAccountError::BudgetExhausted)
        ));
        assert_eq!(lease2.reserved(), 0);
    }

    #[test]
    fn content_length_parsing_distinguishes_absent_invalid_and_numeric() {
        use axum::http::HeaderValue;

        let headers = HeaderMap::new();
        assert!(matches!(
            declared_content_length(&headers),
            DeclaredContentLength::Absent
        ));

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("1234"));
        assert!(matches!(
            declared_content_length(&headers),
            DeclaredContentLength::Bytes(1234)
        ));

        for bad in ["+5", "-5", "abc", "12 ", " 12", "1.5", "0x10", ""] {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from_str(bad).unwrap());
            assert!(
                matches!(
                    declared_content_length(&headers),
                    DeclaredContentLength::Invalid
                ),
                "expected Invalid for {bad:?}"
            );
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_static("99999999999999999999999999"),
        );
        assert!(matches!(
            declared_content_length(&headers),
            DeclaredContentLength::Bytes(u64::MAX)
        ));
    }

    #[tokio::test]
    async fn shed_and_timeout_responses_match_upstream_wire_shape() {
        let shed = shed_response(CONCURRENCY_SHED_MESSAGE);
        assert_eq!(shed.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            shed.headers().get("Retry-After").unwrap().to_str().unwrap(),
            "5"
        );
        let body = axum::body::to_bytes(shed.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            v,
            json!({
                "error": "Service Unavailable",
                "message": "Server is handling too many concurrent uploads, please retry shortly."
            })
        );

        let to = timeout_response(MULTIPART_TIMEOUT_MESSAGE);
        assert_eq!(to.status(), StatusCode::REQUEST_TIMEOUT);
        assert!(to.headers().get("Retry-After").is_none());
        let body = axum::body::to_bytes(to.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            v,
            json!({
                "error": "Request Timeout",
                "message": "The multipart upload timed out."
            })
        );
    }
}
