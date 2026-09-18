#![cfg(feature = "nats")]

use catalyrst_comms::nats::{
    BrokerBus, NatsBus, PublishOutcome, MAX_PUBLISH_BYTES, OUTBOUND_CAPACITY,
};
use futures::{FutureExt, StreamExt};
use std::{sync::Arc, time::Duration};

#[path = "support/nats.rs"]
mod support;
use support::Broker;

async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("broker state did not converge");
}

async fn receive(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("expected subscription delivery")
        .expect("subscription closed")
}

#[tokio::test]
async fn readiness_and_registrations_recover_after_restart_without_duplicate_handlers() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = Broker::start(&binary, None);
    let port = broker.port;
    let bus = Arc::new(BrokerBus::new(Some(broker.url()), "comms-recovery-test"));
    assert!(!bus.is_ready());
    let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
    let subscription = bus.subscribe(
        "test.assignment",
        None,
        Arc::new(move |_, payload| {
            sent.send(payload.to_vec()).unwrap();
        }),
    );
    for _ in 0..8 {
        bus.connect();
    }
    wait_for(|| bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.assignment", b"before".to_vec()).await,
        PublishOutcome::Submitted
    );
    assert_eq!(receive(&mut received).await, b"before");
    assert!(received.try_recv().is_err());
    let mut cancelled = Box::pin(bus.publish("test.assignment", b"cancelled".to_vec()));
    assert!(cancelled.as_mut().now_or_never().is_none());
    drop(cancelled);
    assert_eq!(
        bus.publish("test.assignment", b"after-cancel".to_vec())
            .await,
        PublishOutcome::Submitted
    );
    assert_eq!(
        receive(&mut received).await,
        b"after-cancel",
        "cancelled queued work must not publish"
    );
    assert_eq!(
        bus.publish("test.assignment", vec![0; MAX_PUBLISH_BYTES])
            .await,
        PublishOutcome::Failed
    );

    drop(broker);
    wait_for(|| !bus.is_connected() && !bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.assignment", b"offline".to_vec()).await,
        PublishOutcome::NoConnection
    );
    drop(subscription);
    let (sent, mut replacement) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = bus.subscribe(
        "test.assignment",
        None,
        Arc::new(move |_, payload| {
            sent.send(payload.to_vec()).unwrap();
        }),
    );
    let _broker = Broker::start(&binary, Some(port));
    wait_for(|| bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.assignment", b"after".to_vec()).await,
        PublishOutcome::Submitted
    );
    assert_eq!(receive(&mut replacement).await, b"after");
    assert!(replacement.try_recv().is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .is_none(),
        "retired delivery task survived reconnect"
    );

    let (sent, mut late) = tokio::sync::mpsc::unbounded_channel();
    let _late = bus.subscribe(
        "test.late",
        None,
        Arc::new(move |_, payload| {
            sent.send(payload.to_vec()).unwrap();
        }),
    );
    assert!(
        !bus.is_ready(),
        "dynamic registration must invalidate old readiness"
    );
    wait_for(|| bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.late", b"late".to_vec()).await,
        PublishOutcome::Submitted
    );
    assert_eq!(receive(&mut late).await, b"late");
    drop(bus);
    tokio::time::timeout(Duration::from_secs(2), async {
        assert!(late.recv().await.is_none());
        assert!(replacement.recv().await.is_none());
    })
    .await
    .expect("dropping the bus must retire its delivery tasks");
}

#[cfg(unix)]
#[tokio::test]
async fn an_idle_stalled_connection_loses_readiness_without_a_publish() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let mut broker = Broker::start(&binary, None);
    let bus = BrokerBus::new(Some(broker.url()), "comms-idle-stall-test");
    bus.connect();
    wait_for(|| bus.is_ready()).await;
    broker.signal("-STOP");
    wait_for(|| !bus.is_ready()).await;
    assert!(!bus.is_connected());
    broker.signal("-CONT");
    wait_for(|| bus.is_ready()).await;
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_tcp_is_not_reported_as_delivery_and_pending_work_is_bounded() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let mut broker = Broker::start(&binary, None);
    let bus = BrokerBus::new(Some(broker.url()), "comms-stall-test");
    let observer = async_nats::connect(broker.url()).await.unwrap();
    let mut messages = observer.subscribe("test.assignment").await.unwrap();
    support::round_trip(&observer).await;
    bus.connect();
    wait_for(|| bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.assignment", b"control".to_vec()).await,
        PublishOutcome::Submitted
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), messages.next())
            .await
            .unwrap()
            .unwrap()
            .payload
            .as_ref(),
        b"control"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    broker.signal("-STOP");
    let mut pending = Vec::new();
    let mut rejected = 0;
    for _ in 0..OUTBOUND_CAPACITY * 8 {
        let mut publish = Box::pin(bus.publish("test.assignment", b"stalled".to_vec()));
        match publish.as_mut().now_or_never() {
            None => pending.push(publish),
            Some(PublishOutcome::Failed) => rejected += 1,
            Some(outcome) => panic!("unexpected pre-deadline outcome: {outcome:?}"),
        }
    }
    assert_eq!(pending.len(), OUTBOUND_CAPACITY);
    assert_eq!(rejected, OUTBOUND_CAPACITY * 7);
    let outcomes = tokio::time::timeout(Duration::from_secs(4), futures::future::join_all(pending))
        .await
        .expect("all publishes must finish within their deadline");
    assert!(outcomes
        .iter()
        .all(|outcome| *outcome != PublishOutcome::Submitted));
    assert!(outcomes.contains(&PublishOutcome::Unconfirmed));
    assert!(!bus.is_ready());
    assert_eq!(
        bus.publish("test.assignment", b"offline".to_vec()).await,
        PublishOutcome::NoConnection
    );

    broker.signal("-CONT");
    wait_for(|| bus.is_ready()).await;
    assert_eq!(
        bus.publish("test.assignment", b"recovered".to_vec()).await,
        PublishOutcome::Submitted
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut late_submissions = 0;
        loop {
            let payload = messages.next().await.unwrap().payload;
            assert_ne!(
                payload.as_ref(),
                b"offline",
                "a rejected offline publish was replayed"
            );
            if payload.as_ref() == b"recovered" {
                break;
            }
            assert_eq!(
                payload.as_ref(),
                b"stalled",
                "only an unconfirmed in-flight publish may arrive late"
            );
            late_submissions += 1;
            assert!(
                late_submissions <= 1,
                "expired queued work was submitted after recovery"
            );
        }
    })
    .await
    .expect("recovered assignment must reach the observer");
}
