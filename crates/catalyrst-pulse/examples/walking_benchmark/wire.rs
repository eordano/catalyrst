use catalyrst_pulse::transport::webtransport::framing::{
    datagram_frame, stream_frame, StreamFrameReader,
};
use rusty_enet as enet;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, oneshot},
};
use web_transport::client::{Client, ClientConfig, ClientEvent};

struct ReadySocket(UdpSocket);
impl enet::Socket for ReadySocket {
    type Address = SocketAddr;
    type Error = std::io::Error;
    fn init(&mut self, _: enet::SocketOptions) -> std::io::Result<()> {
        self.0.set_broadcast(true)
    }
    fn send(&mut self, addr: SocketAddr, bytes: &[u8]) -> std::io::Result<usize> {
        match self.0.try_send_to(bytes, addr) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            r => r,
        }
    }
    fn receive(
        &mut self,
        bytes: &mut [u8; enet::MTU_MAX],
    ) -> std::io::Result<Option<(SocketAddr, enet::PacketReceived)>> {
        match self.0.try_recv_from(bytes) {
            Ok((len, addr)) => Ok(Some((addr, enet::PacketReceived::Complete(len)))),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
pub struct Wire {
    pub input: mpsc::UnboundedReceiver<(Instant, Vec<u8>)>,
    output: mpsc::UnboundedSender<(bool, Vec<u8>)>,
    task: tokio::task::JoinHandle<()>,
    closing: Arc<AtomicBool>,
}
impl Wire {
    pub async fn connect(addr: SocketAddr, certificate: Option<Vec<u8>>) -> Self {
        let (output, mut outgoing) = mpsc::unbounded_channel::<(bool, Vec<u8>)>();
        let (incoming, input) = mpsc::unbounded_channel();
        let (ready, opened) = oneshot::channel();
        let closing = Arc::new(AtomicBool::new(false));
        let stopped = closing.clone();
        let task = if let Some(hash) = certificate {
            tokio::task::spawn_blocking(move || {
                let mut client = Client::connect(ClientConfig {
                    url: format!("https://{addr}/"),
                    server_cert_hash: Some(hash),
                })
                .unwrap();
                ready.send(()).unwrap();
                let mut sequence = 0;
                let mut reader = StreamFrameReader::new(65536);
                loop {
                    loop {
                        match outgoing.try_recv() {
                            Ok((reliable, bytes)) => {
                                if reliable {
                                    assert!(client.send_stream(&stream_frame(&bytes)));
                                } else {
                                    assert!(
                                        client.send_datagram(&datagram_frame(1, sequence, &bytes))
                                    );
                                    sequence += 1;
                                }
                            }
                            Err(mpsc::error::TryRecvError::Empty) => break,
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                client.disconnect(0);
                                return;
                            }
                        }
                    }
                    if stopped.load(Ordering::Relaxed) {
                        client.disconnect(0);
                        return;
                    }
                    match client.service(Duration::from_millis(1)) {
                        Some(ClientEvent::Datagram { data }) => {
                            if incoming.send((Instant::now(), data.to_vec())).is_err() {
                                return;
                            }
                        }
                        Some(ClientEvent::StreamData { data }) => {
                            reader.append(&data);
                            while let Some(frame) = reader.try_read().unwrap() {
                                if incoming.send((Instant::now(), frame.to_vec())).is_err() {
                                    return;
                                }
                            }
                        }
                        Some(ClientEvent::Disconnected { reason }) => {
                            assert!(
                                stopped.load(Ordering::Relaxed),
                                "client disconnected {reason}"
                            );
                            return;
                        }
                        None => {}
                    }
                }
            })
        } else {
            tokio::spawn(async move {
                let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
                socket.writable().await.unwrap();
                let mut host = enet::Host::new(
                    ReadySocket(socket),
                    enet::HostSettings {
                        peer_limit: 1,
                        channel_limit: 3,
                        ..Default::default()
                    },
                )
                .unwrap();
                let peer = host.connect(addr, 3, 0).unwrap().id();
                let mut ready = Some(ready);
                loop {
                    while let Some(event) = host.service().unwrap() {
                        match event {
                            enet::Event::Connect { .. } => {
                                ready.take().unwrap().send(()).unwrap();
                            }
                            enet::Event::Receive { packet, .. } => {
                                if incoming
                                    .send((Instant::now(), packet.data().to_vec()))
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            enet::Event::Disconnect { data, .. } => {
                                panic!("ENet disconnected: {data}")
                            }
                        }
                    }
                    tokio::select! {
                        outgoing = outgoing.recv() => {
                            let Some((reliable, bytes)) = outgoing else { host.peer_mut(peer).disconnect_now(0); host.flush(); return; };
                            let kind = if reliable { enet::PacketKind::Reliable } else { enet::PacketKind::Unreliable { sequenced: true } };
                            host.peer_mut(peer).send(if reliable {0} else {1}, &enet::Packet::new(bytes.as_slice(), kind)).unwrap();
                            host.flush();
                        }
                        r = host.socket().0.readable() => { r.unwrap(); }
                        _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                    }
                }
            })
        };
        tokio::time::timeout(Duration::from_secs(10), opened)
            .await
            .unwrap()
            .unwrap();
        Self {
            input,
            output,
            task,
            closing,
        }
    }
    pub fn send(&self, reliable: bool, bytes: Vec<u8>) {
        self.output.send((reliable, bytes)).unwrap();
    }
    pub async fn receive(&mut self) -> (Instant, Vec<u8>) {
        tokio::time::timeout(Duration::from_secs(10), self.input.recv())
            .await
            .unwrap()
            .expect("transport open")
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Relaxed);
        self.task.abort();
    }
}
