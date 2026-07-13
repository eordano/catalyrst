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

pub fn decode_message(data: &[u8]) -> Option<(MessageType, &[u8])> {
    let (&tag, rest) = data.split_first()?;
    let ty = MessageType::from_u8(tag)?;
    Some((ty, rest))
}

pub fn encode_message(msg_type: MessageType, message: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(message.len() + 1);
    packet.push(msg_type as u8);
    packet.extend_from_slice(message);
    packet
}

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
