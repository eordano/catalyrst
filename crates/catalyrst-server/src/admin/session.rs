//! Stateless HMAC-signed admin session cookie.
//!
//! A session is a base64url(payload).base64url(hmac_sha256(secret, payload))
//! string carried in the `cat_admin` cookie. The payload is a small JSON object
//! `{addr, exp}`. Verification re-derives the HMAC over the payload segment with
//! a constant-time compare, checks expiry, and re-checks allowlist membership —
//! so revoking an address (removing it from `ADMIN_ADDRESSES`) invalidates any
//! outstanding cookie on the next request without server-side session state.
//!
//! Default-safe: with `SESSION_SECRET` unset or `ADMIN_ADDRESSES` empty,
//! `admin_enabled()` is false, `mint` returns `None`, and `verify` returns
//! `None` for everything — every mutation endpoint fails closed.

use std::collections::HashSet;
use std::sync::OnceLock;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "cat_admin";

const DEFAULT_TTL_SECS: i64 = 43200; // 12h

/// `SESSION_SECRET` — the HMAC key. Absent ⇒ admin auth disabled. Memoized.
fn session_secret() -> Option<&'static String> {
    static S: OnceLock<Option<String>> = OnceLock::new();
    S.get_or_init(|| match std::env::var("SESSION_SECRET") {
        Ok(s) if !s.trim().is_empty() => Some(s),
        _ => None,
    })
    .as_ref()
}

/// `ADMIN_ADDRESSES` — comma-separated allowlist, lowercased + trimmed. Empty
/// set ⇒ admin disabled. Memoized.
fn admin_addresses() -> &'static HashSet<String> {
    static A: OnceLock<HashSet<String>> = OnceLock::new();
    A.get_or_init(|| {
        let mut set = HashSet::new();
        if let Ok(raw) = std::env::var("ADMIN_ADDRESSES") {
            for a in raw.split(',') {
                let a = a.trim().to_lowercase();
                if !a.is_empty() {
                    set.insert(a);
                }
            }
        }
        set
    })
}

/// True iff `addr` (already lowercased) is in the allowlist.
fn is_allowed(addr: &str) -> bool {
    admin_addresses().contains(addr)
}

/// Session lifetime in seconds, from `ADMIN_SESSION_TTL_SECS` (default 12h).
fn ttl_secs() -> i64 {
    static T: OnceLock<i64> = OnceLock::new();
    *T.get_or_init(|| {
        std::env::var("ADMIN_SESSION_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_TTL_SECS)
    })
}

#[derive(Serialize, Deserialize)]
pub struct SessionPayload {
    pub addr: String,
    pub exp: i64,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Current Unix time in seconds (for the auth challenge expiry).
pub(crate) fn now_unix() -> i64 {
    now()
}

fn sign(secret: &str, payload_b64: &str) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload_b64.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Mint a cookie value for `addr`. Returns `None` when no secret is configured.
pub fn mint(addr: &str) -> Option<String> {
    let secret = session_secret()?;
    let payload = SessionPayload {
        addr: addr.to_lowercase(),
        exp: now() + ttl_secs(),
    };
    let json = serde_json::to_vec(&payload).ok()?;
    let p_b64 = URL_SAFE_NO_PAD.encode(json);
    let sig = sign(secret, &p_b64);
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
    Some(format!("{p_b64}.{sig_b64}"))
}

/// Verify a cookie value, returning the authenticated address on success.
/// Any tampering, expiry, or allowlist miss ⇒ `None`.
pub fn verify(cookie_val: &str) -> Option<String> {
    let secret = session_secret()?;
    let (p_b64, sig_b64) = cookie_val.split_once('.')?;

    // Constant-time HMAC verification.
    let sig = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(p_b64.as_bytes());
    mac.verify_slice(&sig).ok()?;

    // Decode + check the payload.
    let json = URL_SAFE_NO_PAD.decode(p_b64).ok()?;
    let payload: SessionPayload = serde_json::from_slice(&json).ok()?;
    if payload.exp <= now() {
        return None;
    }
    let addr = payload.addr.to_lowercase();
    if !is_allowed(&addr) {
        return None;
    }
    Some(addr)
}

/// True iff admin write controls are configured (a secret AND ≥1 allowed addr).
/// SSR uses this to decide whether to render controls at all (default-safe).
pub fn admin_enabled() -> bool {
    session_secret().is_some() && !admin_addresses().is_empty()
}

/// HMAC-SHA256(`SESSION_SECRET`, `msg`) as base64url, or `None` when no secret
/// is configured. Used to mint a **stateless** sign-in nonce bound to the
/// requesting host + address + expiry (so no server-side nonce store is needed,
/// and the nonce can't be replayed against a different host or address).
pub(crate) fn mac_b64(msg: &str) -> Option<String> {
    let secret = session_secret()?;
    Some(URL_SAFE_NO_PAD.encode(sign(secret, msg)))
}

/// Constant-time verify that `mac_b64_str` is `mac_b64(msg)`. `false` on any
/// mismatch, decode error, or when no secret is configured.
pub(crate) fn mac_verify(msg: &str, mac_b64_str: &str) -> bool {
    let Some(secret) = session_secret() else {
        return false;
    };
    let Ok(provided) = URL_SAFE_NO_PAD.decode(mac_b64_str) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(msg.as_bytes());
    mac.verify_slice(&provided).is_ok()
}

/// `Set-Cookie` header value carrying `value`. `secure` adds the `Secure` flag
/// (derived by the caller from the forwarded scheme).
pub fn set_cookie_header(value: &str, secure: bool) -> String {
    let mut s = format!(
        "{COOKIE_NAME}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        ttl_secs()
    );
    if secure {
        s.push_str("; Secure");
    }
    s
}

/// `Set-Cookie` header value that clears the session cookie.
pub fn clear_cookie_header() -> String {
    format!("{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env + the memoized OnceLocks are process-global; serialize the tests that
    // mutate them. These tests set the env directly and bypass the memoized
    // accessors by exercising the pure crypto via a locally-keyed helper where
    // possible — but mint/verify read the memoized secret, so we install it once.
    static LOCK: Mutex<()> = Mutex::new(());

    fn install_env() {
        // SESSION_SECRET + ADMIN_ADDRESSES are read through OnceLock, so they are
        // captured the first time any test touches them. Set before first use.
        std::env::set_var("SESSION_SECRET", "test-secret-key-0123456789");
        std::env::set_var(
            "ADMIN_ADDRESSES",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA,0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
    }

    #[test]
    fn round_trip_mint_verify() {
        let _g = LOCK.lock().unwrap();
        install_env();
        let addr = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let cookie = mint(addr).expect("secret configured");
        assert_eq!(verify(&cookie).as_deref(), Some(addr));
    }

    #[test]
    fn mint_lowercases_address() {
        let _g = LOCK.lock().unwrap();
        install_env();
        let cookie = mint("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
        assert_eq!(
            verify(&cookie).as_deref(),
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn tampered_signature_fails() {
        let _g = LOCK.lock().unwrap();
        install_env();
        let cookie = mint("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let (p, _sig) = cookie.split_once('.').unwrap();
        let forged = format!("{p}.{}", URL_SAFE_NO_PAD.encode([0u8; 32]));
        assert!(verify(&forged).is_none());
    }

    #[test]
    fn tampered_payload_fails() {
        let _g = LOCK.lock().unwrap();
        install_env();
        let cookie = mint("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let (_p, sig) = cookie.split_once('.').unwrap();
        // Swap the payload to a different (still-allowed) address but keep the
        // original signature — must fail because the HMAC won't match.
        let other = SessionPayload {
            addr: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            exp: now() + 100,
        };
        let p2 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&other).unwrap());
        let forged = format!("{p2}.{sig}");
        assert!(verify(&forged).is_none());
    }

    #[test]
    fn expired_fails() {
        let _g = LOCK.lock().unwrap();
        install_env();
        // Hand-craft an expired-but-correctly-signed cookie.
        let secret = session_secret().unwrap();
        let payload = SessionPayload {
            addr: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            exp: now() - 1,
        };
        let p_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let sig_b64 = URL_SAFE_NO_PAD.encode(sign(secret, &p_b64));
        let cookie = format!("{p_b64}.{sig_b64}");
        assert!(verify(&cookie).is_none());
    }

    #[test]
    fn addr_not_in_allowlist_fails() {
        let _g = LOCK.lock().unwrap();
        install_env();
        let secret = session_secret().unwrap();
        let payload = SessionPayload {
            addr: "0xcccccccccccccccccccccccccccccccccccccccc".into(),
            exp: now() + 100,
        };
        let p_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let sig_b64 = URL_SAFE_NO_PAD.encode(sign(secret, &p_b64));
        let cookie = format!("{p_b64}.{sig_b64}");
        assert!(verify(&cookie).is_none());
    }

    #[test]
    fn malformed_cookie_fails() {
        let _g = LOCK.lock().unwrap();
        install_env();
        assert!(verify("not-a-cookie").is_none());
        assert!(verify("").is_none());
        assert!(verify("a.b.c").is_none());
    }
}
