use std::net::{SocketAddr, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use catalyrst_pulse::{
    batch::{encode_batches, BatchSubject, SeqEncoding, MAX_BATCH_BYTES},
    decentraland::pulse,
    transport::{Event, Host, HostConfig, Packet},
};
use prost::Message;

struct Relay {
    addr: SocketAddr,
    bytes: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Relay {
    fn start(server: SocketAddr) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let addr = socket.local_addr().unwrap();
        let bytes = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, stopping) = (bytes.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut client = None;
            let mut buffer = [0; 65536];
            while !stopping.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buffer) {
                    Ok((len, source)) => {
                        let target = if source == server {
                            count.fetch_add(len + 28, Ordering::SeqCst);
                            client.expect("client initiated connection")
                        } else {
                            client = Some(source);
                            server
                        };
                        socket.send_to(&buffer[..len], target).unwrap();
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => panic!("relay: {error}"),
                }
            }
        });
        Self {
            addr,
            bytes,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn config() -> HostConfig {
    HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        max_peers: 2,
        channel_limit: 2,
    }
}

fn movement_packets(batch: bool, tick: u32) -> Vec<Vec<u8>> {
    let expected: Vec<_> = (4096..4135)
        .map(|subject_id| pulse::PlayerStateDeltaTier0 {
            subject_id,
            baseline_seq: 5000 + tick,
            new_seq: 5001 + tick,
            server_tick: 100000 + tick * 33,
            position_x: Some(128 + tick),
            position_y: Some(4200),
            position_z: Some(200),
            velocity_x: Some(40),
            velocity_z: Some(41),
            rotation_y: Some(64),
            movement_blend: Some(10),
            ..Default::default()
        })
        .collect();
    let messages: Vec<_> = if batch {
        let subjects: Vec<_> = expected
            .iter()
            .map(|delta| BatchSubject::from_delta(delta, 1))
            .collect();
        encode_batches(
            expected[0].server_tick,
            &subjects,
            MAX_BATCH_BYTES,
            SeqEncoding::AbsoluteBaseline,
        )
        .into_iter()
        .map(|encoded| {
            pulse::server_message::Message::PlayerStateDeltaBatchBaseline(
                pulse::PlayerStateDeltaBatch {
                    server_tick: encoded.server_tick,
                    subject_count: encoded.subject_count,
                    payload: encoded.payload,
                },
            )
        })
        .collect()
    } else {
        expected
            .iter()
            .copied()
            .map(pulse::server_message::Message::PlayerStateDelta)
            .collect()
    };
    messages
        .into_iter()
        .map(|message| {
            pulse::ServerMessage {
                message: Some(message),
            }
            .encode_to_vec()
        })
        .collect()
}

async fn measure(batch: bool) -> usize {
    let mut server = Host::bind(config()).await.unwrap();
    let relay = Relay::start(server.local_addr().unwrap());
    let mut client = Host::bind(config()).await.unwrap();
    client.connect(relay.addr, 2).unwrap();
    let mut peer = None;
    let mut connected = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while peer.is_none() || !connected {
        assert!(Instant::now() < deadline, "connection timeout");
        if let Some(Event::Connect { peer: id, .. }) = server.service().await.unwrap() {
            peer = Some(id);
        }
        if let Some(Event::Connect { .. }) = client.service().await.unwrap() {
            connected = true;
        }
    }
    client.flush();
    for _ in 0..3 {
        let _ = server.service().await.unwrap();
    }
    let start = relay.bytes.load(Ordering::SeqCst);
    for tick in 0..20 {
        let tick_started = tokio::time::Instant::now();
        let expected_packets = movement_packets(batch, tick);
        for payload in &expected_packets {
            server
                .send(peer.unwrap(), Packet::unreliable(1, payload.clone()))
                .await
                .unwrap();
        }
        server.flush();
        let mut received = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while received.len() < expected_packets.len() {
            assert!(Instant::now() < deadline, "movement timeout");
            if let Some(Event::Receive { packet, .. }) = client.service().await.unwrap() {
                received.push(packet.data.to_vec());
            }
        }
        assert_eq!(received, expected_packets);
        let _ = server.service().await.unwrap();
        tokio::time::sleep_until(tick_started + Duration::from_millis(33)).await;
    }
    relay.bytes.load(Ordering::SeqCst) - start
}

#[tokio::test]
async fn crowded_movement_halves_enet_ipv4_egress_bytes() {
    let legacy = measure(false).await;
    let negotiated = measure(true).await;
    println!("39 subjects, 20 ticks, ENet + UDP + IPv4 egress: legacy={legacy}, negotiated={negotiated}, reduction={:.2}%", 100.0 * (1.0 - negotiated as f64 / legacy as f64));
    assert!(
        negotiated * 2 <= legacy,
        "50% network-byte target missed: {negotiated}/{legacy}"
    );
}

fn measure_webtransport(batch: bool) -> usize {
    use catalyrst_pulse::transport::webtransport::{WtConfig, WtHost};
    use sha2::{Digest, Sha256};
    use web_transport::client::{Client, ClientConfig, ClientEvent};

    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let hash = Sha256::digest(certificate.cert.der().as_ref()).to_vec();
    let (host, mut events) = WtHost::start(WtConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        cert_pem: certificate.cert.pem(),
        key_pem: certificate.signing_key.serialize_pem(),
        slot_base: 4095,
        slot_capacity: 2,
        max_datagram_bytes: 1200,
        max_message_bytes: 4096,
        service_timeout_ms: 1,
        server_full_reason: 6,
    })
    .unwrap();
    let relay = Relay::start(host.local_addr());
    let mut client = Client::connect(ClientConfig {
        url: format!("https://{}/", relay.addr),
        server_cert_hash: Some(hash),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let peer = loop {
        assert!(Instant::now() < deadline, "WebTransport connection timeout");
        if let Ok(Event::Connect { peer, .. }) = events.try_recv() {
            break peer;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    host.send(peer as u32, Packet::unreliable(1, vec![0]));
    loop {
        assert!(Instant::now() < deadline, "WebTransport warmup timeout");
        if let Some(ClientEvent::Datagram { data }) = client.service(Duration::from_millis(10)) {
            assert_eq!(data.as_ref(), &[0]);
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    let start = relay.bytes.load(Ordering::SeqCst);
    for tick in 0..20 {
        let tick_started = Instant::now();
        let mut expected = movement_packets(batch, tick);
        for payload in &expected {
            host.send(peer as u32, Packet::unreliable(1, payload.clone()));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut received = Vec::new();
        while received.len() < expected.len() {
            assert!(Instant::now() < deadline, "WebTransport movement timeout");
            match client.service(Duration::from_millis(10)) {
                Some(ClientEvent::Datagram { data }) => received.push(data.to_vec()),
                Some(ClientEvent::Disconnected { reason }) => {
                    panic!("WebTransport disconnected: {reason}")
                }
                _ => {}
            }
        }
        expected.sort();
        received.sort();
        assert_eq!(received, expected);
        std::thread::sleep(Duration::from_millis(33).saturating_sub(tick_started.elapsed()));
    }
    let bytes = relay.bytes.load(Ordering::SeqCst) - start;
    client.disconnect(0);
    bytes
}

#[test]
fn crowded_movement_halves_webtransport_ipv4_egress_bytes() {
    let legacy = measure_webtransport(false);
    let negotiated = measure_webtransport(true);
    println!("39 subjects, 20 ticks, QUIC + UDP + IPv4 egress: legacy={legacy}, negotiated={negotiated}, reduction={:.2}%", 100.0 * (1.0 - negotiated as f64 / legacy as f64));
    assert!(
        negotiated * 2 <= legacy,
        "50% network-byte target missed: {negotiated}/{legacy}"
    );
}
