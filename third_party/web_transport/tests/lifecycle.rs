//! Lifecycle tests for the `Host` Rust API (no network peer involved).

use web_transport::host::{Host, HostConfig};

/// Generate a throwaway self-signed cert/key PEM pair for binding the endpoint.
fn self_signed() -> (String, String) {
    let signed = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("rcgen should produce a self-signed cert");
    (signed.cert.pem(), signed.key_pair.serialize_pem())
}

#[test]
fn new_binds_and_reports_os_assigned_port() {
    let (cert_pem, key_pem) = self_signed();

    let host = Host::new(HostConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem,
        key_pem,
    })
    .expect("host should bind and start");

    let addr = host.local_addr();
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_ne!(
        addr.port(),
        0,
        "the OS should assign a concrete port for :0"
    );
}
