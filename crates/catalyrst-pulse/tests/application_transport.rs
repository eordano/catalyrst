use std::io::ErrorKind;
use std::time::{Duration, Instant};

use catalyrst_pulse::transport::webtransport::{WtConfig, WtHost};
use catalyrst_pulse::transport::{Event, Host, HostConfig, Packet};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn enet_application_budget_holds_until_acks_and_reports_disconnected_peers() {
    let config = || HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        max_peers: 2,
        channel_limit: 2,
    };
    let mut server = Host::bind(config()).await.unwrap();
    let mut client = Host::bind(config()).await.unwrap();
    client.connect(server.local_addr().unwrap(), 2).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let peer = loop {
        assert!(Instant::now() < deadline, "ENet connect timed out");
        if let Some(Event::Connect { peer, .. }) = server.service().await.unwrap() {
            break peer;
        }
        client.service().await.unwrap();
    };
    let mut queued = 0;
    loop {
        match server
            .send_application(peer, Packet::reliable(0, vec![7; 1024]))
            .await
        {
            Ok(()) => queued += 1,
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::WouldBlock);
                break;
            }
        }
        assert!(queued < 64, "ENet pending application bytes exceeded bound");
    }
    assert!(queued > 0);
    server.flush();
    assert_eq!(
        server
            .send_application(peer, Packet::reliable(0, vec![7; 1024]))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut received = 0;
    while received < queued {
        assert!(Instant::now() < deadline, "ENet queued data did not drain");
        if let Some(Event::Receive { packet, .. }) = client.service().await.unwrap() {
            assert_eq!(packet.data.as_ref(), &[7; 1024]);
            received += 1;
        }
        server.service().await.unwrap();
    }
    while server
        .send_application(peer, Packet::reliable(0, vec![8; 1024]))
        .await
        .is_err()
    {
        assert!(
            Instant::now() < deadline,
            "ENet acknowledgement did not release permits"
        );
        client.service().await.unwrap();
        server.service().await.unwrap();
    }
    server.disconnect_now(peer, 1).await.unwrap();
    assert_eq!(
        server
            .send_application(peer, Packet::reliable(0, vec![9]))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::NotConnected
    );
}

fn event(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(event) = rx.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "WebTransport event timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn webtransport_budget_survives_both_queues_and_close_releases_blocked_writes() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let hash = Sha256::digest(cert.cert.der()).to_vec();
    let (host, mut events) = WtHost::start(WtConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem: cert.cert.pem(),
        key_pem: cert.signing_key.serialize_pem(),
        slot_base: 100,
        slot_capacity: 1,
        max_message_bytes: 4096,
        max_datagram_bytes: 1200,
        service_timeout_ms: 1,
        server_full_reason: 6,
    })
    .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let connect = || {
        rt.block_on(async {
            let client = web_transport_quinn::ClientBuilder::new()
                .with_server_certificate_hashes(vec![hash.clone()])
                .unwrap();
            client
                .connect(url::Url::parse(&format!("https://{}/", host.local_addr())).unwrap())
                .await
                .unwrap()
        })
    };
    let session = connect();
    let Event::Connect { peer, .. } = event(&mut events) else {
        panic!("expected connect")
    };
    let mut queued = 0;
    loop {
        match host.send_application(peer as u32, Packet::reliable(0, vec![7; 1024])) {
            Ok(()) => queued += 1,
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::WouldBlock);
                break;
            }
        }
        assert!(queued < 64, "WebTransport application bytes exceeded bound");
    }
    // The client deliberately has not opened the bidi stream: draining the outer queue
    // must not free permits while the downstream writer is still blocked in accept_bi.
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        host.send_application(peer as u32, Packet::reliable(0, vec![7; 1024]))
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    rt.block_on(async { session.close(0, b"done") });
    assert!(matches!(event(&mut events), Event::Disconnect { .. }));
    assert_eq!(
        host.send_application(peer as u32, Packet::reliable(0, vec![7]))
            .unwrap_err()
            .kind(),
        ErrorKind::NotConnected
    );
    let next = connect();
    let Event::Connect {
        peer: next_peer, ..
    } = event(&mut events)
    else {
        panic!("expected reconnect")
    };
    assert_eq!(peer, next_peer);
    host.send_application(next_peer as u32, Packet::reliable(0, b"fresh".to_vec()))
        .unwrap();
    let received = rt.block_on(async {
        let (mut send, mut recv) = next.open_bi().await.unwrap();
        send.write_all(&[0, 0, 0, 0]).await.unwrap();
        let mut bytes = [0; 9];
        tokio::time::timeout(Duration::from_secs(3), recv.read_exact(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        bytes
    });
    assert_eq!(
        &received, b"\0\0\0\x05fresh",
        "a reused slot must not receive old application data"
    );
    rt.block_on(async { next.close(0, b"done") });
}

#[test]
fn webtransport_unacknowledged_stream_bytes_remain_bounded_after_write() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let hash = Sha256::digest(cert.cert.der()).to_vec();
    let (host, mut events) = WtHost::start(WtConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem: cert.cert.pem(),
        key_pem: cert.signing_key.serialize_pem(),
        slot_base: 100,
        slot_capacity: 1,
        max_message_bytes: 4096,
        max_datagram_bytes: 1200,
        service_timeout_ms: 1,
        server_full_reason: 6,
    })
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (session, _send, _recv) = rt.block_on(async {
        let client = web_transport_quinn::ClientBuilder::new()
            .with_server_certificate_hashes(vec![hash])
            .unwrap();
        let session = client
            .connect(url::Url::parse(&format!("https://{}/", host.local_addr())).unwrap())
            .await
            .unwrap();
        let (mut send, recv) = session.open_bi().await.unwrap();
        send.write_all(&[0, 0, 0, 0]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        (session, send, recv)
    });
    let Event::Connect { peer, .. } = event(&mut events) else {
        panic!("expected connect")
    };
    // The current-thread client runtime now stops polling QUIC, withholding ACKs
    // after opening the stream. Completed writes must not bypass the queue cap
    // through Quinn's independent send buffer.
    let mut accepted = 0;
    let mut blocked = 0;
    while blocked < 20 {
        match host.send_application(peer as u32, Packet::reliable(0, vec![7; 1024])) {
            Ok(()) => {
                accepted += 1024;
                blocked = 0;
                assert!(
                    accepted <= 128 * 1024,
                    "QUIC send buffer exceeded its bound"
                );
            }
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::WouldBlock);
                blocked += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        accepted >= 64 * 1024,
        "stream never consumed the queued bytes"
    );
    rt.block_on(async { session.close(0, b"done") });
}
