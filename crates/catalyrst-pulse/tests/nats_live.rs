//! The cluster feed against a real broker. Env-gated: set `PULSE_NATS_TEST_URL` to a scratch
//! `nats-server` (never the deployment's own).
//!
//! What only a live broker can show is the wire: that a subscriber on `peer.{wallet}.cluster_change`
//! receives bytes that decode as the vendored `PeerClusterChange`, and that the topology and
//! discovery subjects carry archipelago's shapes. The tracker's publish decisions are asserted
//! against the in-process fake instead, which needs no broker at all.
#![cfg(feature = "nats")]

use std::sync::Arc;
use std::time::Duration;

use catalyrst_pulse::cluster::feed::{ClusterFeedPublisher, DISCOVERY_SUBJECT, ISLANDS_SUBJECT};
use catalyrst_pulse::cluster::nats::{NatsClusterFeed, NatsFeedOptions};
use catalyrst_pulse::cluster::{ClusterInfo, ClusterPass, ClusterPeerInfo, ClusterSession};
use catalyrst_pulse::decentraland::common::Vector3;
use catalyrst_pulse::decentraland::kernel::comms::v3::{
    IslandStatusMessage, ServiceDiscoveryMessage,
};
use catalyrst_pulse::PeerClusterChange;
use futures_util::StreamExt;
use prost::Message as _;

const WALLET: &str = "0xAbCdEf0000000000000000000000000000000001";
const SUBJECT: &str = "peer.0xabcdef0000000000000000000000000000000001.cluster_change";

/// Long enough to absorb a loopback broker's scheduling, short enough that a genuinely missing
/// message fails the test rather than hanging the suite.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);

fn pass() -> ClusterPass {
    let mut pass = ClusterPass::default();
    pass.clusters.push(ClusterInfo {
        id: "C1".into(),
        realm: "test-realm".into(),
        count: 1,
        centroid: Vector3 {
            x: 4.0,
            y: 0.0,
            z: 8.0,
        },
        radius: 2.5,
    });
    pass.peers.push(ClusterPeerInfo {
        peer: 0,
        wallet: WALLET.to_string(),
        cluster_id: "C1".into(),
        realm: "test-realm".into(),
        position: Vector3 {
            x: 4.0,
            y: 0.0,
            z: 8.0,
        },
        parcel: 0,
    });
    pass
}

async fn next_payload(sub: &mut async_nats::Subscriber) -> Vec<u8> {
    tokio::time::timeout(DELIVERY_TIMEOUT, sub.next())
        .await
        .expect("a message must arrive within the delivery timeout")
        .expect("the subscription must stay open")
        .payload
        .to_vec()
}

#[tokio::test]
async fn the_cluster_feed_reaches_a_live_broker_on_every_subject() {
    let Some(url) = catalyrst_testgate::require_env("PULSE_NATS_TEST_URL") else {
        return;
    };

    let client = match async_nats::connect(&url).await {
        Ok(c) => c,
        Err(e) => panic!(
            "{}",
            catalyrst_testgate::breakage(
                "PULSE_NATS_TEST_URL",
                &format!("connect to {url} failed: {e}")
            )
        ),
    };
    let mut changes = client.subscribe(SUBJECT).await.expect("subscribe changes");
    let mut islands = client
        .subscribe(ISLANDS_SUBJECT)
        .await
        .expect("subscribe islands");
    let mut discovery = client
        .subscribe(DISCOVERY_SUBJECT)
        .await
        .expect("subscribe discovery");
    client.flush().await.expect("subscriptions registered");

    let feed = NatsClusterFeed::spawn(NatsFeedOptions {
        url: url.clone(),
        server_name: "pulse-test".into(),
        discovery_interval_ms: 200,
        ..Default::default()
    });

    let pass = pass();
    feed.publish_topology(&pass, 3);
    feed.publish_cluster_change(
        WALLET,
        "C1",
        "test-realm",
        &ClusterSession {
            session: "0xsession-two".into(),
            displaced_session: Some("0xsession-one".into()),
            displaced_cluster_id: Some("C0".into()),
        },
    );

    let change = PeerClusterChange::decode(next_payload(&mut changes).await.as_slice())
        .expect("the change decodes as PeerClusterChange");
    assert_eq!(change.cluster_id, "C1");
    assert_eq!(change.realm, "test-realm");
    assert_eq!(change.session, "0xsession-two");
    assert_eq!(change.displaced_session, "0xsession-one");
    assert_eq!(change.displaced_cluster_id, "C0");

    let topology = IslandStatusMessage::decode(next_payload(&mut islands).await.as_slice())
        .expect("the topology decodes as IslandStatusMessage");
    assert_eq!(topology.data.len(), 1);
    assert_eq!(topology.data[0].id, "C1");
    assert_eq!(topology.data[0].peers, vec![WALLET.to_string()]);
    assert_eq!(
        topology.data[0].max_peers, 0,
        "Pulse caps cluster size nowhere"
    );

    let heartbeat = ServiceDiscoveryMessage::decode(next_payload(&mut discovery).await.as_slice())
        .expect("the heartbeat decodes as ServiceDiscoveryMessage");
    assert_eq!(heartbeat.server_name, "pulse-test");
    let status = heartbeat.status.expect("the heartbeat carries a status");
    assert_eq!(
        status.user_count, 3,
        "the heartbeat advertises active peers, not clustered peers"
    );
    assert_eq!(
        status.commit_hash.as_deref(),
        Some(catalyrst_pulse::cluster::nats::DEFAULT_COMMIT_HASH),
        "an options set that never named a commit still announces upstream's fallback, never a \
         blank string a discovery consumer would have to special-case"
    );
    assert!(status.current_time > 0);
}

#[tokio::test]
async fn a_broker_that_is_down_costs_the_publisher_nothing() {
    let feed = NatsClusterFeed::spawn(NatsFeedOptions {
        url: "nats://127.0.0.1:1".into(),
        ..Default::default()
    });
    let publisher: Arc<dyn ClusterFeedPublisher> = Arc::new(feed);
    let pass = pass();

    let started = std::time::Instant::now();
    for _ in 0..1000 {
        publisher.publish_cluster_change(
            WALLET,
            "C1",
            "test-realm",
            &ClusterSession::new("s".into()),
        );
        publisher.publish_topology(&pass, 1);
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "publishing must never wait on a broker, took {:?}",
        started.elapsed()
    );
}
