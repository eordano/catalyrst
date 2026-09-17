use axum::serve::{ListenerExt, TapIo};
use tokio::net::{TcpListener, TcpStream};

pub(super) fn with_nodelay(listener: TcpListener) -> TapIo<TcpListener, fn(&mut TcpStream)> {
    listener.tap_io(|stream| {
        if let Err(error) = stream.set_nodelay(true) {
            tracing::warn!(%error, "failed to enable TCP_NODELAY on accepted connection");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::ConnectInfo;
    use axum::routing::get;
    use axum::serve::Listener;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn every_accepted_stream_has_nodelay() {
        timeout(Duration::from_secs(5), async {
            let mut listener = with_nodelay(TcpListener::bind("127.0.0.1:0").await.unwrap());
            let address = listener.local_addr().unwrap();
            for _ in 0..3 {
                let client = TcpStream::connect(address).await.unwrap();
                let (server, peer) = listener.accept().await;
                assert!(server.nodelay().unwrap());
                assert_eq!(peer, client.local_addr().unwrap());
                assert_eq!(server.local_addr().unwrap(), address);
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn axum_serves_http_with_peer_address() {
        timeout(Duration::from_secs(5), async {
            let listener = with_nodelay(TcpListener::bind("127.0.0.1:0").await.unwrap());
            let address = listener.local_addr().unwrap();
            let app = axum::Router::new().route(
                "/",
                get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move { peer.to_string() }),
            );
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .unwrap();
            });
            let mut client = TcpStream::connect(address).await.unwrap();
            let peer = client.local_addr().unwrap();
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).await.unwrap();
            server.abort();
            assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(response.ends_with(&peer.to_string()));
        })
        .await
        .unwrap();
    }
}
