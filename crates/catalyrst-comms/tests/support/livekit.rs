use futures::{SinkExt, StreamExt};
use prost::Message as _;
use std::{
    net::{TcpListener, UdpSocket},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

pub type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
pub const KEY: &str = "isolated-comms-test";

pub struct Server {
    child: Child,
    pub url: String,
    pub secret: String,
}

impl Server {
    pub async fn start(binary: &str) -> Self {
        let signal = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let udp = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = signal.local_addr().unwrap().port();
        let secret = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let config = format!(
            "port: {port}\nbind_addresses: [127.0.0.1]\nrtc:\n  tcp_port: 0\n  udp_port: {}\n  node_ip: 127.0.0.1\n  use_external_ip: false\n  enable_loopback_candidate: true\n  ips:\n    includes: [127.0.0.1/32]\n  stun_servers: []\nkeys:\n  {KEY}: {secret}\nlogging:\n  level: error\n",
            udp.local_addr().unwrap().port()
        );
        drop((signal, udp));
        let child = Command::new(binary)
            .env_clear()
            .env("LIVEKIT_CONFIG", config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start owned LiveKit fixture");
        let mut server = Self {
            child,
            url: format!("http://127.0.0.1:{port}"),
            secret,
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                assert!(
                    server.child.try_wait().unwrap().is_none(),
                    "LiveKit fixture exited before readiness"
                );
                if http
                    .get(&server.url)
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned LiveKit fixture readiness deadline");
        #[cfg(target_os = "linux")]
        server.assert_loopback_sockets();
        server
    }

    #[cfg(target_os = "linux")]
    fn assert_loopback_sockets(&self) {
        let root = format!("/proc/{}", self.child.id());
        let inodes: std::collections::HashSet<String> = std::fs::read_dir(format!("{root}/fd"))
            .unwrap()
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .filter_map(|path| {
                path.to_str()?
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')
                    .map(str::to_string)
            })
            .collect();
        let mut sockets = 0;
        for protocol in ["tcp", "tcp6", "udp", "udp6"] {
            let table = std::fs::read_to_string(format!("{root}/net/{protocol}")).unwrap();
            for row in table.lines().skip(1) {
                let columns: Vec<_> = row.split_whitespace().collect();
                if columns.len() <= 9 || !inodes.contains(columns[9]) {
                    continue;
                }
                let address = columns[1].split(':').next().unwrap();
                assert!(
                    address == "0100007F" || address == "00000000000000000000000001000000",
                    "owned LiveKit fixture bound a non-loopback {protocol} socket"
                );
                sockets += 1;
            }
        }
        assert!(sockets >= 2, "expected owned signalling and UDP sockets");
    }

    pub fn ws_url(&self) -> String {
        self.url.replacen("http://", "ws://", 1)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Minimal read-only projection of SignalResponse.join/leave and JoinResponse.participant.
// Unknown protobuf fields are discarded; no ICE or media success is inferred from this signal.
#[derive(prost::Message)]
pub struct Signal {
    #[prost(message, optional, tag = "1")]
    pub join: Option<Join>,
    #[prost(message, optional, tag = "8")]
    pub leave: Option<Leave>,
}

#[derive(prost::Message)]
pub struct Join {
    #[prost(message, optional, tag = "2")]
    pub participant: Option<Participant>,
}

#[derive(prost::Message)]
pub struct Participant {
    #[prost(string, tag = "1")]
    pub sid: String,
    #[prost(string, tag = "2")]
    pub identity: String,
    #[prost(string, tag = "5")]
    pub metadata: String,
}

#[derive(prost::Message)]
pub struct Leave {
    #[prost(uint32, tag = "2")]
    pub reason: u32,
}

pub async fn try_join(server: &Server, adapter: &str) -> Result<(Socket, Participant), String> {
    let token = adapter
        .split_once("access_token=")
        .ok_or_else(|| "fixture adapter has no token".to_string())?
        .1;
    let mut request = format!(
        "{}/rtc?protocol=16&auto_subscribe=0&sdk=rust&version=test",
        server.ws_url()
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "signalling admission deadline".to_string())?
    .map_err(|error| format!("signalling admission failed: {error}"))?;
    let participant = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = socket
                .next()
                .await
                .ok_or_else(|| "signal stream ended".to_string())?
                .map_err(|error| format!("signal frame failed: {error}"))?;
            match frame {
                Message::Binary(bytes) => {
                    if let Some(join) = Signal::decode(bytes)
                        .map_err(|error| format!("invalid signal frame: {error}"))?
                        .join
                    {
                        break join
                            .participant
                            .ok_or_else(|| "join has no participant".to_string());
                    }
                }
                Message::Ping(bytes) => socket
                    .send(Message::Pong(bytes))
                    .await
                    .map_err(|error| format!("cannot answer signal ping: {error}"))?,
                Message::Close(_) => return Err("closed before join".to_string()),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "join response deadline".to_string())??;
    assert!(!participant.sid.is_empty());
    Ok((socket, participant))
}

pub async fn join(server: &Server, adapter: &str) -> (Socket, Participant) {
    try_join(server, adapter)
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

pub async fn leave_reason(socket: &mut Socket) -> u32 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket
                .next()
                .await
                .expect("signal stream")
                .expect("signal frame")
            {
                Message::Binary(bytes) => {
                    if let Some(leave) = Signal::decode(bytes).unwrap().leave {
                        return leave.reason;
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                Message::Close(_) => panic!("closed without leave reason"),
                _ => {}
            }
        }
    })
    .await
    .expect("leave response deadline")
}

pub async fn metadata_update(
    socket: &mut Socket,
    metadata: &str,
    request_id: u32,
) -> serde_json::Value {
    socket
        .send(Message::Text(
            serde_json::json!({"updateMetadata": {
                "metadata": metadata, "requestId": request_id
            }})
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket
                .next()
                .await
                .expect("signal stream")
                .expect("signal frame")
            {
                Message::Text(text) => {
                    let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if response["requestResponse"]["requestId"] == request_id {
                        return response["requestResponse"].clone();
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                Message::Close(_) => panic!("closed before metadata acknowledgement"),
                _ => {}
            }
        }
    })
    .await
    .expect("metadata response deadline")
}
