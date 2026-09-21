use super::auth::VerifiedRoom;
use crate::decentraland::pulse::{
    server_message, ApplicationData, ApplicationErrorCode, ApplicationJoinResult, ApplicationPeer,
    ApplicationPeerJoined, ApplicationPeerLeft, ApplicationSend, ApplicationSendFailed,
    ServerMessage,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

pub const MAX_RELIABLE_PAYLOAD: usize = 3072;
pub const MAX_UNRELIABLE_PAYLOAD: usize = 1024;
pub const MAX_ROOM_PEERS: usize = 64;
pub const MAX_PEER_ROOMS: usize = 16;
pub const MAX_ROOMS: usize = 1024;
const MAX_MESSAGES_PER_SECOND: u32 = 120;
const MAX_BYTES_PER_SECOND: usize = 128 * 1024;
const MAX_LEFT_LOG: usize = 64;
const LEFT_LOG_AGE: Duration = Duration::from_secs(10);
const MAX_NOTICE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub target: u32,
    pub unreliable: bool,
    pub message: ServerMessage,
}

impl Delivery {
    pub fn is_data(&self) -> bool {
        matches!(
            self.message.message,
            Some(
                server_message::Message::ApplicationData(_)
                    | server_message::Message::ApplicationSendFailed(_)
            )
        )
    }

    pub fn room_id(&self) -> Option<u32> {
        match self.message.message.as_ref()? {
            server_message::Message::ApplicationData(data) => Some(data.room_id),
            server_message::Message::ApplicationPeerJoined(joined) => Some(joined.room_id),
            server_message::Message::ApplicationPeerLeft(left) => Some(left.room_id),
            server_message::Message::ApplicationJoinResult(result) => Some(result.room_id),
            server_message::Message::ApplicationSendFailed(failed) => Some(failed.room_id),
            _ => None,
        }
        .filter(|room_id| *room_id != 0)
    }
}

pub struct Membership {
    pub room_id: u32,
    pub actor_id: u32,
    pub request_id: u32,
    pub verified: VerifiedRoom,
    pub lease_deadline: Instant,
    pub renewing: bool,
    pub joined_version: u32,
    pub notice_capable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaySendError {
    InvalidPayload,
    UnknownScope,
    ExpiredScope,
    RateLimited,
}

struct Left {
    version: u32,
    joined_version: u32,
    peer_id: u32,
    identity: String,
    at: Instant,
}

#[derive(Default)]
struct Roster {
    version: u32,
    left: VecDeque<Left>,
}

#[derive(Default)]
pub struct RelayState {
    pub memberships: HashMap<(u32, u32), Membership>,
    rooms: HashMap<String, HashMap<u32, u32>>,
    rosters: HashMap<String, Roster>,
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

    pub fn holds_scope(&self, peer: u32) -> bool {
        self.peer_rooms.contains_key(&peer)
    }

    fn roster_version(&self, room: &str) -> u32 {
        self.rosters.get(room).map_or(0, |roster| roster.version)
    }

    pub fn leave_version(&self, peer: u32, room_id: u32) -> u32 {
        self.memberships.get(&(peer, room_id)).map_or(0, |member| {
            self.roster_version(&member.verified.claims.video.room)
                .saturating_add(1)
        })
    }

    pub fn send_failed(&self, unsent: &Delivery) -> Option<Delivery> {
        let server_message::Message::ApplicationData(data) = unsent.message.message.as_ref()?
        else {
            return None;
        };
        if unsent.unreliable {
            return None;
        }
        let recipient = self.memberships.get(&(unsent.target, data.room_id))?;
        let (peer, sender) = self
            .rooms
            .get(&recipient.verified.claims.video.room)?
            .iter()
            .map(|(peer, room_id)| (*peer, &self.memberships[&(*peer, *room_id)]))
            .find(|(_, member)| member.actor_id == data.sender_id)?;
        sender.notice_capable.then(|| {
            send_failed(
                peer,
                sender.room_id,
                vec![recipient.verified.claims.sub.clone()],
                data.payload.clone(),
            )
        })
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
        let roster = self.rosters.entry(room.clone()).or_default();
        roster.version = roster.version.saturating_add(1);
        let roster_version = roster.version;
        let membership = Membership {
            room_id: handle,
            actor_id: handle,
            request_id,
            verified,
            lease_deadline,
            renewing: false,
            joined_version: roster_version,
            notice_capable: false,
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
                            roster_version,
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
                        roster_version: self.roster_version(&membership.verified.claims.video.room),
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
        let roster_version = self.leave_roster(room, &member);
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
                            roster_version,
                        },
                    )),
                },
            })
            .collect()
    }

    fn leave_roster(&mut self, room: &str, member: &Membership) -> u32 {
        if !self.rooms.contains_key(room) {
            let roster = self.rosters.remove(room).unwrap_or_default();
            return roster.version.saturating_add(1);
        }
        let now = Instant::now();
        let roster = self.rosters.entry(room.to_owned()).or_default();
        roster.version = roster.version.saturating_add(1);
        roster.left.push_back(Left {
            version: roster.version,
            joined_version: member.joined_version,
            peer_id: member.actor_id,
            identity: member.verified.claims.sub.clone(),
            at: now,
        });
        while roster.left.len() > MAX_LEFT_LOG
            || roster
                .left
                .front()
                .is_some_and(|left| now.duration_since(left.at) >= LEFT_LOG_AGE)
        {
            roster.left.pop_front();
        }
        roster.version
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
        let Some(sender) = self.memberships.get_mut(&(peer, send.room_id)) else {
            return Err(RelaySendError::UnknownScope);
        };
        if sender.lease_deadline <= now {
            return Err(RelaySendError::ExpiredScope);
        }
        sender.notice_capable |= send.roster_version != 0;
        let sender = &self.memberships[&(peer, send.room_id)];
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
        let seen =
            |joined_version| send.roster_version == 0 || joined_version <= send.roster_version;
        let mut out = self
            .rooms
            .get(&sender.verified.claims.video.room)
            .into_iter()
            .flat_map(|room| room.iter())
            .filter_map(|(target, room_id)| {
                let member = &self.memberships[&(*target, *room_id)];
                (*target != peer
                    && member.lease_deadline > now
                    && seen(member.joined_version)
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
            .collect::<Vec<_>>();
        if send.roster_version == 0 || send.unreliable {
            return Ok(out);
        }
        let mut identities = Vec::<&str>::new();
        for left in self
            .rosters
            .get(&sender.verified.claims.video.room)
            .into_iter()
            .flat_map(|roster| roster.left.iter())
        {
            if left.version > send.roster_version
                && left.joined_version <= send.roster_version
                && now.saturating_duration_since(left.at) < LEFT_LOG_AGE
                && (send.recipient_id == 0 || left.peer_id == send.recipient_id)
                && !identities.contains(&left.identity.as_str())
            {
                identities.push(&left.identity);
            }
        }
        if identities.is_empty() {
            return Ok(out);
        }
        let notice = |identities: Vec<String>| {
            send_failed(peer, send.room_id, identities, send.payload.clone())
        };
        let base = prost::Message::encoded_len(&notice(vec![]).message) + 8;
        let mut batch = (vec![], base);
        for identity in identities {
            let cost = 1 + prost::length_delimiter_len(identity.len()) + identity.len();
            if !batch.0.is_empty() && batch.1 + cost > MAX_NOTICE_BYTES {
                out.push(notice(std::mem::take(&mut batch.0)));
                batch.1 = base;
            }
            batch.0.push(identity.to_owned());
            batch.1 += cost;
        }
        if !batch.0.is_empty() {
            out.push(notice(batch.0));
        }
        Ok(out)
    }
}

fn send_failed(target: u32, room_id: u32, identities: Vec<String>, payload: Vec<u8>) -> Delivery {
    Delivery {
        target,
        unreliable: false,
        message: ServerMessage {
            message: Some(server_message::Message::ApplicationSendFailed(
                ApplicationSendFailed {
                    room_id,
                    identities,
                    payload,
                },
            )),
        },
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
    fn enter(
        state: &mut RelayState,
        peer: u32,
        name: &str,
        now: Instant,
    ) -> (ApplicationJoinResult, Vec<Delivery>) {
        let mut out = state.join(
            peer,
            peer + 1,
            room(peer, name),
            now + Duration::from_secs(60),
            now,
        );
        match out.pop().unwrap().message.message.unwrap() {
            server_message::Message::ApplicationJoinResult(result) => (result, out),
            _ => panic!(),
        }
    }
    fn identity(peer: u32) -> String {
        format!("0x{peer:040x}")
    }
    fn stamped(
        room_id: u32,
        recipient_id: u32,
        unreliable: bool,
        roster_version: u32,
    ) -> ApplicationSend {
        ApplicationSend {
            room_id,
            recipient_id,
            payload: vec![5, 6],
            unreliable,
            roster_version,
        }
    }
    fn data_targets(out: &[Delivery]) -> Vec<u32> {
        let mut targets = out
            .iter()
            .filter(|delivery| {
                matches!(
                    delivery.message.message,
                    Some(server_message::Message::ApplicationData(_))
                )
            })
            .map(|delivery| delivery.target)
            .collect::<Vec<_>>();
        targets.sort_unstable();
        targets
    }
    fn failures(out: &[Delivery]) -> Vec<(u32, bool, ApplicationSendFailed)> {
        out.iter()
            .filter_map(|delivery| match delivery.message.message.as_ref()? {
                server_message::Message::ApplicationSendFailed(failed) => {
                    Some((delivery.target, delivery.unreliable, failed.clone()))
                }
                _ => None,
            })
            .collect()
    }
    fn notice_versions(out: &[Delivery]) -> Vec<(u32, u32)> {
        let mut versions = out
            .iter()
            .filter_map(|delivery| match delivery.message.message.as_ref()? {
                server_message::Message::ApplicationPeerJoined(joined) => {
                    Some((delivery.target, joined.roster_version))
                }
                server_message::Message::ApplicationPeerLeft(left) => {
                    Some((delivery.target, left.roster_version))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        versions.sort_unstable();
        versions
    }

    #[test]
    fn roster_version_starts_at_one_and_rises_by_one_per_join_and_per_leave() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (first, notices) = enter(&mut s, 1, "scene", now);
        assert_eq!(first.roster_version, 1);
        assert!(notices.is_empty());
        let (second, notices) = enter(&mut s, 2, "scene", now);
        assert_eq!(second.roster_version, 2);
        assert_eq!(notice_versions(&notices), vec![(1, 2)]);
        let (third, notices) = enter(&mut s, 3, "scene", now);
        assert_eq!(third.roster_version, 3);
        assert_eq!(notice_versions(&notices), vec![(1, 3), (2, 3)]);
        let (repeated, notices) = enter(&mut s, 1, "scene", now);
        assert_eq!(
            (repeated.room_id, repeated.roster_version),
            (first.room_id, 3)
        );
        assert!(notices.is_empty());
        let left = s.leave(2, second.room_id);
        assert_eq!(notice_versions(&left), vec![(1, 4), (3, 4)]);
        let (fourth, notices) = enter(&mut s, 4, "scene", now);
        assert_eq!(fourth.roster_version, 5);
        assert_eq!(notice_versions(&notices), vec![(1, 5), (3, 5)]);
        assert_eq!(enter(&mut s, 1, "other", now).0.roster_version, 1);
        for (peer, handle) in [(1, first.room_id), (3, third.room_id), (4, fourth.room_id)] {
            s.leave(peer, handle);
        }
        assert_eq!(enter(&mut s, 5, "scene", now).0.roster_version, 1);
    }

    #[test]
    fn a_stamped_send_is_not_fanned_out_to_a_member_that_joined_after_it() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (known, _) = enter(&mut s, 2, "scene", now);
        let (newer, _) = enter(&mut s, 3, "scene", now);
        for unreliable in [false, true] {
            let stale = stamped(sender.room_id, 0, unreliable, known.roster_version);
            let out = s.send(1, stale, now).unwrap();
            assert_eq!(data_targets(&out), vec![2]);
            assert_eq!(out.len(), 1);
            let current = stamped(sender.room_id, 0, unreliable, newer.roster_version);
            let out = s.send(1, current, now).unwrap();
            assert_eq!(data_targets(&out), vec![2, 3]);
            assert_eq!(out.len(), 2);
        }
    }

    #[test]
    fn an_unstamped_send_reaches_every_live_member_and_is_never_answered() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (leaver, _) = enter(&mut s, 2, "scene", now);
        enter(&mut s, 3, "scene", now);
        s.leave(2, leaver.room_id);
        enter(&mut s, 4, "scene", now);
        for unreliable in [false, true] {
            let out = s
                .send(1, stamped(sender.room_id, 0, unreliable, 0), now)
                .unwrap();
            assert_eq!(data_targets(&out), vec![3, 4]);
            assert_eq!(out.len(), 2);
            let targeted = stamped(sender.room_id, leaver.self_id, unreliable, 0);
            assert!(s.send(1, targeted, now).unwrap().is_empty());
        }
        assert!(!s.memberships[&(1, sender.room_id)].notice_capable);
    }

    #[test]
    fn a_reliable_broadcast_stamped_before_a_leave_is_answered_once_with_identity_and_payload() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (leaver, _) = enter(&mut s, 2, "scene", now);
        let (stays, _) = enter(&mut s, 3, "scene", now);
        let left = s.leave(2, leaver.room_id);
        let stale = stamped(sender.room_id, 0, false, stays.roster_version);
        let out = s.send(1, stale, now).unwrap();
        assert_eq!(data_targets(&out), vec![3]);
        assert_eq!(
            failures(&out),
            vec![(
                1,
                false,
                ApplicationSendFailed {
                    room_id: sender.room_id,
                    identities: vec![identity(2)],
                    payload: vec![5, 6],
                }
            )]
        );
        let notice = out.last().unwrap();
        assert!(notice.is_data());
        assert_eq!(notice.room_id(), Some(sender.room_id));
        assert!(s.memberships[&(1, sender.room_id)].notice_capable);
        let applied = notice_versions(&left)[0].1;
        let out = s
            .send(1, stamped(sender.room_id, 0, false, applied), now)
            .unwrap();
        assert_eq!(data_targets(&out), vec![3]);
        assert!(failures(&out).is_empty());
    }

    #[test]
    fn a_targeted_reliable_send_to_a_just_left_peer_is_answered_and_other_targets_are_not() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (leaver, _) = enter(&mut s, 2, "scene", now);
        let (stays, _) = enter(&mut s, 3, "scene", now);
        s.leave(2, leaver.room_id);
        let stale =
            |recipient_id| stamped(sender.room_id, recipient_id, false, stays.roster_version);
        let out = s.send(1, stale(leaver.self_id), now).unwrap();
        assert_eq!(
            failures(&out),
            vec![(
                1,
                false,
                ApplicationSendFailed {
                    room_id: sender.room_id,
                    identities: vec![identity(2)],
                    payload: vec![5, 6],
                }
            )]
        );
        assert_eq!(out.len(), 1);
        let out = s.send(1, stale(stays.self_id), now).unwrap();
        assert_eq!(data_targets(&out), vec![3]);
        assert_eq!(out.len(), 1);
        assert!(s.send(1, stale(u32::MAX - 1), now).unwrap().is_empty());
    }

    #[test]
    fn a_member_that_came_and_went_after_the_stamp_is_not_named_but_a_known_one_that_returned_is() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (known, _) = enter(&mut s, 2, "scene", now);
        let seen = known.roster_version;
        let (visitor, _) = enter(&mut s, 3, "scene", now);
        s.leave(3, visitor.room_id);
        let out = s
            .send(1, stamped(sender.room_id, 0, false, seen), now)
            .unwrap();
        assert_eq!(data_targets(&out), vec![2]);
        assert!(failures(&out).is_empty());
        let targeted = stamped(sender.room_id, visitor.self_id, false, seen);
        assert!(s.send(1, targeted, now).unwrap().is_empty());
        s.leave(2, known.room_id);
        let (returned, _) = enter(&mut s, 2, "scene", now);
        s.leave(2, returned.room_id);
        let out = s
            .send(1, stamped(sender.room_id, 0, false, seen), now)
            .unwrap();
        assert!(data_targets(&out).is_empty());
        assert_eq!(
            failures(&out)
                .into_iter()
                .map(|(_, _, failed)| failed.identities)
                .collect::<Vec<_>>(),
            vec![vec![identity(2)]]
        );
    }

    #[test]
    fn an_unreliable_send_is_never_answered() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let (leaver, _) = enter(&mut s, 2, "scene", now);
        let (stays, _) = enter(&mut s, 3, "scene", now);
        s.leave(2, leaver.room_id);
        let stale =
            |recipient_id| stamped(sender.room_id, recipient_id, true, stays.roster_version);
        let out = s.send(1, stale(0), now).unwrap();
        assert_eq!(data_targets(&out), vec![3]);
        assert_eq!(out.len(), 1);
        assert!(s.send(1, stale(leaver.self_id), now).unwrap().is_empty());
        let reliable = stamped(sender.room_id, 0, false, stays.roster_version);
        assert_eq!(failures(&s.send(1, reliable, now).unwrap()).len(), 1);
    }

    #[test]
    fn the_leave_log_keeps_the_newest_sixty_four_entries_for_ten_seconds() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let known = (100..163)
            .map(|peer| (peer, enter(&mut s, peer, "scene", now).0))
            .collect::<Vec<_>>();
        let seen = known.last().unwrap().1.roster_version;
        for (peer, member) in &known {
            s.leave(*peer, member.room_id);
        }
        for peer in 200..207 {
            let (visitor, _) = enter(&mut s, peer, "scene", now);
            s.leave(peer, visitor.room_id);
        }
        let stale = || stamped(sender.room_id, 0, false, seen);
        let out = s.send(1, stale(), now).unwrap();
        let named = failures(&out)
            .into_iter()
            .flat_map(|(_, _, failed)| failed.identities)
            .collect::<Vec<_>>();
        assert_eq!(named, (106..163).map(identity).collect::<Vec<_>>());
        let within = Instant::now() + Duration::from_secs(9);
        assert!(!failures(&s.send(1, stale(), within).unwrap()).is_empty());
        let beyond = Instant::now() + Duration::from_secs(11);
        assert!(s.send(1, stale(), beyond).unwrap().is_empty());
    }

    #[test]
    fn a_notice_too_large_for_one_reliable_frame_is_split_without_losing_an_identity() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let (sender, _) = enter(&mut s, 1, "scene", now);
        let members = (100..140)
            .map(|peer| (peer, enter(&mut s, peer, "scene", now).0))
            .collect::<Vec<_>>();
        let seen = members.last().unwrap().1.roster_version;
        for (peer, member) in &members {
            s.leave(*peer, member.room_id);
        }
        let send = ApplicationSend {
            payload: vec![7; MAX_RELIABLE_PAYLOAD],
            ..stamped(sender.room_id, 0, false, seen)
        };
        let out = s.send(1, send, now).unwrap();
        assert!(out.len() > 1);
        assert!(out
            .iter()
            .all(|delivery| prost::Message::encoded_len(&delivery.message) <= 4096));
        let failed = failures(&out);
        assert_eq!(failed.len(), out.len());
        assert!(
            failed
                .iter()
                .all(|(target, _, failed)| *target == 1
                    && failed.payload == [7; MAX_RELIABLE_PAYLOAD])
        );
        let named = failed
            .into_iter()
            .flat_map(|(_, _, failed)| failed.identities)
            .collect::<Vec<_>>();
        assert_eq!(named, (100..140).map(identity).collect::<Vec<_>>());
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
                    unreliable: false,
                    roster_version: 0,
                },
                now
            )
            .unwrap()
            .len(),
            40
        );
    }
    #[test]
    fn every_delivery_names_its_targets_own_room_handle_and_only_payloads_are_data() {
        let mut s = RelayState::default();
        let now = Instant::now();
        let a = join(&mut s, 1, "scene", now);
        let joined = s.join(2, 3, room(2, "scene"), now + Duration::from_secs(5), now);
        let b = s.membership(2, "scene").unwrap().room_id;
        let send = ApplicationSend {
            room_id: b,
            recipient_id: 0,
            payload: vec![5],
            unreliable: false,
            roster_version: 0,
        };
        let sent = s.send(2, send, now).unwrap();
        let left = s.leave(2, b);
        let handles = |out: &[Delivery]| {
            out.iter()
                .map(|delivery| (delivery.target, delivery.room_id(), delivery.is_data()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            handles(&joined),
            vec![(1, Some(a), false), (2, Some(b), false)]
        );
        assert_eq!(handles(&sent), vec![(1, Some(a), true)]);
        assert_eq!(handles(&left), vec![(1, Some(a), false)]);
        let refused = failure(2, 3, ApplicationErrorCode::ApplicationLimit);
        assert_eq!((refused.room_id(), refused.is_data()), (None, false));
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
            roster_version: 0,
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
