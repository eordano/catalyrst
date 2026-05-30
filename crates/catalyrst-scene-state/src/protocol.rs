//! Wire protocol for the scene-state-server WebSocket transport.
//!
//! Faithful port of `src/logic/protocol.ts` upstream. Every WS frame is a
//! binary message whose first byte is a [`MessageType`] discriminant; the
//! remaining bytes are the message body.
//!
//! Three message types exist (matching upstream exactly):
//!
//! - `Auth` (1)  — client -> server, **first** frame. Body is the UTF-8 JSON of
//!   the signed-fetch `x-identity-*` headers the client would otherwise put on
//!   an HTTP request to `GET /ws/:scene`. The server runs the same auth-chain
//!   verification it would for a signed fetch. See [`crate::auth`].
//! - `Init` (2)  — server -> client, sent once immediately after a client is
//!   accepted. Carries the entity-id range assigned to this client plus a
//!   snapshot of the current CRDT state. Layout (big-endian, byte 0 is the
//!   type tag):
//!
//!   ```text
//!   offset  size  field
//!   0       1     MessageType::Init (2)
//!   1       4     start                  (u32) first network entity id for client
//!   5       4     size                   (u32) number of entity ids in range
//!   9       4     localEntitiesReserved  (u32) ids reserved for local (non-net) entities
//!   13      ..    crdtState              (bytes) serialized CRDT snapshot
//!   ```
//!
//! - `Crdt` (3)  — bidirectional steady-state. Body is an opaque CRDT message
//!   buffer (a batch of SDK7 CRDT operations) that the server relays/merges and
//!   fans out to the other clients. The server treats the body as opaque bytes
//!   at the transport layer; semantic interpretation happens in the state core.

/// First-byte discriminant of every WS frame. Values match upstream `MessageType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Auth = 1,
    Init = 2,
    Crdt = 3,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Auth),
            2 => Some(Self::Init),
            3 => Some(Self::Crdt),
            _ => None,
        }
    }
}

/// Splits a raw frame into `(type, body)`. Returns `None` for an empty frame or
/// an unrecognised tag. The body is a borrow of the input (no copy).
pub fn decode_message(data: &[u8]) -> Option<(MessageType, &[u8])> {
    let (&tag, rest) = data.split_first()?;
    let ty = MessageType::from_u8(tag)?;
    Some((ty, rest))
}

/// Prepends `msg_type` to `message`, producing a frame ready to send.
/// Mirrors `encodeMessage` upstream.
pub fn encode_message(msg_type: MessageType, message: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(message.len() + 1);
    packet.push(msg_type as u8);
    packet.extend_from_slice(message);
    packet
}

/// Builds the `Init` frame. Mirrors `encodeInitMessage` upstream (big-endian).
pub fn encode_init_message(
    crdt_state: &[u8],
    start: u32,
    size: u32,
    local_entities_reserved: u32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(13 + crdt_state.len());
    buf.push(MessageType::Init as u8);
    buf.extend_from_slice(&start.to_be_bytes());
    buf.extend_from_slice(&size.to_be_bytes());
    buf.extend_from_slice(&local_entities_reserved.to_be_bytes());
    buf.extend_from_slice(crdt_state);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_crdt_frame() {
        let body = b"\x01\x02\x03";
        let frame = encode_message(MessageType::Crdt, body);
        let (ty, decoded) = decode_message(&frame).unwrap();
        assert_eq!(ty, MessageType::Crdt);
        assert_eq!(decoded, body);
    }

    #[test]
    fn init_layout_matches_upstream() {
        // start=1536, size=512, reserved=512, empty crdt state
        let frame = encode_init_message(&[], 1536, 512, 512);
        assert_eq!(frame.len(), 13);
        assert_eq!(frame[0], MessageType::Init as u8);
        assert_eq!(&frame[1..5], &1536u32.to_be_bytes());
        assert_eq!(&frame[5..9], &512u32.to_be_bytes());
        assert_eq!(&frame[9..13], &512u32.to_be_bytes());
    }

    #[test]
    fn empty_and_unknown_frames_rejected() {
        assert!(decode_message(&[]).is_none());
        assert!(decode_message(&[9]).is_none());
    }
}
