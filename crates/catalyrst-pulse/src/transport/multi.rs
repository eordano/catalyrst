use crate::transport::host::{Event, Host};
use crate::transport::webtransport::WtHost;
use crate::transport::Packet;

use tokio::sync::mpsc::UnboundedReceiver;

pub struct Transports {
    enet: Host,
    wt: Option<WtHost>,
    wt_events: Option<UnboundedReceiver<Event>>,
    enet_capacity: u32,
}

impl Transports {
    pub fn enet_only(enet: Host, enet_capacity: u32) -> Self {
        Self {
            enet,
            wt: None,
            wt_events: None,
            enet_capacity,
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
        }
    }

    fn owns_wt(&self, peer: u32) -> bool {
        self.wt.is_some() && peer >= self.enet_capacity
    }

    pub async fn service(&mut self) -> std::io::Result<Option<Event>> {
        let Self {
            enet, wt_events, ..
        } = self;
        match wt_events.as_mut() {
            Some(rx) => {
                tokio::select! {
                    enet_event = enet.service() => enet_event,
                    wt_event = rx.recv() => Ok(wt_event),
                }
            }
            None => enet.service().await,
        }
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

    pub async fn disconnect(&mut self, peer: u32, reason: u32) -> std::io::Result<()> {
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
        if self.owns_wt(peer) {
            // WebTransport has no separate immediate-close path; the session close is prompt.
            if let Some(wt) = &self.wt {
                wt.disconnect(peer, reason);
            }
            Ok(())
        } else {
            self.enet.disconnect_now(peer as u16, reason).await
        }
    }
}
