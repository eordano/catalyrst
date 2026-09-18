//! C-ABI lifecycle tests: exercise the extern "C" surface directly, the way .NET will.

use std::ffi::CString;

use web_transport::cabi::{wt_host_create, wt_host_destroy, wt_host_service, WtConfig, WtEvent};

fn self_signed() -> (String, String) {
    let signed = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    (signed.cert.pem(), signed.key_pair.serialize_pem())
}

#[test]
fn create_with_null_config_returns_null() {
    let host = unsafe { wt_host_create(std::ptr::null()) };
    assert!(host.is_null());
}

#[test]
fn create_with_invalid_cert_returns_null() {
    let bind = CString::new("127.0.0.1:0").unwrap();
    let cert = CString::new("not a pem").unwrap();
    let key = CString::new("not a pem").unwrap();
    let config = WtConfig {
        bind_addr: bind.as_ptr(),
        cert_pem: cert.as_ptr(),
        key_pem: key.as_ptr(),
    };
    let host = unsafe { wt_host_create(&config) };
    assert!(host.is_null(), "invalid cert should fail to create a host");
}

#[test]
fn create_service_destroy_roundtrip() {
    let (cert_pem, key_pem) = self_signed();
    let bind = CString::new("127.0.0.1:0").unwrap();
    let cert = CString::new(cert_pem).unwrap();
    let key = CString::new(key_pem).unwrap();
    let config = WtConfig {
        bind_addr: bind.as_ptr(),
        cert_pem: cert.as_ptr(),
        key_pem: key.as_ptr(),
    };

    let host = unsafe { wt_host_create(&config) };
    assert!(!host.is_null(), "valid config should create a host");

    // No peers connected: a short service call must report no event.
    let mut event: WtEvent = unsafe { std::mem::zeroed() };
    let got = unsafe { wt_host_service(host, 50, &mut event) };
    assert_eq!(got, 0, "service should return 0 when no events are queued");

    unsafe { wt_host_destroy(host) };
}
