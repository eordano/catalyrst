use std::collections::HashSet;
use std::sync::OnceLock;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "cat_admin";

const DEFAULT_TTL_SECS: i64 = 43200;

fn session_secret() -> Option<&'static String> {
    static S: OnceLock<Option<String>> = OnceLock::new();
    S.get_or_init(|| match std::env::var("SESSION_SECRET") {
        Ok(s) if !s.trim().is_empty() => Some(s),
        _ => None,
    })
    .as_ref()
}

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

fn is_allowed(addr: &str) -> bool {
    admin_addresses().contains(addr)
}

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

pub(crate) fn now_unix() -> i64 {
    now()
}

fn sign(secret: &str, payload_b64: &str) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload_b64.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

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

pub fn verify(cookie_val: &str) -> Option<String> {
    let secret = session_secret()?;
    let (p_b64, sig_b64) = cookie_val.split_once('.')?;

    let sig = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(p_b64.as_bytes());
    mac.verify_slice(&sig).ok()?;

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

pub fn admin_enabled() -> bool {
    session_secret().is_some() && !admin_addresses().is_empty()
}

pub(crate) fn mac_b64(msg: &str) -> Option<String> {
    let secret = session_secret()?;
    Some(URL_SAFE_NO_PAD.encode(sign(secret, msg)))
}

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

pub fn clear_cookie_header() -> String {
    format!("{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn install_env() {
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
