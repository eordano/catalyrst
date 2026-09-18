use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub up: usize,
    pub down: usize,
    pub packets: usize,
}
impl std::ops::Sub for Counts {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            up: self.up - rhs.up,
            down: self.down - rhs.down,
            packets: self.packets - rhs.packets,
        }
    }
}
pub struct Relay {
    pub addr: SocketAddr,
    counts: Arc<Mutex<Counts>>,
    task: tokio::task::JoinHandle<()>,
}
impl Relay {
    pub async fn start(server: SocketAddr, rtt_ms: u64) -> Self {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let counts = Arc::new(Mutex::new(Counts::default()));
        let counter = counts.clone();
        let task = tokio::spawn(async move {
            let mut client = None;
            let mut buffer = [0; 65536];
            let mut queue = std::collections::VecDeque::new();
            loop {
                let next = queue
                    .front()
                    .map(|(time, _, _)| *time)
                    .unwrap_or_else(|| Instant::now() + Duration::from_secs(60));
                tokio::select! {
                    packet = socket.recv_from(&mut buffer) => {
                        let (len, from) = packet.unwrap();
                        let target = if from == server {
                            counter.lock().unwrap().down += len+28;
                            client.unwrap()
                        } else {
                            counter.lock().unwrap().up += len+28;
                            client = Some(from);
                            server
                        };
                        counter.lock().unwrap().packets += 1;
                        if rtt_ms == 0 { socket.send_to(&buffer[..len],target).await.unwrap(); }
                        else { queue.push_back((Instant::now()+Duration::from_millis(rtt_ms/2), target, buffer[..len].to_vec())); }
                    }
                    _ = tokio::time::sleep_until(next.into()), if !queue.is_empty() => {
                        let (_, target, payload) = queue.pop_front().unwrap();
                        socket.send_to(&payload, target).await.unwrap();
                    }
                }
            }
        });
        Self { addr, counts, task }
    }
    pub fn counts(&self) -> Counts {
        *self.counts.lock().unwrap()
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
