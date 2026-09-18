#![cfg(feature = "nats")]

use catalyrst_comms::nats::{
    BrokerBus, NatsBus, PublishOutcome, RequestError, MAX_PENDING_REQUESTS, MAX_PUBLISH_BYTES,
};
use futures::{FutureExt, StreamExt};
use std::{sync::Arc, time::Duration};

#[path = "support/nats.rs"]
mod support;

async fn ready(bus: &BrokerBus) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !bus.is_ready() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

async fn next(subscriber: &mut async_nats::Subscriber) -> async_nats::Message {
    tokio::time::timeout(Duration::from_secs(5), subscriber.next())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn replies_are_correlated_and_late_or_retired_replies_cannot_complete_a_new_request() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = support::Broker::start(&binary, None);
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut queries = client.subscribe("test.lookup").await.unwrap();
    support::round_trip(&client).await;
    let bus = Arc::new(BrokerBus::new(Some(broker.url()), "comms-request-test"));
    bus.connect();
    ready(&bus).await;
    assert_eq!(
        bus.request("test.missing", vec![]).await,
        Err(RequestError::NoResponders)
    );
    assert_eq!(
        bus.request("test.lookup", vec![0; MAX_PUBLISH_BYTES]).await,
        Err(RequestError::Invalid)
    );
    let a = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"a".to_vec()).await })
    };
    let b = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"b".to_vec()).await })
    };
    let first = next(&mut queries).await;
    let second = next(&mut queries).await;
    assert_ne!(first.reply, second.reply);
    client
        .publish(second.reply.clone().unwrap(), second.payload.clone())
        .await
        .unwrap();
    client
        .publish(first.reply.clone().unwrap(), first.payload.clone())
        .await
        .unwrap();
    assert_eq!(a.await.unwrap(), Ok(b"a".to_vec()));
    assert_eq!(b.await.unwrap(), Ok(b"b".to_vec()));

    let silent = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"silent".to_vec()).await })
    };
    let expired = next(&mut queries).await;
    assert_eq!(silent.await.unwrap(), Err(RequestError::TimedOut));
    assert!(bus.is_ready(), "source silence is not a broken broker");
    let fresh = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"fresh".to_vec()).await })
    };
    let current = next(&mut queries).await;
    client
        .publish(expired.reply.unwrap(), "expired".into())
        .await
        .unwrap();
    client
        .publish(first.reply.unwrap(), "duplicate".into())
        .await
        .unwrap();
    client
        .publish(current.reply.unwrap(), "current".into())
        .await
        .unwrap();
    assert_eq!(fresh.await.unwrap(), Ok(b"current".to_vec()));

    let retired = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"retired".to_vec()).await })
    };
    let obsolete = next(&mut queries).await;
    let _registration = bus.subscribe("test.new_registration", None, Arc::new(|_, _| {}));
    ready(&bus).await;
    client
        .publish(obsolete.reply.unwrap(), "wrong-revision".into())
        .await
        .unwrap();
    assert_eq!(retired.await.unwrap(), Err(RequestError::NoConnection));

    let mut cancelled = Box::pin(bus.request("test.lookup", b"cancelled".to_vec()));
    assert!(cancelled.as_mut().now_or_never().is_none());
    drop(cancelled);
    let valid = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", b"valid".to_vec()).await })
    };
    let query = next(&mut queries).await;
    assert_eq!(query.payload.as_ref(), b"valid");
    client
        .publish(query.reply.unwrap(), vec![0; MAX_PUBLISH_BYTES + 1].into())
        .await
        .unwrap();
    assert_eq!(valid.await.unwrap(), Err(RequestError::Invalid));
}

#[tokio::test]
async fn unanswered_requests_are_bounded_without_blocking_publication_and_slots_are_reclaimed() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let broker = support::Broker::start(&binary, None);
    let client = async_nats::connect(broker.url()).await.unwrap();
    let mut queries = client.subscribe("test.lookup").await.unwrap();
    support::round_trip(&client).await;
    let bus = Arc::new(BrokerBus::new(Some(broker.url()), "comms-request-bound"));
    bus.connect();
    ready(&bus).await;
    let mut requests = Vec::new();
    for _ in 0..MAX_PENDING_REQUESTS {
        let bus = bus.clone();
        requests.push(tokio::spawn(async move {
            bus.request("test.lookup", vec![]).await
        }));
        next(&mut queries).await;
    }
    assert_eq!(
        bus.request("test.lookup", vec![]).await,
        Err(RequestError::Overloaded)
    );
    assert_eq!(
        bus.publish("test.still-publishing", vec![]).await,
        PublishOutcome::Submitted
    );
    for request in requests {
        assert_eq!(request.await.unwrap(), Err(RequestError::TimedOut));
    }
    let usable = {
        let bus = bus.clone();
        tokio::spawn(async move { bus.request("test.lookup", vec![]).await })
    };
    let query = next(&mut queries).await;
    client
        .publish(query.reply.unwrap(), "reclaimed".into())
        .await
        .unwrap();
    assert_eq!(usable.await.unwrap(), Ok(b"reclaimed".to_vec()));
    assert!(bus.is_ready());
}
