use super::SocketEvent;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{mpsc::error::TryRecvError, Notify};
use tokio_util::sync::CancellationToken;

pub const MAX_ASSIGNMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AssignmentSend {
    Queued,
    Coalesced,
    Closed,
    TooLarge,
}

#[derive(Default)]
struct Pending {
    assignment: Option<Box<[u8]>>,
    kicked: bool,
}

#[derive(Default)]
struct Mailbox {
    pending: Mutex<Pending>,
    changed: Notify,
    closed: CancellationToken,
}

impl Mailbox {
    fn close(&self) {
        let mut pending = self.pending.lock();
        pending.assignment = None;
        self.closed.cancel();
        drop(pending);
        self.changed.notify_one();
    }
}

pub(super) struct MailboxSender(Arc<Mailbox>);

impl MailboxSender {
    pub(super) fn channel() -> (Arc<Self>, SocketEvents) {
        let shared = Arc::new(Mailbox::default());
        (Arc::new(Self(shared.clone())), SocketEvents(shared))
    }

    pub(super) fn assignment(&self, bytes: &[u8]) -> AssignmentSend {
        if bytes.len() > MAX_ASSIGNMENT_BYTES {
            return AssignmentSend::TooLarge;
        }
        let mut pending = self.0.pending.lock();
        if self.is_closed() {
            return AssignmentSend::Closed;
        }
        let coalesced = pending.assignment.replace(bytes.into()).is_some();
        drop(pending);
        self.0.changed.notify_one();
        if coalesced {
            AssignmentSend::Coalesced
        } else {
            AssignmentSend::Queued
        }
    }

    pub(super) fn kick(&self) -> bool {
        let mut pending = self.0.pending.lock();
        if self.is_closed() {
            return false;
        }
        pending.kicked = true;
        pending.assignment = None;
        self.0.closed.cancel();
        drop(pending);
        self.0.changed.notify_one();
        true
    }

    pub(super) fn is_closed(&self) -> bool {
        self.0.closed.is_cancelled()
    }

    pub(super) fn close(&self) {
        self.0.close();
    }

    pub(super) async fn cancelled(&self) {
        self.0.closed.cancelled().await;
    }
}

impl Drop for MailboxSender {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// One pending absolute assignment and a terminal kick, never a historical FIFO.
/// Taking an event is cancellation-safe: no state is consumed until a poll returns it.
pub struct SocketEvents(Arc<Mailbox>);

impl SocketEvents {
    pub fn try_recv(&mut self) -> Result<SocketEvent, TryRecvError> {
        let mut pending = self.0.pending.lock();
        if std::mem::take(&mut pending.kicked) {
            return Ok(SocketEvent::Kicked);
        }
        if self.0.closed.is_cancelled() {
            return Err(TryRecvError::Disconnected);
        }
        match pending.assignment.take() {
            Some(bytes) => Ok(SocketEvent::IslandChanged(bytes.into_vec())),
            None => Err(TryRecvError::Empty),
        }
    }

    pub async fn recv(&mut self) -> Option<SocketEvent> {
        loop {
            let shared = self.0.clone();
            let changed = shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            match self.try_recv() {
                Ok(event) => return Some(event),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => changed.await,
            }
        }
    }
}

impl Drop for SocketEvents {
    fn drop(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_storage_is_one_sized_payload_after_arbitrary_pressure() {
        let (send, mut receive) = MailboxSender::channel();
        for value in 0..10_000_u64 {
            let mut bytes = Vec::with_capacity(MAX_ASSIGNMENT_BYTES * 2);
            bytes.extend_from_slice(&value.to_le_bytes());
            assert_eq!(
                send.assignment(&bytes),
                if value == 0 {
                    AssignmentSend::Queued
                } else {
                    AssignmentSend::Coalesced
                }
            );
            assert_eq!(send.0.pending.lock().assignment.as_ref().unwrap().len(), 8);
        }
        let SocketEvent::IslandChanged(bytes) = receive.try_recv().unwrap() else {
            panic!("expected assignment");
        };
        assert_eq!(bytes, 9_999_u64.to_le_bytes());
        assert_eq!(
            bytes.capacity(),
            8,
            "do not retain the producer's spare allocation"
        );
        assert!(matches!(receive.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn byte_limit_rejects_without_replacing_a_valid_pending_assignment() {
        let (send, mut receive) = MailboxSender::channel();
        assert_eq!(
            send.assignment(&vec![1; MAX_ASSIGNMENT_BYTES]),
            AssignmentSend::Queued
        );
        assert_eq!(
            send.assignment(&vec![2; MAX_ASSIGNMENT_BYTES + 1]),
            AssignmentSend::TooLarge
        );
        assert!(
            matches!(receive.try_recv(), Ok(SocketEvent::IslandChanged(bytes)) if bytes == vec![1; MAX_ASSIGNMENT_BYTES])
        );
    }

    #[tokio::test]
    async fn kick_is_terminal_discards_backlog_and_wakes_the_writer() {
        let (send, mut receive) = MailboxSender::channel();
        send.assignment(b"old");
        assert!(send.kick());
        send.close();
        assert!(send.is_closed());
        assert!(!send.kick());
        assert_eq!(send.assignment(b"late"), AssignmentSend::Closed);
        send.cancelled().await;
        assert!(matches!(receive.recv().await, Some(SocketEvent::Kicked)));
        assert!(receive.recv().await.is_none());
        assert!(send.0.pending.lock().assignment.is_none());
    }

    #[tokio::test]
    async fn close_drop_and_receiver_loss_release_pending_bytes() {
        let (send, mut receive) = MailboxSender::channel();
        send.assignment(b"old");
        send.close();
        assert!(receive.recv().await.is_none());
        assert_eq!(send.assignment(b"late"), AssignmentSend::Closed);
        let (send, mut receive) = MailboxSender::channel();
        let clone = send.clone();
        drop(send);
        assert!(matches!(receive.try_recv(), Err(TryRecvError::Empty)));
        drop(clone);
        assert!(receive.recv().await.is_none());
        let (send, receive) = MailboxSender::channel();
        send.assignment(b"old");
        drop(receive);
        assert!(send.is_closed());
        assert!(send.0.pending.lock().assignment.is_none());
        assert_eq!(send.assignment(b"late"), AssignmentSend::Closed);
    }

    #[tokio::test]
    async fn cancelling_a_pending_receive_does_not_lose_the_next_notification() {
        let (send, mut receive) = MailboxSender::channel();
        {
            let mut waiting = Box::pin(receive.recv());
            assert!(futures::poll!(&mut waiting).is_pending());
        }
        assert_eq!(send.assignment(b"next"), AssignmentSend::Queued);
        assert!(
            matches!(receive.recv().await, Some(SocketEvent::IslandChanged(bytes)) if bytes == b"next")
        );
        {
            let mut waiting = Box::pin(receive.recv());
            assert!(futures::poll!(&mut waiting).is_pending());
            assert!(send.kick());
            assert!(matches!(waiting.await, Some(SocketEvent::Kicked)));
        }
    }
}
