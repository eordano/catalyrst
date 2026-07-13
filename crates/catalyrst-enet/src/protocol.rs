use bytes::Bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    None = 0,
    Acknowledge = 1,
    Connect = 2,
    VerifyConnect = 3,
    Disconnect = 4,
    Ping = 5,
    SendReliable = 6,
    SendUnreliable = 7,
    SendFragment = 8,
    SendUnsequenced = 9,
    BandwidthLimit = 10,
    ThrottleConfigure = 11,
    SendUnreliableFragment = 12,
}

impl Command {
    pub fn from_u8(v: u8) -> Option<Self> {
        use Command::*;
        Some(match v & COMMAND_MASK {
            0 => None,
            1 => Acknowledge,
            2 => Connect,
            3 => VerifyConnect,
            4 => Disconnect,
            5 => Ping,
            6 => SendReliable,
            7 => SendUnreliable,
            8 => SendFragment,
            9 => SendUnsequenced,
            10 => BandwidthLimit,
            11 => ThrottleConfigure,
            12 => SendUnreliableFragment,
            _ => return Option::None,
        })
    }
}

pub const COMMAND_MASK: u8 = 0x0F;

pub const COMMAND_FLAG_ACKNOWLEDGE: u8 = 0x80;

pub const COMMAND_FLAG_UNSEQUENCED: u8 = 0x40;

pub const PROTOCOL_HEADER_MIN_SIZE: usize = 2;

pub const PROTOCOL_HEADER_MAX_SIZE: usize = 4;
pub const COMMAND_HEADER_SIZE: usize = 4;
pub const PROTOCOL_MINIMUM_MTU: usize = 576;
pub const PROTOCOL_MAXIMUM_MTU: usize = 4096;
pub const PROTOCOL_MAXIMUM_CHANNEL_COUNT: usize = 255;

pub const PROTOCOL_MAXIMUM_PEER_ID: u16 = 0xFFF;

pub const PROTOCOL_HEADER_PEER_MASK: u16 = 0x0FFF;

pub const PROTOCOL_HEADER_SESSION_SHIFT: u16 = 12;

pub const PROTOCOL_HEADER_SESSION_MASK: u16 = 0x3;

pub const PROTOCOL_HEADER_FLAG_COMPRESSED: u16 = 1 << 14;

pub const PROTOCOL_HEADER_FLAG_SENT_TIME: u16 = 1 << 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolHeader {
    pub peer_id: u16,

    pub session_id: u8,

    pub compressed: bool,

    pub has_sent_time: bool,

    pub sent_time: u16,
}

impl ProtocolHeader {
    pub fn new(peer_id: u16) -> Self {
        Self {
            peer_id: peer_id & PROTOCOL_HEADER_PEER_MASK,
            session_id: 0,
            compressed: false,
            has_sent_time: false,
            sent_time: 0,
        }
    }

    pub fn wire_size(&self) -> usize {
        if self.has_sent_time {
            PROTOCOL_HEADER_MAX_SIZE
        } else {
            PROTOCOL_HEADER_MIN_SIZE
        }
    }

    pub fn decode_with_size(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < PROTOCOL_HEADER_MIN_SIZE {
            return None;
        }
        let packed = u16::from_be_bytes([buf[0], buf[1]]);
        let peer_id = packed & PROTOCOL_HEADER_PEER_MASK;
        let session_id =
            ((packed >> PROTOCOL_HEADER_SESSION_SHIFT) & PROTOCOL_HEADER_SESSION_MASK) as u8;
        let compressed = packed & PROTOCOL_HEADER_FLAG_COMPRESSED != 0;
        let has_sent_time = packed & PROTOCOL_HEADER_FLAG_SENT_TIME != 0;
        if has_sent_time {
            if buf.len() < PROTOCOL_HEADER_MAX_SIZE {
                return None;
            }
            let sent_time = u16::from_be_bytes([buf[2], buf[3]]);
            Some((
                Self {
                    peer_id,
                    session_id,
                    compressed,
                    has_sent_time,
                    sent_time,
                },
                PROTOCOL_HEADER_MAX_SIZE,
            ))
        } else {
            Some((
                Self {
                    peer_id,
                    session_id,
                    compressed,
                    has_sent_time,
                    sent_time: 0,
                },
                PROTOCOL_HEADER_MIN_SIZE,
            ))
        }
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        Self::decode_with_size(buf).map(|(h, _)| h)
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        let mut packed = self.peer_id & PROTOCOL_HEADER_PEER_MASK;
        packed |= ((self.session_id as u16) & PROTOCOL_HEADER_SESSION_MASK)
            << PROTOCOL_HEADER_SESSION_SHIFT;
        if self.compressed {
            packed |= PROTOCOL_HEADER_FLAG_COMPRESSED;
        }
        if self.has_sent_time {
            packed |= PROTOCOL_HEADER_FLAG_SENT_TIME;
        }
        out.extend_from_slice(&packed.to_be_bytes());
        if self.has_sent_time {
            out.extend_from_slice(&self.sent_time.to_be_bytes());
        }
    }
}

pub fn clamp_mtu(mtu: u32) -> u32 {
    mtu.clamp(PROTOCOL_MINIMUM_MTU as u32, PROTOCOL_MAXIMUM_MTU as u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandHeader {
    pub command: u8,
    pub channel_id: u8,
    pub reliable_sequence_number: u16,
}

impl CommandHeader {
    pub fn kind(&self) -> Option<Command> {
        Command::from_u8(self.command)
    }
    pub fn is_acknowledged(&self) -> bool {
        self.command & COMMAND_FLAG_ACKNOWLEDGE != 0
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < COMMAND_HEADER_SIZE {
            return None;
        }
        Some(Self {
            command: buf[0],
            channel_id: buf[1],
            reliable_sequence_number: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.command);
        out.push(self.channel_id);
        out.extend_from_slice(&self.reliable_sequence_number.to_be_bytes());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectBody {
    pub outgoing_peer_id: u16,
    pub incoming_session_id: u8,
    pub outgoing_session_id: u8,
    pub mtu: u32,
    pub window_size: u32,
    pub channel_count: u32,
    pub incoming_bandwidth: u32,
    pub outgoing_bandwidth: u32,
    pub packet_throttle_interval: u32,
    pub packet_throttle_acceleration: u32,
    pub packet_throttle_deceleration: u32,
    pub connect_id: u32,
}

pub const CONNECT_BODY_SIZE: usize = 2 + 1 + 1 + 4 * 9;

impl ConnectBody {
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < CONNECT_BODY_SIZE {
            return None;
        }
        let u16a = |o: usize| u16::from_be_bytes([buf[o], buf[o + 1]]);
        let u32a = |o: usize| u32::from_be_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        Some(Self {
            outgoing_peer_id: u16a(0),
            incoming_session_id: buf[2],
            outgoing_session_id: buf[3],
            mtu: clamp_mtu(u32a(4)),
            window_size: u32a(8),
            channel_count: u32a(12),
            incoming_bandwidth: u32a(16),
            outgoing_bandwidth: u32a(20),
            packet_throttle_interval: u32a(24),
            packet_throttle_acceleration: u32a(28),
            packet_throttle_deceleration: u32a(32),
            connect_id: u32a(36),
        })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.outgoing_peer_id.to_be_bytes());
        out.push(self.incoming_session_id);
        out.push(self.outgoing_session_id);
        for v in [
            self.mtu,
            self.window_size,
            self.channel_count,
            self.incoming_bandwidth,
            self.outgoing_bandwidth,
            self.packet_throttle_interval,
            self.packet_throttle_acceleration,
            self.packet_throttle_deceleration,
            self.connect_id,
        ] {
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcknowledgeBody {
    pub received_reliable_sequence_number: u16,
    pub received_sent_time: u16,
}

pub const ACKNOWLEDGE_BODY_SIZE: usize = 4;

impl AcknowledgeBody {
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < ACKNOWLEDGE_BODY_SIZE {
            return None;
        }
        Some(Self {
            received_reliable_sequence_number: u16::from_be_bytes([buf[0], buf[1]]),
            received_sent_time: u16::from_be_bytes([buf[2], buf[3]]),
        })
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.received_reliable_sequence_number.to_be_bytes());
        out.extend_from_slice(&self.received_sent_time.to_be_bytes());
    }
}

#[derive(Debug, Clone)]
pub enum DecodedCommand {
    Acknowledge(AcknowledgeBody),
    Connect(ConnectBody),
    VerifyConnect(ConnectBody),
    Disconnect {
        data: u32,
    },
    Ping,
    SendReliable {
        channel: u8,
        sequence: u16,
        data: Bytes,
    },
    SendUnreliable {
        channel: u8,
        unreliable_sequence: u16,
        data: Bytes,
    },
    SendUnsequenced {
        channel: u8,
        unsequenced_group: u16,
        data: Bytes,
    },

    Other(Command),
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub header: CommandHeader,
    pub body: DecodedCommand,
    pub size: usize,
}

pub fn parse_command(buf: &[u8]) -> Option<ParsedCommand> {
    let header = CommandHeader::decode(buf)?;
    let body = &buf[COMMAND_HEADER_SIZE..];
    let kind = header.kind()?;
    let (decoded, body_size) = match kind {
        Command::Acknowledge => (
            DecodedCommand::Acknowledge(AcknowledgeBody::decode(body)?),
            ACKNOWLEDGE_BODY_SIZE,
        ),
        Command::Connect => (
            DecodedCommand::Connect(ConnectBody::decode(body)?),
            CONNECT_BODY_SIZE + 4,
        ),
        Command::VerifyConnect => (
            DecodedCommand::VerifyConnect(ConnectBody::decode(body)?),
            CONNECT_BODY_SIZE,
        ),
        Command::Disconnect => {
            if body.len() < 4 {
                return None;
            }
            let data = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
            (DecodedCommand::Disconnect { data }, 4)
        }
        Command::Ping => (DecodedCommand::Ping, 0),
        Command::SendReliable => {
            if body.len() < 2 {
                return None;
            }
            let len = u16::from_be_bytes([body[0], body[1]]) as usize;
            if body.len() < 2 + len {
                return None;
            }
            let data = Bytes::copy_from_slice(&body[2..2 + len]);
            (
                DecodedCommand::SendReliable {
                    channel: header.channel_id,
                    sequence: header.reliable_sequence_number,
                    data,
                },
                2 + len,
            )
        }
        Command::SendUnreliable => {
            if body.len() < 4 {
                return None;
            }
            let unreliable_sequence = u16::from_be_bytes([body[0], body[1]]);
            let len = u16::from_be_bytes([body[2], body[3]]) as usize;
            if body.len() < 4 + len {
                return None;
            }
            let data = Bytes::copy_from_slice(&body[4..4 + len]);
            (
                DecodedCommand::SendUnreliable {
                    channel: header.channel_id,
                    unreliable_sequence,
                    data,
                },
                4 + len,
            )
        }
        Command::SendUnsequenced => {
            if body.len() < 4 {
                return None;
            }
            let unsequenced_group = u16::from_be_bytes([body[0], body[1]]);
            let len = u16::from_be_bytes([body[2], body[3]]) as usize;
            if body.len() < 4 + len {
                return None;
            }
            let data = Bytes::copy_from_slice(&body[4..4 + len]);
            (
                DecodedCommand::SendUnsequenced {
                    channel: header.channel_id,
                    unsequenced_group,
                    data,
                },
                4 + len,
            )
        }
        Command::BandwidthLimit => (DecodedCommand::Other(Command::BandwidthLimit), 8),
        Command::ThrottleConfigure => (DecodedCommand::Other(Command::ThrottleConfigure), 12),
        Command::None => (DecodedCommand::Other(Command::None), 0),

        Command::SendFragment | Command::SendUnreliableFragment => {
            if body.len() < 20 {
                return None;
            }
            let len = u16::from_be_bytes([body[2], body[3]]) as usize;
            if body.len() < 20 + len {
                return None;
            }
            (DecodedCommand::Other(kind), 20 + len)
        }
    };
    Some(ParsedCommand {
        header,
        body: decoded,
        size: COMMAND_HEADER_SIZE + body_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_header_no_sent_time_is_two_bytes() {
        let h = ProtocolHeader::new(0x100);
        let mut out = Vec::new();
        h.encode(&mut out);
        assert_eq!(out.len(), PROTOCOL_HEADER_MIN_SIZE);
        let (back, n) = ProtocolHeader::decode_with_size(&out).unwrap();
        assert_eq!(n, 2);
        assert_eq!(back.peer_id, 0x100);
        assert_eq!(back.session_id, 0);
        assert!(!back.has_sent_time);
    }

    #[test]
    fn protocol_header_session_and_sent_time_roundtrip() {
        let h = ProtocolHeader {
            peer_id: 0x123,
            session_id: 0x3,
            compressed: false,
            has_sent_time: true,
            sent_time: 0xABCD,
        };
        let mut out = Vec::new();
        h.encode(&mut out);
        assert_eq!(out.len(), PROTOCOL_HEADER_MAX_SIZE);
        let (back, n) = ProtocolHeader::decode_with_size(&out).unwrap();
        assert_eq!(n, 4);
        assert_eq!(back, h);
    }

    #[test]
    fn peer_id_high_nibble_masked_off_index() {
        let packed: u16 = 0x0123 | (0x3 << PROTOCOL_HEADER_SESSION_SHIFT);
        let bytes = packed.to_be_bytes();
        let (h, _) = ProtocolHeader::decode_with_size(&bytes).unwrap();
        assert_eq!(h.peer_id, 0x123);
        assert_eq!(h.session_id, 0x3);
    }

    #[test]
    fn mtu_is_clamped_into_range() {
        assert_eq!(clamp_mtu(100), PROTOCOL_MINIMUM_MTU as u32);
        assert_eq!(clamp_mtu(9999), PROTOCOL_MAXIMUM_MTU as u32);
        assert_eq!(clamp_mtu(1400), 1400);
    }

    #[test]
    fn command_header_roundtrip() {
        let h = CommandHeader {
            command: Command::SendReliable as u8 | COMMAND_FLAG_ACKNOWLEDGE,
            channel_id: 3,
            reliable_sequence_number: 42,
        };
        let mut out = Vec::new();
        h.encode(&mut out);
        let back = CommandHeader::decode(&out).unwrap();
        assert_eq!(back, h);
        assert!(back.is_acknowledged());
        assert_eq!(back.kind(), Some(Command::SendReliable));
    }

    #[test]
    fn verify_connect_body_roundtrip() {
        let b = ConnectBody {
            outgoing_peer_id: 7,
            incoming_session_id: 1,
            outgoing_session_id: 2,
            mtu: 1400,
            window_size: 4096,
            channel_count: 3,
            incoming_bandwidth: 0,
            outgoing_bandwidth: 0,
            packet_throttle_interval: 5,
            packet_throttle_acceleration: 2,
            packet_throttle_deceleration: 2,
            connect_id: 0xDEADBEEF,
        };
        let mut out = Vec::new();
        b.encode(&mut out);
        assert_eq!(out.len(), CONNECT_BODY_SIZE);
        assert_eq!(ConnectBody::decode(&out), Some(b));
    }

    #[test]
    fn parse_send_reliable_command() {
        let header = CommandHeader {
            command: Command::SendReliable as u8 | COMMAND_FLAG_ACKNOWLEDGE,
            channel_id: 2,
            reliable_sequence_number: 9,
        };
        let payload = b"hello";
        let mut buf = Vec::new();
        header.encode(&mut buf);
        buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        buf.extend_from_slice(payload);
        let parsed = parse_command(&buf).unwrap();
        assert_eq!(parsed.size, buf.len());
        match parsed.body {
            DecodedCommand::SendReliable {
                channel,
                sequence,
                data,
            } => {
                assert_eq!(channel, 2);
                assert_eq!(sequence, 9);
                assert_eq!(&data[..], payload);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_acknowledge_command() {
        let header = CommandHeader {
            command: Command::Acknowledge as u8,
            channel_id: 0,
            reliable_sequence_number: 0,
        };
        let body = AcknowledgeBody {
            received_reliable_sequence_number: 11,
            received_sent_time: 22,
        };
        let mut buf = Vec::new();
        header.encode(&mut buf);
        body.encode(&mut buf);
        let parsed = parse_command(&buf).unwrap();
        assert_eq!(parsed.size, COMMAND_HEADER_SIZE + ACKNOWLEDGE_BODY_SIZE);
        match parsed.body {
            DecodedCommand::Acknowledge(b) => assert_eq!(b, body),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
