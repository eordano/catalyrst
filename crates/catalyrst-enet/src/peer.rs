pub type PeerId = u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    Disconnected,
    Connecting,
    AcknowledgingConnect,
    ConnectionPending,
    ConnectionSucceeded,
    Connected,
    DisconnectLater,
    Disconnecting,
    AcknowledgingDisconnect,
    Zombie,
}

impl From<rusty_enet::PeerState> for PeerState {
    fn from(s: rusty_enet::PeerState) -> Self {
        match s {
            rusty_enet::PeerState::Disconnected => PeerState::Disconnected,
            rusty_enet::PeerState::Connecting => PeerState::Connecting,
            rusty_enet::PeerState::AcknowledgingConnect => PeerState::AcknowledgingConnect,
            rusty_enet::PeerState::ConnectionPending => PeerState::ConnectionPending,
            rusty_enet::PeerState::ConnectionSucceeded => PeerState::ConnectionSucceeded,
            rusty_enet::PeerState::Connected => PeerState::Connected,
            rusty_enet::PeerState::DisconnectLater => PeerState::DisconnectLater,
            rusty_enet::PeerState::Disconnecting => PeerState::Disconnecting,
            rusty_enet::PeerState::AcknowledgingDisconnect => PeerState::AcknowledgingDisconnect,
            rusty_enet::PeerState::Zombie => PeerState::Zombie,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer {
    pub id: PeerId,
    pub state: PeerState,
}
