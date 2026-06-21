use std::collections::HashMap;
use std::io::Write as _;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use prost::Message as _;
use sha2::{Digest, Sha256};

use catalyrst_pulse::batch::decode_batch;
use catalyrst_pulse::decentraland::pulse::{
    client_message, server_message, ClientMessage, HandshakeRequest, PlayerState, PlayerStateInput,
    ServerMessage, TeleportRequest,
};
use catalyrst_pulse::transport::webtransport::framing::{
    datagram_frame, stream_frame, StreamFrameReader,
};
use web_transport::client::{Client, ClientConfig, ClientEvent};

const REALM: &str = "netcode-baseline";
const PARCEL_INDEX: i32 = 5;
const TICK_HZ: f64 = 20.0;
const CHANNEL_SEQUENCED: u8 = 1;
const STREAM_MSG_CAP: usize = 4096;

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_num<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn dev_cert() -> (String, String, Vec<u8>) {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let hash = Sha256::digest(certified.cert.der().as_ref()).to_vec();
    (
        certified.cert.pem(),
        certified.key_pair.serialize_pem(),
        hash,
    )
}

fn free_udp_port() -> u16 {
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.local_addr().unwrap().port()
}

async fn sign_handshake(ts_ms: i64, protocol_features: u32) -> Vec<u8> {
    use alloy::signers::{local::PrivateKeySigner, Signer};
    use catalyrst_pulse::handshake::build_signed_fetch_payload;
    use catalyrst_types::{AuthLink, AuthLinkType};

    let root = PrivateKeySigner::random();
    let root_addr = format!("{:#x}", root.address());
    let ephemeral = PrivateKeySigner::random();
    let eph_addr = format!("{:#x}", ephemeral.address());

    let ts = ts_ms.to_string();
    let metadata = "{\"signer\":\"dcl:explorer\"}";
    let connect_payload = build_signed_fetch_payload("connect", "/", &ts, metadata);
    let eph_payload = format!(
        "Decentraland Login\nEphemeral address: {eph_addr}\nExpiration: 2099-01-01T00:00:00.000Z"
    );
    let eph_sig = root.sign_message(eph_payload.as_bytes()).await.unwrap();
    let final_sig = ephemeral
        .sign_message(connect_payload.as_bytes())
        .await
        .unwrap();

    let chain = [
        AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: root_addr,
            signature: None,
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaEphemeral,
            payload: eph_payload,
            signature: Some(eph_sig.to_string()),
        },
        AuthLink {
            link_type: AuthLinkType::EcdsaSignedEntity,
            payload: connect_payload,
            signature: Some(final_sig.to_string()),
        },
    ];
    let mut map = serde_json::Map::new();
    for (i, link) in chain.iter().enumerate() {
        map.insert(
            format!("x-identity-auth-chain-{i}"),
            serde_json::Value::String(serde_json::to_string(link).unwrap()),
        );
    }
    map.insert("x-identity-timestamp".into(), serde_json::Value::String(ts));
    map.insert(
        "x-identity-metadata".into(),
        serde_json::Value::String(metadata.into()),
    );
    let bag = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();

    ClientMessage {
        message: Some(client_message::Message::Handshake(HandshakeRequest {
            auth_chain: bag.into_bytes(),
            profile_version: 0,
            initial_state: None,
            protocol_features,
        })),
    }
    .encode_to_vec()
}

fn grounded_run_state(idx: usize, t: f64) -> PlayerState {
    let phase = idx as f64 * (std::f64::consts::TAU / 40.0);
    let w = 1.5;
    let radius = 5.0_f64;
    let angle = phase + w * t;
    let px = 8.0 + radius * angle.cos();
    let pz = 8.0 + radius * angle.sin();
    let vx = -radius * w * angle.sin();
    let vz = radius * w * angle.cos();
    let py = 1.0 + 0.30 * (2.0 * std::f64::consts::PI * 1.3 * t).sin();
    let rot = angle.rem_euclid(std::f64::consts::TAU).to_degrees();
    let blend = 1.0 + 0.35 * (2.0 * std::f64::consts::PI * 1.5 * t).sin();

    let mut s = PlayerState {
        parcel_index: PARCEL_INDEX,
        ..Default::default()
    };
    s.set_position_x_f(px as f32);
    s.set_position_y_f(py as f32);
    s.set_position_z_f(pz as f32);
    s.set_velocity_x_f(vx as f32);
    s.set_velocity_z_f(vz as f32);
    s.set_rotation_y_f(rot as f32);
    s.set_movement_blend_f(blend as f32);
    s
}

fn teleport_bytes() -> Vec<u8> {
    let mut t = TeleportRequest {
        parcel_index: PARCEL_INDEX,
        realm: REALM.to_string(),
        ..Default::default()
    };
    t.set_position_x_f(8.0);
    t.set_position_y_f(1.0);
    t.set_position_z_f(8.0);
    ClientMessage {
        message: Some(client_message::Message::Teleport(t)),
    }
    .encode_to_vec()
}

fn connect_and_join(
    url: &str,
    cert_hash: &[u8],
    handshake: &[u8],
    rtt: Duration,
) -> anyhow::Result<(Client, StreamFrameReader)> {
    let mut client = Client::connect(ClientConfig {
        url: url.to_string(),
        server_cert_hash: Some(cert_hash.to_vec()),
    })
    .map_err(|e| anyhow::anyhow!("connect: {e}"))?;

    thread::sleep(rtt / 2);
    if !client.send_stream(&stream_frame(handshake)) {
        anyhow::bail!("handshake send failed");
    }

    let mut reader = StreamFrameReader::new(STREAM_MSG_CAP);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut authed = false;
    while Instant::now() < deadline && !authed {
        match client.service(Duration::from_millis(200)) {
            Some(ClientEvent::StreamData { data }) => {
                reader.append(&data);
                while let Ok(Some(frame)) = reader.try_read() {
                    if let Ok(ServerMessage {
                        message: Some(server_message::Message::Handshake(h)),
                    }) = ServerMessage::decode(&frame[..])
                    {
                        if !h.success {
                            anyhow::bail!("handshake rejected: {:?}", h.error);
                        }
                        authed = true;
                    }
                }
            }
            Some(ClientEvent::Disconnected { reason }) => {
                anyhow::bail!("disconnected during handshake (reason {reason})")
            }
            _ => {}
        }
    }
    if !authed {
        anyhow::bail!("no HandshakeResponse within 15s");
    }

    thread::sleep(rtt / 2);
    if !client.send_stream(&stream_frame(&teleport_bytes())) {
        anyhow::bail!("teleport send failed");
    }
    Ok((client, reader))
}

#[derive(Default)]
struct BotResult {
    recv_datagram_bytes: u64,
    recv_datagram_count: u64,
    recv_stream_bytes: u64,
    recv_stream_count: u64,
    sent_inputs: u64,
    deltas: Vec<(Instant, usize, usize)>,
    rtt_us: Vec<u64>,
    recv_batch_datagrams: u64,
    recv_batch_subjects: u64,
    recv_batch_bytes: u64,
}

fn count_delta_fields(d: &catalyrst_pulse::decentraland::pulse::PlayerStateDeltaTier0) -> usize {
    [
        d.parcel_index.is_some(),
        d.position_x.is_some(),
        d.position_y.is_some(),
        d.position_z.is_some(),
        d.velocity_x.is_some(),
        d.velocity_y.is_some(),
        d.velocity_z.is_some(),
        d.rotation_y.is_some(),
        d.movement_blend.is_some(),
        d.slide_blend.is_some(),
        d.head_yaw.is_some(),
        d.head_pitch.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count()
}

fn run_bot(
    mut client: Client,
    mut reader: StreamFrameReader,
    idx: usize,
    is_observer: bool,
    stop: Arc<AtomicBool>,
    counting: Arc<AtomicBool>,
) -> BotResult {
    let mut res = BotResult::default();
    let start = Instant::now();
    let period = Duration::from_secs_f64(1.0 / TICK_HZ);
    let mut next_send = Instant::now();
    let mut seq: u32 = 1;
    let mut last_rtt = Instant::now();
    let mut last_known: HashMap<u32, u32> = HashMap::new();

    while !stop.load(Ordering::Relaxed) {
        while let Some(ev) = client.service(Duration::from_millis(1)) {
            let on = counting.load(Ordering::Relaxed);
            match ev {
                ClientEvent::Datagram { data } => {
                    if on {
                        res.recv_datagram_bytes += data.len() as u64;
                        res.recv_datagram_count += 1;
                    }
                    if is_observer {
                        match ServerMessage::decode(&data[..]) {
                            Ok(ServerMessage {
                                message: Some(server_message::Message::PlayerStateDelta(d)),
                            }) => {
                                last_known.insert(d.subject_id, d.new_seq);
                                if on {
                                    res.deltas.push((
                                        Instant::now(),
                                        data.len(),
                                        count_delta_fields(&d),
                                    ));
                                }
                            }
                            Ok(ServerMessage {
                                message: Some(server_message::Message::PlayerStateDeltaBatch(b)),
                            }) => {
                                if let Ok(subjects) =
                                    decode_batch(b.subject_count, &b.payload, |id| {
                                        last_known.get(&id).copied().unwrap_or(0)
                                    })
                                {
                                    let n = subjects.len().max(1);
                                    let per_subject = data.len() / n;
                                    let arrival = Instant::now();
                                    for s in &subjects {
                                        last_known.insert(s.subject_id, s.new_seq);
                                        if on {
                                            res.deltas.push((
                                                arrival,
                                                per_subject,
                                                s.present_field_count(),
                                            ));
                                        }
                                    }
                                    if on {
                                        res.recv_batch_datagrams += 1;
                                        res.recv_batch_subjects += subjects.len() as u64;
                                        res.recv_batch_bytes += data.len() as u64;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ClientEvent::StreamData { data } => {
                    if on {
                        res.recv_stream_bytes += data.len() as u64;
                    }
                    reader.append(&data);
                    while let Ok(Some(frame)) = reader.try_read() {
                        if on {
                            res.recv_stream_count += 1;
                        }
                        if is_observer {
                            match ServerMessage::decode(&frame[..]) {
                                Ok(ServerMessage {
                                    message: Some(server_message::Message::PlayerJoined(pj)),
                                }) => {
                                    if let Some(st) = pj.state.as_ref() {
                                        last_known.insert(st.subject_id, st.sequence);
                                    }
                                }
                                Ok(ServerMessage {
                                    message: Some(server_message::Message::PlayerStateFull(f)),
                                }) => {
                                    last_known.insert(f.subject_id, f.sequence);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ClientEvent::Disconnected { reason } => {
                    eprintln!("bot {idx} disconnected (reason {reason})");
                    return res;
                }
            }
            if Instant::now() >= next_send {
                break;
            }
        }

        let now = Instant::now();
        if now >= next_send {
            let t = now.duration_since(start).as_secs_f64();
            let input = ClientMessage {
                message: Some(client_message::Message::Input(PlayerStateInput {
                    state: Some(grounded_run_state(idx, t)),
                })),
            }
            .encode_to_vec();
            if client.send_datagram(&datagram_frame(CHANNEL_SEQUENCED, seq, &input)) {
                seq = seq.wrapping_add(1);
                if counting.load(Ordering::Relaxed) {
                    res.sent_inputs += 1;
                }
            }
            next_send += period;
            if next_send < now {
                next_send = now + period;
            }
        }

        if is_observer && last_rtt.elapsed() >= Duration::from_millis(500) {
            res.rtt_us.push(client.rtt_us());
            last_rtt = Instant::now();
        }
    }
    res
}

fn sample_proc(pid: u32, stop: Arc<AtomicBool>) -> Vec<(Instant, u64, u64)> {
    let stat_path = format!("/proc/{pid}/stat");
    let status_path = format!("/proc/{pid}/status");
    let mut out = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let ticks = std::fs::read_to_string(&stat_path)
            .ok()
            .and_then(|s| parse_cpu_ticks(&s));
        let rss = std::fs::read_to_string(&status_path)
            .ok()
            .and_then(|s| parse_vmrss_kb(&s));
        if let (Some(t), Some(r)) = (ticks, rss) {
            out.push((Instant::now(), t, r));
        }
        thread::sleep(Duration::from_millis(250));
    }
    out
}

fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let rparen = stat.rfind(')')?;
    let rest: Vec<&str> = stat[rparen + 1..].split_whitespace().collect();
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    Some(utime + stime)
}

fn parse_vmrss_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(v) = line.strip_prefix("VmRSS:") {
            return v.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        sorted[lo] + (rank - lo as f64) * (sorted[hi] - sorted[lo])
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_server(
    server_bin: &str,
    cores: &str,
    enet: u16,
    wt: u16,
    cert_path: &str,
    key_path: &str,
    log_path: &str,
) -> anyhow::Result<Child> {
    let log = std::fs::File::create(log_path)?;
    let err = log.try_clone()?;
    let mut cmd = if cores.is_empty() {
        Command::new(server_bin)
    } else {
        let mut c = Command::new("taskset");
        c.arg("-c").arg(cores).arg(server_bin);
        c
    };
    cmd.env("PULSE_BIND", format!("127.0.0.1:{enet}"))
        .env("PULSE_WT_ENABLED", "1")
        .env("PULSE_WT_BIND", format!("127.0.0.1:{wt}"))
        .env("PULSE_WT_CERT_PATH", cert_path)
        .env("PULSE_WT_KEY_PATH", key_path)
        .env("RUST_LOG", "warn")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    Ok(cmd.spawn()?)
}

fn main() -> anyhow::Result<()> {
    let n_bots: usize = env_num("LT_BOTS", 40usize);
    let protocol_features: u32 = env_num("LT_PROTOCOL_FEATURES", 0u32);
    let rtt_ms: u64 = env_num("LT_RTT_MS", 0u64);
    let duration_s: f64 = env_num("LT_DURATION_S", 20.0f64);
    let warmup_s: f64 = env_num("LT_WARMUP_S", 4.0f64);
    let out_path = env_str("LT_OUT", "/tmp/netcode_loadtest.json");
    let server_bin = env_str("LT_SERVER_BIN", "target/release/examples/netcode_server");
    let server_cores = env_str("LT_SERVER_CORES", "");
    let wt_port: u16 = {
        let p = env_num("LT_WT_PORT", 0u16);
        if p == 0 {
            free_udp_port()
        } else {
            p
        }
    };
    let enet_port: u16 = {
        let p = env_num("LT_ENET_PORT", 0u16);
        if p == 0 {
            free_udp_port()
        } else {
            p
        }
    };
    let rtt = Duration::from_millis(rtt_ms);

    let out_dir = std::path::Path::new(&out_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&out_dir)?;
    let cert_path = out_dir.join("dev-cert.pem");
    let key_path = out_dir.join("dev-key.pem");
    let log_path = out_dir.join(format!("server-rtt{rtt_ms}.log"));

    let (cert_pem, key_pem, cert_hash) = dev_cert();
    std::fs::write(&cert_path, &cert_pem)?;
    std::fs::write(&key_path, &key_pem)?;

    eprintln!(
        "[harness] rtt={rtt_ms}ms bots={n_bots} dur={duration_s}s wt=127.0.0.1:{wt_port} enet={enet_port} cores=[{server_cores}]"
    );

    let mut server = spawn_server(
        &server_bin,
        &server_cores,
        enet_port,
        wt_port,
        cert_path.to_str().unwrap(),
        key_path.to_str().unwrap(),
        log_path.to_str().unwrap(),
    )?;
    let server_pid = server.id();

    let sign_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let handshakes: Vec<Vec<u8>> = sign_rt.block_on(async {
        let mut v = Vec::with_capacity(n_bots);
        for _ in 0..n_bots {
            v.push(sign_handshake(now_ms(), protocol_features).await);
        }
        v
    });

    let url = format!("https://127.0.0.1:{wt_port}/");

    let mut first: Option<(Client, StreamFrameReader)> = None;
    let boot_deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < boot_deadline {
        match connect_and_join(&url, &cert_hash, &handshakes[1.min(n_bots - 1)], rtt) {
            Ok(c) => {
                first = Some(c);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(300)),
        }
    }
    let first = first.ok_or_else(|| anyhow::anyhow!("server WT never came up"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let counting = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    {
        let (c, r) = first;
        let (stop, counting) = (stop.clone(), counting.clone());
        handles.push(thread::spawn(move || {
            (1usize, run_bot(c, r, 1, false, stop, counting))
        }));
    }
    for (idx, hs) in handshakes.iter().enumerate().take(n_bots).skip(2) {
        let (c, r) = connect_and_join(&url, &cert_hash, hs, rtt)?;
        let (stop, counting) = (stop.clone(), counting.clone());
        handles.push(thread::spawn(move || {
            (idx, run_bot(c, r, idx, false, stop, counting))
        }));
    }

    thread::sleep(Duration::from_secs_f64(1.5));

    let t0 = Instant::now();
    let (mut obs, mut obs_reader) = connect_and_join(&url, &cert_hash, &handshakes[0], rtt)?;
    let mut first_state_latency_ms = f64::NAN;
    let fs_deadline = Instant::now() + Duration::from_secs(10);
    'outer: while Instant::now() < fs_deadline {
        if let Some(ev) = obs.service(Duration::from_millis(100)) {
            match ev {
                ClientEvent::StreamData { data } => {
                    obs_reader.append(&data);
                    while let Ok(Some(frame)) = obs_reader.try_read() {
                        if let Ok(ServerMessage { message: Some(m) }) =
                            ServerMessage::decode(&frame[..])
                        {
                            if matches!(
                                m,
                                server_message::Message::PlayerJoined(_)
                                    | server_message::Message::PlayerStateFull(_)
                            ) {
                                first_state_latency_ms =
                                    (t0.elapsed() + rtt / 2).as_secs_f64() * 1000.0;
                                break 'outer;
                            }
                        }
                    }
                }
                ClientEvent::Datagram { data } => {
                    if let Ok(ServerMessage {
                        message: Some(server_message::Message::PlayerStateDelta(_)),
                    }) = ServerMessage::decode(&data[..])
                    {
                        first_state_latency_ms = (t0.elapsed() + rtt / 2).as_secs_f64() * 1000.0;
                        break 'outer;
                    }
                }
                ClientEvent::Disconnected { reason } => {
                    anyhow::bail!("observer disconnected (reason {reason})")
                }
            }
        }
    }
    {
        let (stop, counting) = (stop.clone(), counting.clone());
        handles.push(thread::spawn(move || {
            (0usize, run_bot(obs, obs_reader, 0, true, stop, counting))
        }));
    }

    let proc_stop = Arc::new(AtomicBool::new(false));
    let proc_handle = {
        let ps = proc_stop.clone();
        thread::spawn(move || sample_proc(server_pid, ps))
    };

    thread::sleep(Duration::from_secs_f64(warmup_s));
    let window_start = Instant::now();
    counting.store(true, Ordering::Relaxed);
    thread::sleep(Duration::from_secs_f64(duration_s));
    counting.store(false, Ordering::Relaxed);
    let window_end = Instant::now();
    let window_secs = window_end.duration_since(window_start).as_secs_f64();

    stop.store(true, Ordering::Relaxed);
    proc_stop.store(true, Ordering::Relaxed);

    let mut results: Vec<(usize, BotResult)> = Vec::new();
    for h in handles {
        results.push(h.join().map_err(|_| anyhow::anyhow!("bot thread panic"))?);
    }
    let proc_samples = proc_handle
        .join()
        .map_err(|_| anyhow::anyhow!("proc sampler panic"))?;

    let _ = server.kill();
    let _ = server.wait();

    let connected = results.len();
    let cluster_dgram_bytes: u64 = results.iter().map(|(_, r)| r.recv_datagram_bytes).sum();
    let cluster_dgram_pkts: u64 = results.iter().map(|(_, r)| r.recv_datagram_count).sum();
    let cluster_stream_bytes: u64 = results.iter().map(|(_, r)| r.recv_stream_bytes).sum();
    let cluster_sent: u64 = results.iter().map(|(_, r)| r.sent_inputs).sum();

    let obs = results
        .iter()
        .find(|(i, _)| *i == 0)
        .map(|(_, r)| r)
        .ok_or_else(|| anyhow::anyhow!("no observer result"))?;

    let obs_dgram_bytes_s = obs.recv_datagram_bytes as f64 / window_secs;
    let obs_dgram_pkts_s = obs.recv_datagram_count as f64 / window_secs;
    let obs_stream_bytes_s = obs.recv_stream_bytes as f64 / window_secs;

    let mut sizes: Vec<f64> = obs.deltas.iter().map(|(_, l, _)| *l as f64).collect();
    let mut fields: Vec<f64> = obs.deltas.iter().map(|(_, _, f)| *f as f64).collect();
    let mut inter: Vec<f64> = obs
        .deltas
        .windows(2)
        .map(|w| (w[1].0.duration_since(w[0].0)).as_secs_f64() * 1000.0)
        .collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    fields.sort_by(|a, b| a.partial_cmp(b).unwrap());
    inter.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = |v: &[f64]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        }
    };

    let mut rtts: Vec<f64> = obs.rtt_us.iter().map(|u| *u as f64).collect();
    rtts.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let clk_tck = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(100.0);
    let win: Vec<&(Instant, u64, u64)> = proc_samples
        .iter()
        .filter(|(t, _, _)| *t >= window_start && *t <= window_end + Duration::from_millis(300))
        .collect();
    let (cpu_pct, rss_peak_mb, rss_mean_mb) = if win.len() >= 2 {
        let first = win.first().unwrap();
        let last = win.last().unwrap();
        let dt = last.0.duration_since(first.0).as_secs_f64();
        let dticks = last.1.saturating_sub(first.1) as f64;
        let cpu = if dt > 0.0 {
            (dticks / clk_tck) / dt * 100.0
        } else {
            0.0
        };
        let peak = win.iter().map(|(_, _, r)| *r).max().unwrap_or(0) as f64 / 1024.0;
        let meanr = win.iter().map(|(_, _, r)| *r as f64).sum::<f64>() / win.len() as f64 / 1024.0;
        (cpu, peak, meanr)
    } else {
        (f64::NAN, f64::NAN, f64::NAN)
    };

    let json = serde_json::json!({
        "config": {
            "bots": n_bots,
            "connected_peers": connected,
            "injected_rtt_ms": rtt_ms,
            "rtt_model": "application-layer (uplink+downlink rtt/2); NOT kernel netem",
            "duration_s": window_secs,
            "warmup_s": warmup_s,
            "tick_hz": TICK_HZ,
            "realm": REALM,
            "parcel_index": PARCEL_INDEX,
            "protocol_features": protocol_features,
            "transport": "webtransport-quic-loopback",
            "server_cores": server_cores,
            "server_pid": server_pid,
        },
        "per_observer": {
            "datagram_bytes_per_s": obs_dgram_bytes_s,
            "datagram_pkts_per_s": obs_dgram_pkts_s,
            "stream_bytes_per_s": obs_stream_bytes_s,
            "delta_datagrams": obs.deltas.len(),
            "delta_size_bytes": {
                "min": sizes.first().copied().unwrap_or(0.0),
                "p50": percentile(&sizes, 50.0),
                "mean": mean(&sizes),
                "p99": percentile(&sizes, 99.0),
                "max": sizes.last().copied().unwrap_or(0.0),
            },
            "changed_fields": {
                "min": fields.first().copied().unwrap_or(0.0),
                "mean": mean(&fields),
                "max": fields.last().copied().unwrap_or(0.0),
            },
            "inter_arrival_ms": {
                "p50": percentile(&inter, 50.0),
                "mean": mean(&inter),
                "p99": percentile(&inter, 99.0),
            },
            "scheme_c": {
                "batch_datagrams_per_s": obs.recv_batch_datagrams as f64 / window_secs,
                "batch_subjects_per_s": obs.recv_batch_subjects as f64 / window_secs,
                "batch_bytes_per_s": obs.recv_batch_bytes as f64 / window_secs,
                "mean_subjects_per_batch": if obs.recv_batch_datagrams > 0 {
                    obs.recv_batch_subjects as f64 / obs.recv_batch_datagrams as f64
                } else {
                    0.0
                },
                "mean_bytes_per_subject": if obs.recv_batch_subjects > 0 {
                    obs.recv_batch_bytes as f64 / obs.recv_batch_subjects as f64
                } else {
                    0.0
                },
            },
        },
        "cluster_total": {
            "egress_datagram_bytes_per_s": cluster_dgram_bytes as f64 / window_secs,
            "egress_datagram_pkts_per_s": cluster_dgram_pkts as f64 / window_secs,
            "egress_stream_bytes_per_s": cluster_stream_bytes as f64 / window_secs,
            "ingress_input_pkts_per_s": cluster_sent as f64 / window_secs,
        },
        "latency": {
            "handshake_to_first_state_ms": first_state_latency_ms,
            "transport_rtt_us_measured": {
                "p50": percentile(&rtts, 50.0),
                "p99": percentile(&rtts, 99.0),
                "note": "QUIC smoothed RTT over loopback; injected RTT is modeled separately",
            },
            "steady_rtt_ms_modeled": rtt_ms,
        },
        "server": {
            "cpu_percent": cpu_pct,
            "rss_peak_mb": rss_peak_mb,
            "rss_mean_mb": rss_mean_mb,
            "clk_tck": clk_tck,
        },
    });

    let mut f = std::fs::File::create(&out_path)?;
    writeln!(f, "{}", serde_json::to_string_pretty(&json)?)?;
    eprintln!(
        "[harness] rtt={rtt_ms}ms done: obs {obs_dgram_pkts_s:.0} pkt/s {obs_dgram_bytes_s:.0} B/s; \
         delta p50 {:.0}B; cpu {cpu_pct:.1}% rss {rss_peak_mb:.0}MB -> {out_path}",
        percentile(&sizes, 50.0)
    );
    Ok(())
}
