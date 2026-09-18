use futures::StreamExt;
use std::time::Duration;

pub async fn round_trip(client: &async_nats::Client) {
    tokio::time::timeout(Duration::from_secs(3), async {
        let inbox = client.new_inbox();
        let mut replies = client.subscribe(inbox.clone()).await.unwrap();
        client.publish(inbox, "fixture-ready".into()).await.unwrap();
        assert_eq!(
            replies.next().await.unwrap().payload.as_ref(),
            b"fixture-ready"
        );
    })
    .await
    .expect("fixture broker round trip failed");
}

pub use catalyrst_testgate::nats::Broker;
