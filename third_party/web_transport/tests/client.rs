//! Integration tests for the `Client` API, driven against our own `Host` over loopback.

use std::time::Duration;

use web_transport::client::{Client, ClientConfig, ClientEvent};
use web_transport::host::{Event, Host, HostConfig};

struct TestCert {
    cert_pem: String,
    key_pem: String,
    cert_der: Vec<u8>,
}

fn self_signed() -> TestCert {
    let signed = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    TestCert {
        cert_pem: signed.cert.pem(),
        key_pem: signed.key_pair.serialize_pem(),
        cert_der: signed.cert.der().to_vec(),
    }
}

fn cert_hash(der: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(der).to_vec()
}

fn start_host(cert: &TestCert) -> Host {
    Host::new(HostConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
    })
    .expect("host should bind and start")
}

fn connect_client(cert: &TestCert, port: u16) -> Client {
    Client::connect(ClientConfig {
        url: format!("https://127.0.0.1:{port}/"),
        server_cert_hash: Some(cert_hash(&cert.cert_der)),
    })
    .expect("client should connect")
}

fn await_host_connect(host: &mut Host) -> u64 {
    match host
        .service(Duration::from_secs(5))
        .expect("expected a Connect event")
    {
        Event::Connect { peer_id, .. } => peer_id,
        other => panic!("expected Connect, got {other:?}"),
    }
}

fn drain_host_stream(host: &mut Host, peer_id: u64, expected_len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < expected_len {
        match host
            .service(Duration::from_secs(5))
            .expect("expected StreamData")
        {
            Event::StreamData { peer_id: pid, data } if pid == peer_id => {
                out.extend_from_slice(&data)
            }
            Event::StreamData { .. } => {}
            other => panic!("expected StreamData, got {other:?}"),
        }
    }
    out
}

fn drain_client_stream(client: &mut Client, expected_len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < expected_len {
        match client
            .service(Duration::from_secs(5))
            .expect("expected StreamData")
        {
            ClientEvent::StreamData { data } => out.extend_from_slice(&data),
            other => panic!("expected StreamData, got {other:?}"),
        }
    }
    out
}

#[test]
fn client_connects_and_roundtrips_both_directions() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();

    let mut client = connect_client(&cert, port);
    let peer_id = await_host_connect(&mut host);

    // ---- datagrams ----
    assert!(client.send_datagram(b"c2s-datagram"));
    match host.service(Duration::from_secs(5)).expect("host datagram") {
        Event::Datagram { peer_id: pid, data } => {
            assert_eq!(pid, peer_id);
            assert_eq!(&data[..], b"c2s-datagram");
        }
        other => panic!("expected Datagram, got {other:?}"),
    }

    assert!(host.send_datagram(peer_id, b"s2c-datagram"));
    match client
        .service(Duration::from_secs(5))
        .expect("client datagram")
    {
        ClientEvent::Datagram { data } => assert_eq!(&data[..], b"s2c-datagram"),
        other => panic!("expected Datagram, got {other:?}"),
    }

    // ---- reliable stream ----
    // The client's first stream write also establishes the bidi stream server-side.
    let c2s = b"client-to-server-stream";
    assert!(client.send_stream(c2s));
    assert_eq!(drain_host_stream(&mut host, peer_id, c2s.len()), c2s);

    let s2c = b"server-to-client-stream";
    assert!(host.send_stream(peer_id, s2c));
    assert_eq!(drain_client_stream(&mut client, s2c.len()), s2c);
}

#[test]
fn client_observes_host_initiated_disconnect() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();

    let mut client = connect_client(&cert, port);
    let peer_id = await_host_connect(&mut host);

    assert!(
        host.disconnect(peer_id, 0),
        "host.disconnect should succeed"
    );

    match client
        .service(Duration::from_secs(5))
        .expect("expected a Disconnected event")
    {
        ClientEvent::Disconnected { .. } => {}
        other => panic!("expected Disconnected, got {other:?}"),
    }
}
