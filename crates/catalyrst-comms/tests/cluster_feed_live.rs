//! The cluster subscriber against a real broker. Set `COMMS_NATS_SERVER_BIN` to launch an isolated
//! loopback fixture, or `COMMS_NATS_TEST_URL` to a scratch broker (never the deployment's own).
//!
//! What only a live broker can show is the wire and the subject semantics the in-process fake can
//! only imitate: that a `PeerClusterChange` published by an independent client is decoded here,
//! that the reply lands on the session-addressed subject a WS Connector would be listening on, and
//! that a queue group hands one event to exactly one of two subscribers. Every decision the
//! subscriber makes is asserted against the fake bus instead, which needs no broker at all.
#![cfg(feature = "nats")]

#[path = "support/nats.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use prost::Message as _;

use catalyrst_comms::cluster_subscriber::{ClusterGateway, ClusterSubscriber, GatewayError};
use catalyrst_comms::config::ClusterConfig;
use catalyrst_comms::livekit::Removal;
use catalyrst_comms::nats::{BrokerBus, NatsBus};
use catalyrst_comms::peer_state::ClusterPeerState;
use catalyrst_pulse::decentraland::kernel::comms::v3::IslandChangedMessage;
use catalyrst_pulse::PeerClusterChange;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Long enough to absorb a loopback broker's scheduling, short enough that a genuinely missing
/// message fails the test rather than hanging the suite.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);

struct MintingGateway {
    minted: AtomicUsize,
}

#[async_trait]
impl ClusterGateway for MintingGateway {
    async fn is_denied(&self, _wallet: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }

    async fn mint_connection_string(
        &self,
        _wallet: &str,
        _session: Option<&str>,
        room: &str,
        _ttl_seconds: u64,
        _not_before_unix: Option<u64>,
    ) -> Result<String, GatewayError> {
        self.minted.fetch_add(1, Ordering::SeqCst);
        Ok(format!("livekit:wss://sfu?access_token=token-for-{room}"))
    }

    async fn remove_participant(
        &self,
        _room: &str,
        _identity: &str,
        _revoke_tokens_minted_before: Option<i64>,
    ) -> Result<Removal, GatewayError> {
        Ok(Removal::Absent)
    }

    async fn holds_participant(&self, _room: &str, _identity: &str) -> Result<bool, GatewayError> {
        Ok(false)
    }
}

fn config(url: &str, queue_group: &str) -> ClusterConfig {
    ClusterConfig {
        nats_url: Some(url.to_string()),
        enabled: true,
        queue_group: queue_group.to_string(),
        takeover_retry_delay_ms: 0,
        ..ClusterConfig::default()
    }
}

fn start(
    url: &str,
    queue_group: &str,
    gateway: Arc<MintingGateway>,
) -> (Arc<ClusterSubscriber>, Arc<dyn NatsBus>) {
    let bus: Arc<dyn NatsBus> = Arc::new(BrokerBus::new(
        Some(url.to_string()),
        "catalyrst-comms-test",
    ));
    let subscriber = ClusterSubscriber::new(
        bus.clone(),
        gateway,
        Arc::new(ClusterPeerState::default()),
        config(url, queue_group),
    );
    subscriber.start();
    (subscriber, bus)
}

/// The bus connects in the background, so the subscriptions are not live the instant `start`
/// returns; a publish before that is simply lost, core NATS having no redelivery.
async fn await_ready(buses: &[&Arc<dyn NatsBus>]) {
    let deadline = tokio::time::Instant::now() + DELIVERY_TIMEOUT;
    while !buses.iter().all(|bus| bus.is_ready()) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the subscriptions never reached the broker"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn next_payload(subscription: &mut async_nats::Subscriber) -> Vec<u8> {
    tokio::time::timeout(DELIVERY_TIMEOUT, subscription.next())
        .await
        .expect("a message must arrive within the delivery timeout")
        .expect("the subscription must stay open")
        .payload
        .to_vec()
}

/// Both phases share one test, and one broker, on purpose: `peer.*.cluster_change` is a single
/// subject space, so a second subscriber running beside the first under another queue group would
/// mint for its events too and neither phase could assert how many replies a client sees.
#[tokio::test]
async fn the_cluster_feed_reaches_a_live_broker_and_one_replica_answers_each_event() {
    let broker = std::env::var("COMMS_NATS_SERVER_BIN")
        .ok()
        .map(|binary| support::Broker::start(&binary, None));
    let url = match &broker {
        Some(broker) => broker.url(),
        None => {
            let Some(url) = catalyrst_testgate::require_env("COMMS_NATS_TEST_URL") else {
                return;
            };
            url
        }
    };

    let pulse = match async_nats::connect(&url).await {
        Ok(client) => client,
        Err(e) => panic!(
            "{}",
            catalyrst_testgate::breakage(
                "COMMS_NATS_TEST_URL",
                &format!("connect to {url} failed: {e}")
            )
        ),
    };

    let mut connector = pulse
        .subscribe(format!("engine.peer.{WALLET}.island_changed.{SESSION}"))
        .await
        .expect("subscribe as the ws connector");
    support::round_trip(&pulse).await;

    let gateway = Arc::new(MintingGateway {
        minted: AtomicUsize::new(0),
    });
    let (subscriber, bus) = start(&url, "catalyrst-comms-live-one", gateway.clone());
    await_ready(&[&bus]).await;

    let change = PeerClusterChange {
        cluster_id: "C1".into(),
        realm: "main".into(),
        session: SESSION.into(),
        displaced_session: String::new(),
        displaced_cluster_id: String::new(),
    };
    pulse
        .publish(
            format!("peer.{WALLET}.cluster_change"),
            change.encode_to_vec().into(),
        )
        .await
        .expect("publish the cluster change");

    let island = IslandChangedMessage::decode(next_payload(&mut connector).await.as_slice())
        .expect("the reply decodes as IslandChangedMessage");
    assert_eq!(island.island_id, "island-C1");
    assert_eq!(island.from_island_id, None);
    assert!(island.peers.is_empty());
    assert!(island.conn_str.contains("token-for-island-C1"));
    assert_eq!(gateway.minted.load(Ordering::SeqCst), 1);

    subscriber.stop().await;

    two_replicas_mint_once_between_them(&url, &pulse).await;
}

/// Without the queue group two replicas would each mint and publish, handing one client two
/// `island_changed` messages carrying two different tokens.
async fn two_replicas_mint_once_between_them(url: &str, pulse: &async_nats::Client) {
    let wallet = "0x2222222222222222222222222222222222222222";
    let mut connector = pulse
        .subscribe(format!("engine.peer.{wallet}.island_changed.{SESSION}"))
        .await
        .expect("subscribe as the ws connector");
    support::round_trip(pulse).await;

    let first = Arc::new(MintingGateway {
        minted: AtomicUsize::new(0),
    });
    let second = Arc::new(MintingGateway {
        minted: AtomicUsize::new(0),
    });
    let (one, one_bus) = start(url, "catalyrst-comms-live-two", first.clone());
    let (two, two_bus) = start(url, "catalyrst-comms-live-two", second.clone());
    await_ready(&[&one_bus, &two_bus]).await;

    let change = PeerClusterChange {
        cluster_id: "C2".into(),
        realm: "main".into(),
        session: SESSION.into(),
        displaced_session: String::new(),
        displaced_cluster_id: String::new(),
    };
    pulse
        .publish(
            format!("peer.{wallet}.cluster_change"),
            change.encode_to_vec().into(),
        )
        .await
        .expect("publish the cluster change");

    let island = IslandChangedMessage::decode(next_payload(&mut connector).await.as_slice())
        .expect("the reply decodes as IslandChangedMessage");
    assert_eq!(island.island_id, "island-C2");

    assert!(
        tokio::time::timeout(Duration::from_millis(500), connector.next())
            .await
            .is_err(),
        "exactly one member of the queue group may answer an event"
    );
    assert_eq!(
        first.minted.load(Ordering::SeqCst) + second.minted.load(Ordering::SeqCst),
        1
    );

    one.stop().await;
    two.stop().await;
}
