use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use catalyrst_pulse::transport::webtransport::config::{
    DEFAULT_MAX_DATAGRAM_BYTES, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_SERVICE_TIMEOUT_MS,
};
use catalyrst_pulse::transport::webtransport::framing::{datagram_frame, stream_frame};
use catalyrst_pulse::transport::webtransport::{WtConfig, WtHost};
use catalyrst_pulse::transport::{Event, Packet};

use sha2::{Digest, Sha256};
use web_transport::client::{Client, ClientConfig};

const SLOT_BASE: u32 = 4095;

fn dev_cert() -> (String, String, Vec<u8>) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("generate dev cert");
    let cert_pem = certified.cert.pem();
    let key_pem = certified.key_pair.serialize_pem();
    let hash = Sha256::digest(certified.cert.der().as_ref()).to_vec();
    (cert_pem, key_pem, hash)
}

fn wait_for<F>(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>,
    deadline: Instant,
    mut pred: F,
) -> Option<Event>
where
    F: FnMut(&Event) -> bool,
{
    while Instant::now() < deadline {
        match rx.try_recv() {
            Ok(ev) if pred(&ev) => return Some(ev),
            Ok(_) => continue,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    None
}

#[test]
fn webtransport_stream_and_datagram_roundtrip() {
    let (cert_pem, key_pem, cert_hash) = dev_cert();

    let (host, mut events) = WtHost::start(WtConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem,
        key_pem,
        slot_base: SLOT_BASE,
        slot_capacity: 8,
        max_datagram_bytes: DEFAULT_MAX_DATAGRAM_BYTES,
        max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        service_timeout_ms: DEFAULT_SERVICE_TIMEOUT_MS,
        server_full_reason: 6,
    })
    .expect("start WtHost");

    let url = format!("https://127.0.0.1:{}/", host.local_addr().port());
    let client = Client::connect(ClientConfig {
        url,
        server_cert_hash: Some(cert_hash),
    })
    .expect("client connect");

    let deadline = Instant::now() + Duration::from_secs(10);

    let connect = wait_for(&mut events, deadline, |e| {
        matches!(e, Event::Connect { .. })
    })
    .expect("Connect event");
    let Event::Connect { peer, ip } = connect else {
        unreachable!()
    };
    assert_eq!(peer as u32, SLOT_BASE, "first WT peer takes the base slot");
    assert_eq!(ip.as_deref(), Some("127.0.0.1"));

    assert!(client.send_stream(&stream_frame(b"HELLO-RELIABLE")));
    let recv = wait_for(&mut events, deadline, |e| {
        matches!(e, Event::Receive { .. })
    })
    .expect("stream Receive");
    match recv {
        Event::Receive {
            channel, packet, ..
        } => {
            assert_eq!(channel, 0);
            assert_eq!(&packet.data[..], b"HELLO-RELIABLE");
        }
        _ => unreachable!(),
    }

    assert!(client.send_datagram(&datagram_frame(1, 0, b"POS-0")));
    let r0 = wait_for(&mut events, deadline, |e| {
        matches!(e, Event::Receive { .. })
    })
    .expect("first datagram Receive");
    match r0 {
        Event::Receive {
            channel, packet, ..
        } => {
            assert_eq!(channel, 1);
            assert_eq!(&packet.data[..], b"POS-0");
        }
        _ => unreachable!(),
    }
    assert!(client.send_datagram(&datagram_frame(1, 0, b"POS-STALE")));
    assert!(client.send_datagram(&datagram_frame(1, 1, b"POS-1")));
    let r1 = wait_for(
        &mut events,
        deadline,
        |e| matches!(e, Event::Receive { packet, .. } if &packet.data[..] == b"POS-1"),
    )
    .expect("newer datagram Receive");
    match r1 {
        Event::Receive { packet, .. } => assert_eq!(&packet.data[..], b"POS-1"),
        _ => unreachable!(),
    }
    assert!(
        wait_for(
            &mut events,
            Instant::now() + Duration::from_millis(300),
            |e| { matches!(e, Event::Receive { packet, .. } if &packet.data[..] == b"POS-STALE") }
        )
        .is_none(),
        "stale datagram must be dropped, not delivered"
    );
    assert_eq!(
        host.metrics()
            .datagrams_dropped_stale
            .load(Ordering::Relaxed),
        1,
        "exactly one stale datagram dropped"
    );

    assert!(client.send_datagram(&datagram_frame(2, 0, b"UNSEQ")));
    let r2 = wait_for(&mut events, deadline, |e| {
        matches!(e, Event::Receive { channel: 2, .. })
    })
    .expect("unsequenced datagram Receive");
    match r2 {
        Event::Receive {
            channel, packet, ..
        } => {
            assert_eq!(channel, 2);
            assert_eq!(&packet.data[..], b"UNSEQ");
        }
        _ => unreachable!(),
    }

    host.send(
        peer as u32,
        Packet::reliable(0, vec![0u8; DEFAULT_MAX_MESSAGE_BYTES + 100]),
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        host.metrics()
            .messages_dropped_oversize
            .load(Ordering::Relaxed),
        1,
        "oversize reliable message must be dropped and counted, not sent"
    );

    client.disconnect(0);
    let disc_deadline = Instant::now() + Duration::from_secs(5);
    let disc = wait_for(&mut events, disc_deadline, |e| {
        matches!(e, Event::Disconnect { .. })
    })
    .expect("Disconnect event");
    assert!(matches!(disc, Event::Disconnect { peer: p } if p as u32 == SLOT_BASE));
    drop(client);
}
