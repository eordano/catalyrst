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
use catalyrst_pulse::server::FEATURE_DELTA_BATCH;
use catalyrst_pulse::transport::webtransport::framing::{
    datagram_frame, stream_frame, StreamFrameReader,
};
use web_transport::client::{Client, ClientConfig, ClientEvent};

const REALM: &str = "netcode-matrix";
const PARCEL_INDEX: i32 = 5;
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

struct Rng(u64);
impl Rng {
    fn next_unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
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

fn grounded_run_state(idx: usize, n: usize, t: f64) -> PlayerState {
    let phase = idx as f64 * (std::f64::consts::TAU / n.max(1) as f64);
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
struct DriftStats {
    samples: u64,
    dropped: u64,
    delivered: u64,
    max_abs: i64,
    sum_abs: i64,
    final_err: i64,
    max_between_heals: i64,
    seg_max: i64,
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
    drift: DriftStats,
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

struct BotCfg {
    idx: usize,
    n: usize,
    tick_hz: f64,
    is_observer: bool,
    drift: bool,
    loss_p: f64,
    heal: Duration,
    seed: u64,
}

fn run_bot(
    mut client: Client,
    mut reader: StreamFrameReader,
    cfg: BotCfg,
    stop: Arc<AtomicBool>,
    counting: Arc<AtomicBool>,
) -> BotResult {
    let mut res = BotResult::default();
    let start = Instant::now();
    let period = Duration::from_secs_f64(1.0 / cfg.tick_hz);
    let mut next_send = Instant::now();
    let mut seq: u32 = 1;
    let mut last_rtt = Instant::now();
    let mut known_truth: HashMap<u32, u32> = HashMap::new();
    let mut known_lossy: HashMap<u32, u32> = HashMap::new();
    let mut rng = Rng(cfg.seed);
    let mut last_heal = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        while let Some(ev) = client.service(Duration::from_millis(1)) {
            let on = counting.load(Ordering::Relaxed);
            match ev {
                ClientEvent::Datagram { data } => {
                    if on {
                        res.recv_datagram_bytes += data.len() as u64;
                        res.recv_datagram_count += 1;
                    }
                    if cfg.is_observer {
                        match ServerMessage::decode(&data[..]) {
                            Ok(ServerMessage {
                                message: Some(server_message::Message::PlayerStateDelta(d)),
                            }) => {
                                known_truth.insert(d.subject_id, d.new_seq);
                                known_lossy.insert(d.subject_id, d.new_seq);
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
                                let truth = decode_batch(b.subject_count, &b.payload, |id| {
                                    known_truth.get(&id).copied().unwrap_or(0)
                                });
                                if let Ok(truth) = truth {
                                    let n = truth.len().max(1);
                                    let per_subject = data.len() / n;
                                    let arrival = Instant::now();
                                    for s in &truth {
                                        known_truth.insert(s.subject_id, s.new_seq);
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
                                        res.recv_batch_subjects += truth.len() as u64;
                                        res.recv_batch_bytes += data.len() as u64;
                                    }
                                    if cfg.drift {
                                        drift_step(
                                            &mut res,
                                            &mut known_lossy,
                                            &known_truth,
                                            &b.payload,
                                            b.subject_count,
                                            &mut rng,
                                            cfg.loss_p,
                                            on,
                                        );
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
                        if cfg.is_observer {
                            match ServerMessage::decode(&frame[..]) {
                                Ok(ServerMessage {
                                    message: Some(server_message::Message::PlayerJoined(pj)),
                                }) => {
                                    if let Some(st) = pj.state.as_ref() {
                                        known_truth.insert(st.subject_id, st.sequence);
                                        known_lossy.insert(st.subject_id, st.sequence);
                                    }
                                }
                                Ok(ServerMessage {
                                    message: Some(server_message::Message::PlayerStateFull(f)),
                                }) => {
                                    known_truth.insert(f.subject_id, f.sequence);
                                    known_lossy.insert(f.subject_id, f.sequence);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ClientEvent::Disconnected { reason } => {
                    eprintln!("bot {} disconnected (reason {reason})", cfg.idx);
                    return res;
                }
            }
            if Instant::now() >= next_send {
                break;
            }
        }

        if cfg.drift && cfg.heal > Duration::ZERO && last_heal.elapsed() >= cfg.heal {
            res.drift.max_between_heals = res.drift.max_between_heals.max(res.drift.seg_max);
            res.drift.seg_max = 0;
            known_lossy = known_truth.clone();
            last_heal = Instant::now();
        }

        let now = Instant::now();
        if now >= next_send {
            let t = now.duration_since(start).as_secs_f64();
            let input = ClientMessage {
                message: Some(client_message::Message::Input(PlayerStateInput {
                    state: Some(grounded_run_state(cfg.idx, cfg.n, t)),
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

        if cfg.is_observer && last_rtt.elapsed() >= Duration::from_millis(200) {
            res.rtt_us.push(client.rtt_us());
            last_rtt = Instant::now();
        }
    }
    res.drift.max_between_heals = res.drift.max_between_heals.max(res.drift.seg_max);
    res
}

#[allow(clippy::too_many_arguments)]
fn drift_step(
    res: &mut BotResult,
    known_lossy: &mut HashMap<u32, u32>,
    known_truth: &HashMap<u32, u32>,
    payload: &[u8],
    subject_count: u32,
    rng: &mut Rng,
    loss_p: f64,
    on: bool,
) {
    let drop = rng.next_unit() < loss_p;
    if drop {
        if on {
            res.drift.dropped += 1;
        }
        return;
    }
    let lossy = match decode_batch(subject_count, payload, |id| {
        known_lossy.get(&id).copied().unwrap_or(0)
    }) {
        Ok(v) => v,
        Err(_) => return,
    };
    if on {
        res.drift.delivered += 1;
    }
    for s in &lossy {
        let truth = known_truth.get(&s.subject_id).copied().unwrap_or(s.new_seq);
        let err = truth as i64 - s.new_seq as i64;
        known_lossy.insert(s.subject_id, s.new_seq);
        if on {
            res.drift.samples += 1;
            res.drift.sum_abs += err.abs();
            res.drift.max_abs = res.drift.max_abs.max(err.abs());
            res.drift.seg_max = res.drift.seg_max.max(err.abs());
            res.drift.final_err = err;
        }
    }
}

fn sample_proc(
    pid: u32,
    alloc_path: String,
    stop: Arc<AtomicBool>,
) -> Vec<(Instant, u64, u64, u64)> {
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
        let allocs = std::fs::read_to_string(&alloc_path)
            .ok()
            .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
            .unwrap_or(0);
        if let (Some(t), Some(r)) = (ticks, rss) {
            out.push((Instant::now(), t, r, allocs));
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
    tick_ms: u64,
    seq_encoding: &str,
    alloc_path: &str,
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
        .env("PULSE_TICK_MS", tick_ms.to_string())
        .env("PULSE_SEQ_ENCODING", seq_encoding)
        .env("PULSE_ALLOC_STAT_PATH", alloc_path)
        .env("RUST_LOG", "warn")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    Ok(cmd.spawn()?)
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn main() -> anyhow::Result<()> {
    let n_bots: usize = env_num("MX_PEERS", 40usize);
    let tick_hz: f64 = env_num("MX_TICK_HZ", 20.0f64);
    let config = env_str("MX_CONFIG", "v1-abs");
    let loss_pct: f64 = env_num("MX_LOSS_PCT", 0.0f64);
    let drift: bool = env_num::<u8>("MX_DRIFT", 0) != 0;
    let heal_s: f64 = env_num("MX_HEAL_S", 5.0f64);
    let seed: u64 = env_num("MX_SEED", 0x1234_5678_9abc_def0u64);
    let rtt_ms: u64 = env_num("MX_RTT_MS", 0u64);
    let duration_s: f64 = env_num("MX_DURATION_S", 10.0f64);
    let warmup_s: f64 = env_num("MX_WARMUP_S", 3.0f64);
    let out_path = env_str("MX_OUT", "/tmp/matrix_cell.json");
    let server_bin = env_str("MX_SERVER_BIN", "target/release/examples/matrix_server");
    let server_cores = env_str("MX_SERVER_CORES", "");

    let (protocol_features, seq_encoding) = match config.as_str() {
        "v0" => (0u32, "absolute"),
        "v1-abs" => (FEATURE_DELTA_BATCH, "absolute"),
        "v1-delta" => (FEATURE_DELTA_BATCH, "delta"),
        other => anyhow::bail!("unknown MX_CONFIG {other} (want v0|v1-abs|v1-delta)"),
    };
    let tick_ms = (1000.0 / tick_hz).round().max(1.0) as u64;
    let loss_p = loss_pct / 100.0;
    let rtt = Duration::from_millis(rtt_ms);

    let wt_port = {
        let p = env_num("MX_WT_PORT", 0u16);
        if p == 0 {
            free_udp_port()
        } else {
            p
        }
    };
    let enet_port = {
        let p = env_num("MX_ENET_PORT", 0u16);
        if p == 0 {
            free_udp_port()
        } else {
            p
        }
    };

    let out_dir = std::path::Path::new(&out_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&out_dir)?;
    let cert_path = out_dir.join("dev-cert.pem");
    let key_path = out_dir.join("dev-key.pem");
    let tag = format!("{config}-p{n_bots}-hz{tick_hz:.0}-loss{loss_pct:.0}");
    let log_path = out_dir.join(format!("server-{tag}.log"));
    let alloc_path = out_dir.join(format!("alloc-{tag}.stat"));
    let _ = std::fs::remove_file(&alloc_path);

    let (cert_pem, key_pem, cert_hash) = dev_cert();
    std::fs::write(&cert_path, &cert_pem)?;
    std::fs::write(&key_path, &key_pem)?;

    eprintln!(
        "[cell] config={config} peers={n_bots} hz={tick_hz:.0} (tick={tick_ms}ms) loss={loss_pct:.0}% \
         drift={drift} wt=127.0.0.1:{wt_port} cores=[{server_cores}]"
    );

    let mut server = spawn_server(
        &server_bin,
        &server_cores,
        enet_port,
        wt_port,
        cert_path.to_str().unwrap(),
        key_path.to_str().unwrap(),
        log_path.to_str().unwrap(),
        tick_ms,
        seq_encoding,
        alloc_path.to_str().unwrap(),
    )?;
    let server_pid = server.id();

    let sign_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
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
    let first = match first {
        Some(c) => c,
        None => {
            let _ = server.kill();
            let _ = server.wait();
            write_failed(
                &out_path,
                &config,
                n_bots,
                tick_hz,
                loss_pct,
                "server WT never came up",
            )?;
            eprintln!("[cell] FAILED: server WT never came up");
            return Ok(());
        }
    };

    let stop = Arc::new(AtomicBool::new(false));
    let counting = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    let mk = |idx: usize, is_observer: bool| BotCfg {
        idx,
        n: n_bots,
        tick_hz,
        is_observer,
        drift,
        loss_p,
        heal: Duration::from_secs_f64(heal_s),
        seed,
    };

    {
        let (c, r) = first;
        let (stop, counting) = (stop.clone(), counting.clone());
        let cfg = mk(1, false);
        handles.push(thread::spawn(move || {
            (1usize, run_bot(c, r, cfg, stop, counting))
        }));
    }
    let mut connect_failures = 0usize;
    for (idx, hs) in handshakes.iter().enumerate().take(n_bots).skip(2) {
        match connect_and_join(&url, &cert_hash, hs, rtt) {
            Ok((c, r)) => {
                let (stop, counting) = (stop.clone(), counting.clone());
                let cfg = mk(idx, false);
                handles.push(thread::spawn(move || {
                    (idx, run_bot(c, r, cfg, stop, counting))
                }));
            }
            Err(e) => {
                connect_failures += 1;
                eprintln!("[cell] bot {idx} connect failed: {e}");
            }
        }
    }

    thread::sleep(Duration::from_secs_f64(1.5));

    let t0 = Instant::now();
    let (mut obs, mut obs_reader) = match connect_and_join(&url, &cert_hash, &handshakes[0], rtt) {
        Ok(c) => c,
        Err(e) => {
            stop.store(true, Ordering::Relaxed);
            for h in handles {
                let _ = h.join();
            }
            let _ = server.kill();
            let _ = server.wait();
            write_failed(
                &out_path,
                &config,
                n_bots,
                tick_hz,
                loss_pct,
                &format!("observer connect: {e}"),
            )?;
            eprintln!("[cell] FAILED: observer connect: {e}");
            return Ok(());
        }
    };
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
                    if let Ok(ServerMessage { message: Some(_) }) = ServerMessage::decode(&data[..])
                    {
                        first_state_latency_ms = (t0.elapsed() + rtt / 2).as_secs_f64() * 1000.0;
                        break 'outer;
                    }
                }
                ClientEvent::Disconnected { reason } => {
                    eprintln!("[cell] observer disconnected during first-state (reason {reason})");
                    break 'outer;
                }
            }
        }
    }
    {
        let (stop, counting) = (stop.clone(), counting.clone());
        let cfg = mk(0, true);
        handles.push(thread::spawn(move || {
            (0usize, run_bot(obs, obs_reader, cfg, stop, counting))
        }));
    }

    let proc_stop = Arc::new(AtomicBool::new(false));
    let proc_handle = {
        let ps = proc_stop.clone();
        let ap = alloc_path.to_string_lossy().to_string();
        thread::spawn(move || sample_proc(server_pid, ap, ps))
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

    let mut rtts: Vec<f64> = obs.rtt_us.iter().map(|u| *u as f64).collect();
    rtts.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let clk_tck = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(100.0);
    let win: Vec<&(Instant, u64, u64, u64)> = proc_samples
        .iter()
        .filter(|(t, _, _, _)| *t >= window_start && *t <= window_end + Duration::from_millis(300))
        .collect();
    let (cpu_pct, rss_peak_mb, rss_mean_mb, alloc_per_s) = if win.len() >= 2 {
        let first = win.first().unwrap();
        let last = win.last().unwrap();
        let dt = last.0.duration_since(first.0).as_secs_f64();
        let dticks = last.1.saturating_sub(first.1) as f64;
        let cpu = if dt > 0.0 {
            (dticks / clk_tck) / dt * 100.0
        } else {
            0.0
        };
        let peak = win.iter().map(|(_, _, r, _)| *r).max().unwrap_or(0) as f64 / 1024.0;
        let meanr =
            win.iter().map(|(_, _, r, _)| *r as f64).sum::<f64>() / win.len() as f64 / 1024.0;
        let dalloc = last.3.saturating_sub(first.3) as f64;
        let aps = if dt > 0.0 { dalloc / dt } else { f64::NAN };
        (cpu, peak, meanr, aps)
    } else {
        (f64::NAN, f64::NAN, f64::NAN, f64::NAN)
    };

    let expected_dgram_s = tick_hz;
    let capped = connected < n_bots
        || (protocol_features != 0
            && obs.recv_batch_datagrams > 0
            && (obs.recv_batch_datagrams as f64 / window_secs) < 0.5 * expected_dgram_s);
    let status = if capped { "capped" } else { "ok" };

    let json = serde_json::json!({
        "config": {
            "feature_config": config,
            "peers_requested": n_bots,
            "connected_peers": connected,
            "connect_failures": connect_failures,
            "protocol_features": protocol_features,
            "server_seq_encoding": seq_encoding,
            "tick_hz": tick_hz,
            "tick_ms": tick_ms,
            "loss_pct": loss_pct,
            "drift_enabled": drift,
            "duration_s": window_secs,
            "warmup_s": warmup_s,
            "realm": REALM,
            "parcel_index": PARCEL_INDEX,
            "transport": "webtransport-quic-loopback",
            "server_cores": server_cores,
            "server_pid": server_pid,
            "status": status,
        },
        "bandwidth": {
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
                "scheme_c": {
                    "batch_datagrams_per_s": obs.recv_batch_datagrams as f64 / window_secs,
                    "batch_subjects_per_s": obs.recv_batch_subjects as f64 / window_secs,
                    "batch_bytes_per_s": obs.recv_batch_bytes as f64 / window_secs,
                    "mean_subjects_per_batch": if obs.recv_batch_datagrams > 0 {
                        obs.recv_batch_subjects as f64 / obs.recv_batch_datagrams as f64
                    } else { 0.0 },
                    "mean_bytes_per_subject": if obs.recv_batch_subjects > 0 {
                        obs.recv_batch_bytes as f64 / obs.recv_batch_subjects as f64
                    } else { 0.0 },
                },
            },
            "cluster_total": {
                "egress_datagram_bytes_per_s": cluster_dgram_bytes as f64 / window_secs,
                "egress_datagram_pkts_per_s": cluster_dgram_pkts as f64 / window_secs,
                "egress_stream_bytes_per_s": cluster_stream_bytes as f64 / window_secs,
                "ingress_input_pkts_per_s": cluster_sent as f64 / window_secs,
            },
        },
        "latency": {
            "handshake_to_first_state_ms": first_state_latency_ms,
            "inter_arrival_ms": {
                "p50": percentile(&inter, 50.0),
                "mean": mean(&inter),
                "p99": percentile(&inter, 99.0),
            },
            "transport_rtt_us": {
                "p50": percentile(&rtts, 50.0),
                "p99": percentile(&rtts, 99.0),
                "samples": rtts.len(),
                "note": "QUIC smoothed RTT over loopback; injected RTT modeled separately",
            },
            "steady_rtt_ms_modeled": rtt_ms,
        },
        "server_cost": {
            "cpu_percent": cpu_pct,
            "rss_peak_mb": rss_peak_mb,
            "rss_mean_mb": rss_mean_mb,
            "alloc_per_s": alloc_per_s,
            "clk_tck": clk_tck,
        },
        "drift": if drift {
            serde_json::json!({
                "loss_pct": loss_pct,
                "samples": obs.drift.samples,
                "batches_delivered": obs.drift.delivered,
                "batches_dropped": obs.drift.dropped,
                "max_abs_seq_err": obs.drift.max_abs,
                "mean_abs_seq_err": if obs.drift.samples > 0 {
                    obs.drift.sum_abs as f64 / obs.drift.samples as f64
                } else { 0.0 },
                "final_seq_err": obs.drift.final_err,
                "max_seq_err_between_heals": obs.drift.max_between_heals,
                "heal_period_s": heal_s,
            })
        } else {
            serde_json::Value::Null
        },
    });

    let mut f = std::fs::File::create(&out_path)?;
    writeln!(f, "{}", serde_json::to_string_pretty(&json)?)?;
    eprintln!(
        "[cell] {tag} [{status}]: obs {obs_dgram_pkts_s:.0} pkt/s {obs_dgram_bytes_s:.0} B/s; \
         cpu {cpu_pct:.1}% rss {rss_peak_mb:.0}MB alloc {alloc_per_s:.0}/s; \
         drift max {} -> {out_path}",
        obs.drift.max_abs
    );
    Ok(())
}

fn write_failed(
    out_path: &str,
    config: &str,
    peers: usize,
    hz: f64,
    loss: f64,
    reason: &str,
) -> anyhow::Result<()> {
    let json = serde_json::json!({
        "config": {
            "feature_config": config,
            "peers_requested": peers,
            "tick_hz": hz,
            "loss_pct": loss,
            "status": "failed",
            "failure_reason": reason,
        }
    });
    let mut f = std::fs::File::create(out_path)?;
    writeln!(f, "{}", serde_json::to_string_pretty(&json)?)?;
    Ok(())
}
