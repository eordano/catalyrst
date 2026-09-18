use crate::feed::FeedCache;
use crate::registry::PeerLink;
use axum::extract::ws::Message;
use futures::{Sink, SinkExt};
use std::time::Duration;

const WRITE_DEADLINE: Duration = Duration::from_secs(5);

/// A failed/cancelled write may already have emitted a frame prefix. The caller
/// must retire the transport, never retry the frame or reuse that sink.
pub(super) async fn send<S: Sink<Message> + Unpin>(
    socket: &mut S,
    message: Message,
    owner: Option<&PeerLink>,
    feed: &FeedCache,
) -> bool {
    let closed = async {
        match owner {
            Some(owner) => owner.cancelled().await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        biased;
        _ = closed => false,
        _ = tokio::time::sleep(WRITE_DEADLINE) => {
            feed.on_socket_write_timeout();
            false
        }
        result = socket.send(message) => result.is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{PeersRegistry, SocketEvent};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[derive(Default)]
    struct HeldSink {
        ready: bool,
        flush: bool,
        started: usize,
    }

    impl Sink<Message> for HeldSink {
        type Error = ();
        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            if self.ready {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }
        fn start_send(mut self: Pin<&mut Self>, _: Message) -> Result<(), ()> {
            self.started += 1;
            Ok(())
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            if self.flush {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }
        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
            self.poll_flush(cx)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn writes_are_bounded_before_readiness_and_during_flush() {
        for ready in [false, true] {
            let feed = FeedCache::default();
            let mut sink = HeldSink {
                ready,
                ..Default::default()
            };
            let started = tokio::time::Instant::now();
            assert!(!send(&mut sink, Message::Binary(vec![1].into()), None, &feed).await);
            assert_eq!(tokio::time::Instant::now() - started, WRITE_DEADLINE);
            assert_eq!(sink.started, usize::from(ready));
            assert_eq!(feed.socket_write_timeout_count(), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn closed_owners_never_begin_a_new_frame_even_on_a_ready_sink() {
        let registry = PeersRegistry::new();
        let (owner, _rx, _) = registry.on_peer_connected("wallet", "session");
        owner.close();
        let feed = FeedCache::default();
        let mut sink = HeldSink {
            ready: true,
            flush: true,
            started: 0,
        };
        assert!(
            !send(
                &mut sink,
                Message::Binary(vec![1].into()),
                Some(&owner),
                &feed
            )
            .await
        );
        assert_eq!(sink.started, 0);
        assert_eq!(feed.socket_write_timeout_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_kick_or_close_interrupts_a_pending_frame_without_waiting_for_the_deadline() {
        for kick in [false, true] {
            let registry = PeersRegistry::new();
            let (owner, _rx, _) = registry.on_peer_connected("wallet", "session");
            let feed = FeedCache::default();
            let mut sink = HeldSink {
                ready: true,
                ..Default::default()
            };
            let started = tokio::time::Instant::now();
            let mut write = Box::pin(send(
                &mut sink,
                Message::Binary(vec![1].into()),
                Some(&owner),
                &feed,
            ));
            assert!(futures::poll!(&mut write).is_pending());
            if kick {
                owner.send(SocketEvent::Kicked);
            } else {
                owner.close();
            }
            assert!(!write.await);
            assert_eq!(tokio::time::Instant::now(), started);
            assert_eq!(feed.socket_write_timeout_count(), 0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_current_owner_can_finish_the_write() {
        let registry = PeersRegistry::new();
        let (owner, _rx, _) = registry.on_peer_connected("wallet", "session");
        let feed = FeedCache::default();
        let mut sink = HeldSink {
            ready: true,
            flush: true,
            started: 0,
        };
        assert!(
            send(
                &mut sink,
                Message::Binary(vec![1].into()),
                Some(&owner),
                &feed
            )
            .await
        );
        assert_eq!(sink.started, 1);
        assert_eq!(feed.socket_write_timeout_count(), 0);
    }
}
