#[path = "walking_benchmark/auth.rs"]
mod auth;
#[path = "walking_benchmark/relay.rs"]
mod relay;
#[path = "walking_benchmark/state.rs"]
mod state;
#[path = "walking_benchmark/wire.rs"]
mod wire;

use auth::{packet, Identity};
use catalyrst_pulse::{
    decentraland::pulse as p,
    transport::{
        webtransport::{WtConfig, WtHost},
        Host, HostConfig, Transports,
    },
    v4, PulseServer,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use wire::Wire;

fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() - 1) as f64 * fraction).round() as usize])
}

async fn scenario(
    transport: &str,
    mode: &str,
    others: usize,
    rtt: u64,
    seconds: u32,
    phase_ms: u64,
    peer_mode: &str,
) -> serde_json::Value {
    let mut server = PulseServer::new();
    server
        .v4
        .enable(v4::PulseV4Config {
            audience: "walking-benchmark".into(),
            issuer: "loopback".into(),
            challenge_ttl_ms: 15000,
            max_pending: 128,
            max_auth_chain_bytes: 3072,
            max_capabilities: 32,
            max_public_detail_bytes: 128,
            max_frame_bytes: 4096,
        })
        .unwrap();
    let host = Host::bind(HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        max_peers: 128,
        channel_limit: 3,
    })
    .await
    .unwrap();
    let enet_addr = host.local_addr().unwrap();
    let (transports, addr, certificate) = if transport == "quic" {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let hash = Sha256::digest(cert.cert.der().as_ref()).to_vec();
        let (wt, events) = tokio::task::spawn_blocking(move || {
            WtHost::start(WtConfig {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                cert_pem: cert.cert.pem(),
                key_pem: cert.signing_key.serialize_pem(),
                slot_base: 4095,
                slot_capacity: 128,
                max_datagram_bytes: 1200,
                max_message_bytes: 4096,
                service_timeout_ms: 1,
                server_full_reason: 6,
            })
        })
        .await
        .unwrap()
        .unwrap();
        let addr = wt.local_addr();
        (
            Transports::with_webtransport(host, 4095, wt, events),
            addr,
            Some(hash),
        )
    } else {
        (Transports::enet_only(host, 4095), enet_addr, None)
    };
    let server_started = Instant::now();
    let server = tokio::spawn(server.serve(transports, 50));
    let identities: Vec<_> = (0..=others).map(Identity::new).collect();
    let wallets: HashMap<_, _> = identities
        .iter()
        .enumerate()
        .map(|(i, id)| (id.address.clone(), i))
        .collect();
    let mut clients = vec![];
    let now = Instant::now();
    let mut history: Vec<_> = (0..=others)
        .map(|i| vec![(now, state::walking(i, 0)); 2])
        .collect();
    for (index, identity) in identities.iter().enumerate().take(others) {
        let mut client = Wire::connect(addr, certificate.clone()).await;
        identity
            .authenticate(
                &mut client,
                if peer_mode == "same" { mode } else { peer_mode },
                state::walking(index, 0),
            )
            .await;
        teleport(&client, index);
        clients.push(client);
    }
    let relay = relay::Relay::start(addr, rtt).await;
    let began = Instant::now();
    let mut observer = Wire::connect(relay.addr, certificate.clone()).await;
    let transport_ms = began.elapsed().as_secs_f64() * 1000.0;
    identities[others]
        .authenticate(&mut observer, mode, state::walking(others, 0))
        .await;
    let auth_ms = began.elapsed().as_secs_f64() * 1000.0;
    let setup = relay.counts();
    teleport(&observer, others);
    let mut received = state::Receiver::new(wallets);
    let mut scene_ready_ms = if others == 0 { Some(auth_ms) } else { None };
    let warm_until = tokio::time::Instant::now() + Duration::from_millis(300 + rtt * 2);
    loop {
        tokio::select! {
            data=observer.input.recv() => {
                let (time,bytes)=data.unwrap();
                for request in received.handle(p::ServerMessage::decode(bytes.as_slice()).unwrap(),time,&history) { observer.send(true,packet(p::client_message::Message::Resync(request))); }
                if scene_ready_ms.is_none() && received.subjects.len()==others { scene_ready_ms=Some(began.elapsed().as_secs_f64()*1000.0); }
            }
            _=tokio::time::sleep_until(warm_until) => break,
        }
    }
    assert_eq!(
        received.subjects.len(),
        others,
        "every nearby participant must join"
    );
    let first_scene_ms = began.elapsed().as_secs_f64() * 1000.0;
    let start_counts = relay.counts();
    let walk_started = Instant::now();
    let frames = seconds * 10;
    let mut last_sent_frame = vec![0u32; others + 1];
    let next = (server_started.elapsed().as_millis() as u64 / 100 + 1) * 100 + phase_ms;
    let mut ticker = tokio::time::interval_at(
        (server_started + Duration::from_millis(next)).into(),
        Duration::from_millis(100),
    );
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut frame = 0;
    while frame < frames {
        tokio::select! {
            _=ticker.tick() => {
                frame+=1;
                for (index,client) in clients.iter_mut().enumerate() {
                    while client.input.try_recv().is_ok() {}
                    let state=state::walking(index,frame);
                    if state != history[index].last().unwrap().1 || frame-last_sent_frame[index]>=10 {
                        history[index].push((Instant::now(),state));
                        last_sent_frame[index]=frame;
                        client.send(false,packet(p::client_message::Message::Input(p::PlayerStateInput {state:Some(state)})));
                    }
                }
                let state=state::walking(others,frame);
                if state != history[others].last().unwrap().1 || frame-last_sent_frame[others]>=10 {
                    history[others].push((Instant::now(),state));
                    last_sent_frame[others]=frame;
                    observer.send(false,packet(p::client_message::Message::Input(p::PlayerStateInput {state:Some(state)})));
                }
            }
            data=observer.input.recv() => {
                let (time,bytes)=data.unwrap();
                for request in received.handle(p::ServerMessage::decode(bytes.as_slice()).unwrap(),time,&history) {observer.send(true,packet(p::client_message::Message::Resync(request)));}
            }
        }
    }
    let drain_until = tokio::time::Instant::now() + Duration::from_millis(250 + rtt * 2);
    loop {
        tokio::select! {
            data=observer.input.recv() => {
                let (time,bytes)=data.unwrap();
                for request in received.handle(p::ServerMessage::decode(bytes.as_slice()).unwrap(),time,&history) {observer.send(true,packet(p::client_message::Message::Resync(request)));}
            }
            _=tokio::time::sleep_until(drain_until) => break,
        }
    }
    let counts = relay.counts() - start_counts;
    assert!(
        received.failures.is_empty(),
        "{}",
        received
            .failures
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    for subject in received.subjects.values() {
        assert_eq!(
            subject.sequence,
            history[subject.index].len() as u32 - 1,
            "all inputs must be accepted and final state must converge"
        );
        assert_eq!(subject.state, state::walking(subject.index, frames));
    }
    if others > 0 {
        assert!(
            received.updates * 10
                >= history[..others].iter().map(|h| h.len() - 2).sum::<usize>() * 8,
            "at least 80% of sent movement updates must be applied"
        );
    }
    let result = serde_json::json!({"transport":transport,"mode":mode,"others":others,"rtt_ms":rtt,"seconds":seconds,"elapsed_ms":walk_started.elapsed().as_secs_f64()*1000.0,
        "setup_up_bytes":setup.up,"setup_down_bytes":setup.down,"transport_ready_ms":transport_ms,"authenticated_ms":auth_ms,
        "scene_ready_ms":scene_ready_ms,"scene_warmup_finished_ms":first_scene_ms,"phase_ms":phase_ms,"other_client_mode":if peer_mode=="same" {mode} else {peer_mode},"auth_roundtrips":if mode=="v4" {2}else{1},
        "up_bytes":counts.up,"down_bytes":counts.down,"total_bytes":counts.up+counts.down,"packets":counts.packets,
        "sent_updates":history[..others].iter().map(|h|h.len()-2).sum::<usize>(),"observer_uploads":history[others].len()-2,"movement_updates":received.updates,"batches":received.batches,"dictionary_batches":received.dictionary_batches,"resyncs":received.resyncs,
        "movement_p50_ms":percentile(&received.latencies,0.5),"movement_p95_ms":percentile(&received.latencies,0.95),"movement_max_ms":percentile(&received.latencies,1.0),
        "correct":true});
    drop(observer);
    drop(clients);
    server.abort();
    let _ = server.await;
    result
}

fn teleport(wire: &Wire, index: usize) {
    let state = state::walking(index, 0);
    wire.send(
        true,
        packet(p::client_message::Message::Teleport(p::TeleportRequest {
            parcel_index: state.parcel_index,
            position_x: state.position_x,
            position_y: state.position_y,
            position_z: state.position_z,
            realm: "walking-benchmark".into(),
        })),
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(args.len() % 2 == 0, "arguments are --name value pairs");
    let mut options = HashMap::new();
    for pair in args.chunks(2) {
        assert!(
            [
                "--seconds",
                "--rtt-ms",
                "--transports",
                "--modes",
                "--participants",
                "--repeats",
                "--peer-mode"
            ]
            .contains(&pair[0].as_str()),
            "unknown argument"
        );
        options.insert(pair[0].as_str(), pair[1].as_str());
    }
    let get = |key, default| *options.get(key).unwrap_or(&default);
    let seconds: u32 = get("--seconds", "10").parse().unwrap();
    assert!((1..=120).contains(&seconds));
    let repeats: usize = get("--repeats", "1").parse().unwrap();
    for repeat in 0..repeats {
        for transport in get("--transports", "enet,quic").split(',') {
            assert!(["enet", "quic"].contains(&transport));
            for rtt in get("--rtt-ms", "0,40")
                .split(',')
                .map(|s| s.parse::<u64>().unwrap())
            {
                for others in get("--participants", "0,1,10,40")
                    .split(',')
                    .map(|s| s.parse::<usize>().unwrap())
                {
                    assert!(others <= 100);
                    for mode in get("--modes", "legacy,negotiated,v4").split(',') {
                        assert!(["legacy", "negotiated", "v4"].contains(&mode));
                        let peer_mode = get("--peer-mode", "same");
                        assert!(["same", "legacy"].contains(&peer_mode));
                        let mut result = scenario(
                            transport,
                            mode,
                            others,
                            rtt,
                            seconds,
                            [7, 23, 41][repeat % 3],
                            peer_mode,
                        )
                        .await;
                        result["repeat"] = repeat.into();
                        println!("{result}");
                    }
                }
            }
        }
    }
}
