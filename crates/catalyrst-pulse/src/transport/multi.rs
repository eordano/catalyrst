use crate::transport::host::{Event, Host};
use crate::transport::silence::RelaySilence;
use crate::transport::webtransport::WtHost;
use crate::transport::Packet;

use std::time::Instant;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct Transports {
    enet: Host,
    wt: Option<WtHost>,
    wt_events: Option<UnboundedReceiver<Event>>,
    enet_capacity: u32,
    silence: RelaySilence,
}

impl Transports {
    pub fn enet_only(enet: Host, enet_capacity: u32) -> Self {
        Self {
            enet,
            wt: None,
            wt_events: None,
            enet_capacity,
            silence: RelaySilence::default(),
        }
    }

    pub fn with_webtransport(
        enet: Host,
        enet_capacity: u32,
        wt: WtHost,
        wt_events: UnboundedReceiver<Event>,
    ) -> Self {
        Self {
            enet,
            wt: Some(wt),
            wt_events: Some(wt_events),
            enet_capacity,
            silence: RelaySilence::default(),
        }
    }

    fn owns_wt(&self, peer: u32) -> bool {
        self.wt.is_some() && peer >= self.enet_capacity
    }

    pub async fn service(&mut self) -> std::io::Result<Option<Event>> {
        let Self {
            enet, wt_events, ..
        } = self;
        let event = match wt_events.as_mut() {
            Some(rx) => {
                tokio::select! {
                    enet_event = enet.service() => enet_event,
                    wt_event = rx.recv() => Ok(wt_event),
                }
            }
            None => enet.service().await,
        }?;
        match &event {
            Some(Event::Receive { peer, .. }) => self.silence.heard(*peer as u32, Instant::now()),
            Some(Event::Disconnect { peer }) => self.silence.forget(*peer as u32),
            _ => {}
        }
        Ok(event)
    }

    pub fn peer_ip(&mut self, peer: u32) -> Option<String> {
        if self.owns_wt(peer) {
            None
        } else {
            self.enet.peer_ip(peer as u16)
        }
    }

    pub async fn send(&mut self, peer: u32, packet: Packet) -> std::io::Result<()> {
        if self.owns_wt(peer) {
            if let Some(wt) = &self.wt {
                wt.send(peer, packet);
            }
            Ok(())
        } else {
            self.enet.send(peer as u16, packet).await
        }
    }

    pub async fn send_application(&mut self, peer: u32, packet: Packet) -> std::io::Result<()> {
        self.queue_application(peer, packet, false).await
    }

    pub async fn send_control(&mut self, peer: u32, packet: Packet) -> std::io::Result<()> {
        self.queue_application(peer, packet, true).await
    }

    async fn queue_application(
        &mut self,
        peer: u32,
        packet: Packet,
        control: bool,
    ) -> std::io::Result<()> {
        if self.owns_wt(peer) {
            self.wt
                .as_ref()
                .expect("owns WebTransport peer")
                .queue_application(peer, packet, control)
        } else if peer < self.enet_capacity {
            self.enet
                .queue_application(peer as u16, packet, control)
                .await
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "application peer unavailable",
            ))
        }
    }

    pub fn set_relay_timeout(&mut self, peer: u32, relay: bool, sends_input: bool) {
        if peer < self.enet_capacity {
            self.enet.set_relay_timeout(peer as u16, relay);
        } else if self.owns_wt(peer) {
            self.silence
                .watch(peer, relay && sends_input, Instant::now());
        }
    }

    pub fn silent_relay_peers(&self, now: Instant) -> Vec<u32> {
        self.silence.silent(now)
    }

    #[cfg(test)]
    pub(crate) fn enet_timeout(&mut self, peer: u32) -> Option<(u32, u32)> {
        self.enet.peer_timeout(peer as u16)
    }

    /// WebTransport sends dispatch immediately over their mpsc channel; only ENet batches.
    pub fn flush(&mut self) {
        self.enet.flush();
    }

    pub async fn disconnect(&mut self, peer: u32, reason: u32) -> std::io::Result<()> {
        self.silence.forget(peer);
        if self.owns_wt(peer) {
            if let Some(wt) = &self.wt {
                wt.disconnect(peer, reason);
            }
            Ok(())
        } else {
            self.enet.disconnect(peer as u16, reason).await
        }
    }

    pub async fn disconnect_now(&mut self, peer: u32, reason: u32) -> std::io::Result<()> {
        self.silence.forget(peer);
        if self.owns_wt(peer) {
            if let Some(wt) = &self.wt {
                wt.disconnect(peer, reason);
            }
            Ok(())
        } else {
            self.enet.disconnect_now(peer as u16, reason).await
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::transport::webtransport::WtConfig;
    use crate::transport::HostConfig;
    use sha2::{Digest, Sha256};
    use std::time::Duration;

    pub(crate) const SLOT_BASE: u32 = 100;

    pub(crate) struct Listening {
        wt: WtHost,
        events: UnboundedReceiver<Event>,
        hash: Vec<u8>,
    }

    pub(crate) fn listen(slot_capacity: usize) -> Listening {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let hash = Sha256::digest(cert.cert.der()).to_vec();
        let (wt, events) = WtHost::start(WtConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            cert_pem: cert.cert.pem(),
            key_pem: cert.signing_key.serialize_pem(),
            slot_base: SLOT_BASE,
            slot_capacity,
            max_message_bytes: 4096,
            max_datagram_bytes: 1200,
            service_timeout_ms: 1,
            server_full_reason: 6,
        })
        .unwrap();
        Listening { wt, events, hash }
    }

    pub(crate) async fn connect(
        listening: Listening,
        clients: usize,
    ) -> (Transports, Vec<(u32, web_transport_quinn::Session)>) {
        let address = listening.wt.local_addr();
        let enet = Host::bind(HostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            max_peers: 1,
            channel_limit: 3,
        })
        .await
        .unwrap();
        let mut transports =
            Transports::with_webtransport(enet, SLOT_BASE, listening.wt, listening.events);
        let mut connected = vec![];
        for _ in 0..clients {
            let client = web_transport_quinn::ClientBuilder::new()
                .with_server_certificate_hashes(vec![listening.hash.clone()])
                .unwrap();
            let session = client
                .connect(url::Url::parse(&format!("https://{address}/")).unwrap())
                .await
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let peer = loop {
                assert!(Instant::now() < deadline, "WebTransport connect timed out");
                if let Some(Event::Connect { peer, .. }) = transports.service().await.unwrap() {
                    break peer as u32;
                }
            };
            connected.push((peer, session));
        }
        (transports, connected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::silence::RELAY_SILENCE;
    use crate::transport::webtransport::framing::stream_frame;
    use std::io::ErrorKind;
    use std::time::Duration;

    #[test]
    fn a_webtransport_relay_member_is_silent_five_seconds_after_its_last_message() {
        let listening = testing::listen(2);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let (mut transports, connected) = testing::connect(listening, 2).await;
            let (quiet, talking) = (connected[0].0, connected[1].0);
            transports.set_relay_timeout(quiet, true, true);
            transports.set_relay_timeout(talking, true, true);
            transports.set_relay_timeout(0, true, true);
            let (mut send, _recv) = connected[1].1.open_bi().await.unwrap();
            let before = Instant::now();
            send.write_all(&stream_frame(b"input")).await.unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                assert!(Instant::now() < deadline, "WebTransport receive timed out");
                if let Some(Event::Receive { peer, .. }) = transports.service().await.unwrap() {
                    assert_eq!(peer as u32, talking);
                    break;
                }
            }
            let heard = Instant::now();
            let short = RELAY_SILENCE - Duration::from_nanos(1);
            assert_eq!(transports.silent_relay_peers(before + short), vec![quiet]);
            assert_eq!(
                transports.silent_relay_peers(heard + RELAY_SILENCE),
                vec![quiet, talking]
            );
            transports.set_relay_timeout(quiet, true, false);
            assert_eq!(
                transports.silent_relay_peers(heard + RELAY_SILENCE),
                vec![talking]
            );
            transports.disconnect_now(talking, 3).await.unwrap();
            assert_eq!(
                transports.silent_relay_peers(heard + RELAY_SILENCE),
                Vec::<u32>::new()
            );
        });
    }

    #[test]
    fn a_control_notice_reaches_a_webtransport_peer_whose_data_budget_is_full() {
        let listening = testing::listen(1);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let (mut transports, connected) = testing::connect(listening, 1).await;
            let peer = connected[0].0;
            let payload = || Packet::reliable(0, vec![7; 1024]);
            let notice = || Packet::reliable(0, vec![7; 18]);
            while transports.send_application(peer, payload()).await.is_ok() {}
            let refused = transports.send_application(peer, notice()).await;
            assert_eq!(refused.unwrap_err().kind(), ErrorKind::WouldBlock);
            for _ in 0..16 {
                transports.send_control(peer, notice()).await.unwrap();
            }
            let refused = transports.send_application(peer, notice()).await;
            assert_eq!(refused.unwrap_err().kind(), ErrorKind::WouldBlock);
            let exhausted = loop {
                if let Err(error) = transports.send_control(peer, payload()).await {
                    break error;
                }
            };
            assert_eq!(exhausted.kind(), ErrorKind::WouldBlock);
        });
    }
}
