use super::*;
use crate::application_relay::state::Delivery;
use std::collections::{HashSet, VecDeque};

impl PulseServer {
    pub(super) async fn apply_application(
        &mut self,
        transports: &mut Transports,
        peer: u32,
        message: client_message::Message,
    ) -> anyhow::Result<()> {
        let Some(relay) = self.application_relay.as_mut() else {
            return Ok(());
        };
        let mut retire = vec![];
        let out = match message {
            client_message::Message::ApplicationJoin(join) => {
                relay.join(peer, join, chrono::Utc::now().timestamp().max(0) as u64)
            }
            client_message::Message::ApplicationLeave(leave) => relay.leave(peer, leave.room_id),
            client_message::Message::ApplicationSend(send) => {
                let (out, close) = relay.send(peer, send, std::time::Instant::now());
                if close {
                    retire.push(peer);
                }
                out
            }
            _ => vec![],
        };
        self.deliver_application(transports, out, retire).await
    }

    pub(super) async fn deliver_application(
        &mut self,
        transports: &mut Transports,
        out: Vec<Delivery>,
        retire: Vec<u32>,
    ) -> anyhow::Result<()> {
        let mut queue = VecDeque::from(out);
        let mut retire = VecDeque::from(retire);
        let mut retired = HashSet::new();
        loop {
            while let Some(peer) = retire.pop_front() {
                if !retired.insert(peer) {
                    continue;
                }
                if let Some(relay) = self.application_relay.as_mut() {
                    queue.extend(relay.disconnect(peer));
                }
                let notify = self.remove_peer_state(peer);
                transports
                    .disconnect_now(peer, DisconnectReason::AuthFailed.code())
                    .await?;
                if notify {
                    self.notify_player_left(transports, peer).await?;
                }
            }
            let Some(delivery) = queue.pop_front() else {
                break;
            };
            if retired.contains(&delivery.target) {
                continue;
            }
            let bytes = delivery.message.encode_to_vec();
            let packet = if delivery.unreliable {
                Packet::unsequenced(channel::UNRELIABLE_UNSEQUENCED, bytes)
            } else {
                Packet::reliable(channel::RELIABLE, bytes)
            };
            if transports
                .send_application(delivery.target, packet)
                .await
                .is_err()
                && !delivery.unreliable
            {
                retire.push_back(delivery.target);
            }
        }
        transports.flush();
        Ok(())
    }
}
