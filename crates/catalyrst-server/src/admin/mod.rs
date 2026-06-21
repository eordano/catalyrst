//! Admin console authentication + mutation controls.
//!
//! WO-1 lands the foundation: a stateless HMAC session cookie (`session`) and the
//! wallet sign-in handshake + `AdminSession` request gate (`auth`). WO-2 adds the
//! proxy + content-local mutation handlers (`api`). Every mutation records a row
//! in the shared `admin_audit` log (`audit`). Everything is default-safe: with
//! `ADMIN_ADDRESSES` and `SESSION_SECRET` unset, `session::admin_enabled()` is
//! false and every gated endpoint returns 403.

pub mod api;
pub mod audit;
pub mod auth;
pub mod session;

pub use audit::record;
