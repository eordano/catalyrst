use super::auth::VerifiedRoom;
use crate::decentraland::pulse::{
    server_message, ApplicationData, ApplicationErrorCode, ApplicationJoinResult, ApplicationPeer,
    ApplicationPeerJoined, ApplicationPeerLeft, ApplicationSend, ServerMessage,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const MAX_RELIABLE_PAYLOAD: usize = 3072;
pub const MAX_UNRELIABLE_PAYLOAD: usize = 1024;
pub const MAX_ROOM_PEERS: usize = 64;
pub const MAX_PEER_ROOMS: usize = 16;
pub const MAX_ROOMS: usize = 1024;
const MAX_MESSAGES_PER_SECOND: u32 = 120;
const MAX_BYTES_PER_SECOND: usize = 128 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub target: u32,
    pub unreliable: bool,
    pub message: ServerMessage,
}

pub struct Membership {
    pub room_id: u32,
    pub actor_id: u32,
    pub request_id: u32,
    pub verified: VerifiedRoom,
    pub lease_deadline: Instant,
    pub renewing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaySendError {
    InvalidPayload,
    UnknownScope,
    ExpiredScope,
    RateLimited,
}

#[derive(Default)]
pub struct RelayState {
    pub memberships: HashMap<(u32, u32), Membership>,
    rooms: HashMap<String, HashMap<u32, u32>>,
    peer_rooms: HashMap<u32, HashSet<u32>>,
    next_handle: u32,
    budgets: HashMap<u32, (Instant, u32, usize)>,
}

impl RelayState {
    pub fn membership(&self, peer: u32, room: &str) -> Option<&Membership> {
        let handle = self.rooms.get(room)?.get(&peer)?;
        self.memberships.get(&(peer, *handle))
    }

    pub fn room_ids_for_peer(&self, peer: u32) -> Vec<u32> {
        self.peer_rooms
            .get(&peer)
            .into_iter()
            .flat_map(|rooms| rooms.iter().copied())
            .collect()
    }

    pub fn credentials_for_peer(&self, peer: u32) -> Vec<VerifiedRoom> {
        self.peer_rooms
            .get(&peer)
            .into_iter()
            .flat_map(|rooms| rooms.iter())
            .map(|room| self.memberships[&(peer, *room)].verified.clone())
            .collect()
    }
    pub fn join(
        &mut self,
        peer: u32,
        request_id: u32,
        verified: VerifiedRoom,
        lease_deadline: Instant,
        now: Instant,
    ) -> Vec<Delivery> {
        if lease_deadline <= now {
            return vec![failure(
                peer,
                request_id,
                ApplicationErrorCode::ApplicationUnavailable,
            )];
        }
        if let Some(handle) = self
            .rooms
            .get(&verified.claims.video.room)
            .and_then(|room| room.get(&peer))
        {
            if self.memberships[&(peer, *handle)].lease_deadline <= now {
                return vec![failure(
                    peer,
                    request_id,
                    ApplicationErrorCode::ApplicationUnavailable,
                )];
            }
            return vec![self.join_result(peer, request_id, &self.memberships[&(peer, *handle)])];
        }
        let room = verified.claims.video.room.clone();
        let members = self.rooms.get(&room);
        if self
            .peer_rooms
            .get(&peer)
            .is_some_and(|rooms| rooms.len() >= MAX_PEER_ROOMS)
            || members.is_some_and(|p| p.len() >= MAX_ROOM_PEERS)
            || (members.is_none() && self.rooms.len() >= MAX_ROOMS)
            || self.next_handle == u32::MAX
        {
            return vec![failure(
                peer,
                request_id,
                ApplicationErrorCode::ApplicationLimit,
            )];
        }
        if members
            .into_iter()
            .flat_map(|peers| peers.iter())
            .any(|(p, r)| self.memberships[&(*p, *r)].verified.claims.sub == verified.claims.sub)
        {
            return vec![failure(
                peer,
                request_id,
                ApplicationErrorCode::ApplicationNotAuthorized,
            )];
        }
        self.next_handle += 1;
        let handle = self.next_handle;
        let membership = Membership {
            room_id: handle,
            actor_id: handle,
            request_id,
            verified,
            lease_deadline,
            renewing: false,
        };
        let mut out = self
            .rooms
            .get(&room)
            .into_iter()
            .flat_map(|peers| peers.iter())
            .map(|(target, room_id)| Delivery {
                target: *target,
                unreliable: false,
                message: ServerMessage {
                    message: Some(server_message::Message::ApplicationPeerJoined(
                        ApplicationPeerJoined {
                            room_id: *room_id,
                            peer: Some(peer_info(&membership)),
                        },
                    )),
                },
            })
            .collect::<Vec<_>>();
        self.rooms.entry(room).or_default().insert(peer, handle);
        self.peer_rooms.entry(peer).or_default().insert(handle);
        self.memberships.insert((peer, handle), membership);
        out.push(self.join_result(peer, request_id, &self.memberships[&(peer, handle)]));
        out
    }

    fn join_result(&self, peer: u32, request_id: u32, membership: &Membership) -> Delivery {
        let mut peers = self
            .rooms
            .get(&membership.verified.claims.video.room)
            .into_iter()
            .flat_map(|room| room.iter())
            .map(|(p, r)| peer_info(&self.memberships[&(*p, *r)]))
            .collect::<Vec<_>>();
        peers.sort_by_key(|p| p.peer_id);
        Delivery {
            target: peer,
            unreliable: false,
            message: ServerMessage {
                message: Some(server_message::Message::ApplicationJoinResult(
                    ApplicationJoinResult {
                        request_id,
                        room_id: membership.room_id,
                        self_id: membership.actor_id,
                        max_payload_bytes: MAX_RELIABLE_PAYLOAD as u32,
                        peers,
                        error: ApplicationErrorCode::ApplicationOk as i32,
                        max_unreliable_payload_bytes: MAX_UNRELIABLE_PAYLOAD as u32,
                    },
                )),
            },
        }
    }

    pub fn leave(&mut self, peer: u32, room_id: u32) -> Vec<Delivery> {
        let Some(member) = self.memberships.remove(&(peer, room_id)) else {
            return vec![];
        };
        let room = &member.verified.claims.video.room;
        if let Some(peers) = self.rooms.get_mut(room) {
            peers.remove(&peer);
            if peers.is_empty() {
                self.rooms.remove(room);
            }
        }
        if let Some(rooms) = self.peer_rooms.get_mut(&peer) {
            rooms.remove(&room_id);
            if rooms.is_empty() {
                self.peer_rooms.remove(&peer);
            }
        }
        self.rooms
            .get(room)
            .into_iter()
            .flat_map(|peers| peers.iter())
            .map(|(target, room_id)| Delivery {
                target: *target,
                unreliable: false,
                message: ServerMessage {
                    message: Some(server_message::Message::ApplicationPeerLeft(
                        ApplicationPeerLeft {
                            room_id: *room_id,
                            peer_id: member.actor_id,
                        },
                    )),
                },
            })
            .collect()
    }

    pub fn disconnect(&mut self, peer: u32) -> Vec<Delivery> {
        let rooms = self
            .peer_rooms
            .get(&peer)
            .into_iter()
            .flat_map(|rooms| rooms.iter().copied())
            .collect::<Vec<_>>();
        self.budgets.remove(&peer);
        rooms
            .into_iter()
            .flat_map(|room| self.leave(peer, room))
            .collect()
    }

    pub fn send(
        &mut self,
        peer: u32,
        send: ApplicationSend,
        now: Instant,
    ) -> Result<Vec<Delivery>, RelaySendError> {
        let cap = if send.unreliable {
            MAX_UNRELIABLE_PAYLOAD
        } else {
            MAX_RELIABLE_PAYLOAD
        };
        if send.payload.is_empty() || send.payload.len() > cap {
            return Err(RelaySendError::InvalidPayload);
        }
        let Some(sender) = self.memberships.get(&(peer, send.room_id)) else {
            return Err(RelaySendError::UnknownScope);
        };
        if sender.lease_deadline <= now {
            return Err(RelaySendError::ExpiredScope);
        }
        let budget = self.budgets.entry(peer).or_insert((now, 0, 0));
        if now.duration_since(budget.0) >= Duration::from_secs(1) {
            *budget = (now, 0, 0);
        }
        if budget.1 >= MAX_MESSAGES_PER_SECOND
            || budget.2 + send.payload.len() > MAX_BYTES_PER_SECOND
        {
            return Err(RelaySendError::RateLimited);
        }
        budget.1 += 1;
        budget.2 += send.payload.len();
        Ok(self
            .rooms
            .get(&sender.verified.claims.video.room)
            .into_iter()
            .flat_map(|room| room.iter())
            .filter_map(|(target, room_id)| {
                let member = &self.memberships[&(*target, *room_id)];
                (*target != peer
                    && member.lease_deadline > now
                    && (send.recipient_id == 0 || member.actor_id == send.recipient_id))
                    .then(|| Delivery {
                        target: *target,
                        unreliable: send.unreliable,
                        message: ServerMessage {
                            message: Some(server_message::Message::ApplicationData(
                                ApplicationData {
                                    room_id: *room_id,
                                    sender_id: sender.actor_id,
                                    payload: send.payload.clone(),
                                },
                            )),
                        },
                    })
            })
            .collect())
    }
}

fn peer_info(member: &Membership) -> ApplicationPeer {
    ApplicationPeer {
        peer_id: member.actor_id,
        identity: member.verified.claims.sub.clone(),
        is_guest: Some(member.verified.is_guest),
    }
}
pub fn failure(peer: u32, request_id: u32, error: ApplicationErrorCode) -> Delivery {
    Delivery {
        target: peer,
        unreliable: false,
        message: ServerMessage {
            message: Some(server_message::Message::ApplicationJoinResult(
                ApplicationJoinResult {
                    request_id,
                    error: error as i32,
                    ..Default::default()
                },
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::auth::{RoomClaims, RoomGrant};
    use super::*;
    fn room(peer: u32, name: &str) -> VerifiedRoom {
        VerifiedRoom {
            claims: RoomClaims {
                iss: "key".into(),
                sub: format!("0x{peer:040x}"),
                exp: 100,
                nbf: 0,
                video: RoomGrant {
                    room: name.into(),
                    room_join: true,
                    can_publish_data: true,
                },
                metadata: String::new(),
            },
            wallet: String::new(),
            session: String::new(),
            is_guest: true,
            header_payload: String::new(),
            nonce: [0; 32],
            proof: vec![],
        }
    }
    fn join(state: &mut RelayState, peer: u32, name: &str, now: Instant) -> u32 {
        let out = state.join(
            peer,
            peer + 1,
            room(peer, name),
            now + Duration::from_secs(5),
            now,
        );
        match out.last().unwrap().message.message.as_ref().unwrap() {
            server_message::Message::ApplicationJoinResult(r) => r.room_id,
            _ => panic!(),
        }
    }
    #[test]
    fn forty_other_participants_have_complete_rosters_independent_of_aoi() {
        let mut s = RelayState::default();
        let now = Instant::now();
        for p in 0..40 {
            assert_ne!(join(&mut s, p, "scene", now), 0);
        }
        let out = s.join(40, 41, room(40, "scene"), now + Duration::from_secs(1), now);
        assert_eq!(out.len(), 41);
        let Some(server_message::Message::ApplicationJoinResult(r)) =
            &out.last().unwrap().message.message
        else {
            panic!()
        };
        assert_eq!(r.peers.len(), 41);
        assert_eq!(
            s.send(
                40,
                ApplicationSend {
                    room_id: r.room_id,
                    recipient_id: 0,
                    payload: vec![1],
                    unreliable: false
                },
                now
            )
            .unwrap()
            .len(),
            40
        );
    }
    #[test]
    fn routing_is_scoped_stamped_and_fenced_by_expiry_and_disconnect() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let a = join(&mut s, 1, "a", now);
        let b = join(&mut s, 2, "a", now);
        let c = join(&mut s, 3, "b", now);
        let send = |recipient_id| ApplicationSend {
            room_id: a,
            recipient_id,
            payload: vec![5],
            unreliable: false,
        };
        assert!(s.send(1, send(c), now).unwrap().is_empty());
        let out = s.send(1, send(b), now).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target, 2);
        let Some(server_message::Message::ApplicationData(data)) = &out[0].message.message else {
            panic!()
        };
        assert_eq!((data.room_id, data.sender_id), (b, a));
        assert!(s.send(1, send(0), now + Duration::from_secs(5)).is_err());
        let left = s.disconnect(2);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].target, 1);
        let next = join(&mut s, 2, "a", now);
        assert!(next > b);
        assert!(s.send(1, send(b), now).unwrap().is_empty());
    }
    #[test]
    fn room_count_peer_count_payload_and_rate_are_bounded() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let a = join(&mut s, 0, "room", now);
        for p in 1..64 {
            assert_ne!(join(&mut s, p, "room", now), 0);
        }
        assert_eq!(join(&mut s, 64, "room", now), 0);
        let roster = s.join(63, 65, room(63, "room"), now + Duration::from_secs(5), now);
        assert!(prost::Message::encoded_len(&roster[0].message) <= 4096);
        for i in 1..16 {
            assert_ne!(join(&mut s, 0, &format!("room{i}"), now), 0);
        }
        assert_eq!(join(&mut s, 0, "seventeenth", now), 0);
        assert!(s
            .send(
                0,
                ApplicationSend {
                    room_id: a,
                    payload: vec![0; 1025],
                    unreliable: true,
                    ..Default::default()
                },
                now
            )
            .is_err());
        for _ in 0..120 {
            assert_eq!(
                s.send(
                    0,
                    ApplicationSend {
                        room_id: a,
                        payload: vec![1],
                        ..Default::default()
                    },
                    now
                )
                .unwrap()
                .len(),
                63
            );
        }
        assert!(s
            .send(
                0,
                ApplicationSend {
                    room_id: a,
                    payload: vec![1],
                    ..Default::default()
                },
                now
            )
            .is_err());
    }
}
