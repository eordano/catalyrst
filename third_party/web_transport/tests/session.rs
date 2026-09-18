//! Session-level integration tests: a real `web-transport-quinn` client connects to the
//! host over loopback and we assert the events the host produces.

use std::time::Duration;

use web_transport::host::{Event, Host, HostConfig};

struct TestCert {
    cert_pem: String,
    key_pem: String,
    cert_der: Vec<u8>,
}

/// A throwaway self-signed cert. The client trusts it by SHA-256 hash (the WebTransport
/// `serverCertificateHashes` mechanism), so no SAN/DNS matching is needed.
fn self_signed() -> TestCert {
    let signed = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("rcgen should produce a self-signed cert");
    TestCert {
        cert_pem: signed.cert.pem(),
        key_pem: signed.key_pair.serialize_pem(),
        cert_der: signed.cert.der().to_vec(),
    }
}

fn start_host(cert: &TestCert) -> Host {
    Host::new(HostConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
    })
    .expect("host should bind and start")
}

/// Connect a client to the host on its own runtime; returns the runtime (keep it alive) and
/// the established session.
fn connect(cert_der: &[u8], port: u16) -> (tokio::runtime::Runtime, web_transport_quinn::Session) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let session = rt.block_on(async {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(cert_der).to_vec();
        let client = web_transport_quinn::ClientBuilder::new()
            .with_server_certificate_hashes(vec![hash])
            .expect("client builder");
        let url = url::Url::parse(&format!("https://127.0.0.1:{port}/")).unwrap();
        client.connect(url).await.expect("client should connect")
    });
    (rt, session)
}

#[test]
fn client_connection_emits_connect_event() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();

    // Keep the client runtime and session alive for the duration of the assertion.
    let (_rt, _session) = connect(&cert.cert_der, port);

    let event = host
        .service(Duration::from_secs(5))
        .expect("expected a Connect event");

    match event {
        Event::Connect {
            peer_id,
            remote_addr,
        } => {
            assert_eq!(peer_id, 1, "the first peer should get id 1");
            assert!(
                remote_addr.starts_with("127.0.0.1"),
                "unexpected remote_addr: {remote_addr}"
            );
        }
        other => panic!("expected Connect, got {other:?}"),
    }
}

/// Drain events until a `Connect` arrives and return its peer id.
fn await_connect(host: &mut Host) -> u64 {
    match host
        .service(Duration::from_secs(5))
        .expect("expected a Connect event")
    {
        Event::Connect { peer_id, .. } => peer_id,
        other => panic!("expected Connect, got {other:?}"),
    }
}

#[test]
fn datagram_roundtrips_through_the_host_api() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();
    let (rt, session) = connect(&cert.cert_der, port);
    let peer_id = await_connect(&mut host);

    // client -> server
    let payload = b"hello-datagram";
    rt.block_on(async {
        session
            .send_datagram(bytes::Bytes::from_static(payload))
            .expect("client should send a datagram");
    });

    match host
        .service(Duration::from_secs(5))
        .expect("expected a Datagram event")
    {
        Event::Datagram { peer_id: pid, data } => {
            assert_eq!(pid, peer_id);
            assert_eq!(&data[..], payload);
        }
        other => panic!("expected Datagram, got {other:?}"),
    }

    // server -> client (echo)
    assert!(
        host.send_datagram(peer_id, payload),
        "host.send_datagram should succeed"
    );
    let echoed = rt.block_on(async {
        session
            .read_datagram()
            .await
            .expect("client should receive the echoed datagram")
    });
    assert_eq!(&echoed[..], payload);
}

#[test]
fn client_close_emits_disconnect_event() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();
    let (rt, session) = connect(&cert.cert_der, port);
    let peer_id = await_connect(&mut host);

    rt.block_on(async { session.close(0u32, b"bye") });

    match host
        .service(Duration::from_secs(5))
        .expect("expected a Disconnect event")
    {
        Event::Disconnect { peer_id: pid, .. } => assert_eq!(pid, peer_id),
        other => panic!("expected Disconnect, got {other:?}"),
    }
}

/// Drain `StreamData` events for `peer_id` until at least `expected_len` bytes are collected.
/// Stream bytes may arrive across multiple chunks.
fn drain_stream_bytes(host: &mut Host, peer_id: u64, expected_len: usize) -> Vec<u8> {
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

#[test]
fn reliable_stream_roundtrips_through_the_host_api() {
    let cert = self_signed();
    let mut host = start_host(&cert);
    let port = host.local_addr().port();
    let (rt, session) = connect(&cert.cert_der, port);
    let peer_id = await_connect(&mut host);

    // client opens the reliable bidi stream and writes a message
    let request = b"ping-over-stream";
    let (send, recv) = rt.block_on(async {
        let (mut send, recv) = session
            .open_bi()
            .await
            .expect("client should open a bidi stream");
        send.write_all(request)
            .await
            .expect("client should write to the stream");
        (send, recv)
    });

    // server observes the bytes
    let received = drain_stream_bytes(&mut host, peer_id, request.len());
    assert_eq!(received, request);

    // server -> client (echo on the same stream)
    let reply = b"pong-over-stream";
    assert!(
        host.send_stream(peer_id, reply),
        "host.send_stream should succeed"
    );
    let echoed = rt.block_on(async {
        let mut recv = recv;
        let mut buf = vec![0u8; reply.len()];
        recv.read_exact(&mut buf)
            .await
            .expect("client should read the echoed bytes");
        buf
    });
    assert_eq!(&echoed[..], reply);

    drop(send);
}
