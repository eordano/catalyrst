use std::net::SocketAddr;
use std::time::{Duration, Instant};

use catalyrst_enet::{Event, Host, HostConfig, Packet, PeerId};

fn cfg(port: u16) -> HostConfig {
    HostConfig {
        bind: format!("127.0.0.1:{port}").parse::<SocketAddr>().unwrap(),
        max_peers: 32,
        channel_limit: 2,
    }
}

#[tokio::test]
async fn loopback_connect_and_reliable_delivery() {
    let mut server = Host::bind(cfg(0)).await.expect("bind server");
    let mut client = Host::bind(cfg(0)).await.expect("bind client");
    let server_addr = server.local_addr().unwrap();

    client.connect(server_addr, 2).expect("connect");

    let mut server_peer: Option<PeerId> = None;
    let mut client_connected = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while (server_peer.is_none() || !client_connected) && Instant::now() < deadline {
        if let Some(Event::Connect { peer }) = server.service().await.unwrap() {
            server_peer = Some(peer);
        }
        if let Some(Event::Connect { .. }) = client.service().await.unwrap() {
            client_connected = true;
        }
    }
    let server_peer = server_peer.expect("server must observe the peer connect");
    assert!(client_connected, "client must observe its connect complete");

    let payload = b"catalyrst-enet reliable loopback".to_vec();
    server
        .send(server_peer, Packet::reliable(0, payload.clone()))
        .await
        .expect("send reliable");

    let mut received: Option<Vec<u8>> = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while received.is_none() && Instant::now() < deadline {
        let _ = server.service().await.unwrap();
        if let Some(Event::Receive {
            channel, packet, ..
        }) = client.service().await.unwrap()
        {
            assert_eq!(channel, 0);
            assert!(packet.flags.is_reliable());
            received = Some(packet.data.to_vec());
        }
    }

    assert_eq!(
        received.expect("client must receive the reliable packet"),
        payload
    );
}

#[tokio::test]
async fn disconnect_with_reason_tears_down_the_peer() {
    let mut server = Host::bind(cfg(0)).await.expect("bind server");
    let mut client = Host::bind(cfg(0)).await.expect("bind client");
    let server_addr = server.local_addr().unwrap();

    client.connect(server_addr, 2).expect("connect");

    let mut server_peer: Option<PeerId> = None;
    let mut client_connected = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while (server_peer.is_none() || !client_connected) && Instant::now() < deadline {
        if let Some(Event::Connect { peer }) = server.service().await.unwrap() {
            server_peer = Some(peer);
        }
        if let Some(Event::Connect { .. }) = client.service().await.unwrap() {
            client_connected = true;
        }
    }
    let server_peer = server_peer.expect("server must observe the peer connect");
    assert!(client_connected, "client must observe its connect complete");

    const REASON: u32 = 2;
    server
        .disconnect(server_peer, REASON)
        .await
        .expect("disconnect");

    let mut client_saw_disconnect = false;
    let mut server_saw_disconnect = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while (!client_saw_disconnect || !server_saw_disconnect) && Instant::now() < deadline {
        if let Some(Event::Disconnect { .. }) = server.service().await.unwrap() {
            server_saw_disconnect = true;
        }
        if let Some(Event::Disconnect { .. }) = client.service().await.unwrap() {
            client_saw_disconnect = true;
        }
    }

    assert!(
        client_saw_disconnect,
        "client must observe the server-initiated disconnect"
    );
    assert!(
        server_saw_disconnect,
        "server must surface its own graceful disconnect event"
    );
}
