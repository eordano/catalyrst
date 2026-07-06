use crate::session::Scope;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

pub fn subject_actions(scope: Scope) -> String {
    format!("fed.{}.actions", scope.as_str())
}

pub fn subject_snapshots(scope: Scope) -> String {
    format!("fed.{}.snapshots", scope.as_str())
}

pub fn account_name() -> &'static str {
    "federation"
}

pub fn stream_name(scope: Scope) -> String {
    format!("FED_{}", scope.as_str().to_ascii_uppercase())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    #[serde(default = "default_env_version")]
    pub version: u32,

    pub scope: Scope,

    pub primary_type: String,

    pub signature_hash: String,

    pub signer: String,

    pub signed_at: i64,

    pub signed_json: serde_json::Value,

    #[serde(default)]
    pub origin_peer: Option<String>,
}

fn default_env_version() -> u32 {
    1
}

impl GossipEnvelope {
    pub fn local<T>(
        scope: Scope,
        signed: &crate::sig::Signed<T>,
        signature_hash: String,
        signer: String,
    ) -> Result<Self, crate::error::FedError>
    where
        T: crate::sig::TypedMessage + Serialize,
    {
        let signed_json = serde_json::to_value(signed)
            .map_err(|e| crate::error::FedError::Malformed(e.to_string()))?;
        Ok(Self {
            version: default_env_version(),
            scope,
            primary_type: T::PRIMARY_TYPE.to_string(),
            signature_hash,
            signer,
            signed_at: signed.signed_at,
            signed_json,
            origin_peer: None,
        })
    }

    pub fn subject(&self) -> String {
        subject_actions(self.scope)
    }

    pub fn encode(&self) -> Result<Vec<u8>, crate::error::FedError> {
        serde_json::to_vec(self).map_err(|e| crate::error::FedError::Malformed(e.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, crate::error::FedError> {
        serde_json::from_slice(bytes).map_err(|e| crate::error::FedError::Malformed(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub enum GossipConfig {
    Disabled,

    Nats { url: String, peer_id: String },
}

impl GossipConfig {
    pub fn from_env() -> Self {
        match std::env::var("FED_GOSSIP").ok().as_deref() {
            Some("nats") => GossipConfig::Nats {
                url: std::env::var("FED_NATS_URL")
                    .unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string()),
                peer_id: std::env::var("FED_PEER_ID").unwrap_or_else(|_| "local".to_string()),
            },
            _ => GossipConfig::Disabled,
        }
    }
}

#[async_trait::async_trait]
pub trait GossipPublisher: Send + Sync {
    async fn publish(&self, env: &GossipEnvelope) -> Result<(), crate::error::FedError>;

    async fn subscribe(
        &self,
        _scope: Scope,
    ) -> Result<Option<mpsc::Receiver<GossipEnvelope>>, crate::error::FedError> {
        Ok(None)
    }

    fn is_live(&self) -> bool {
        true
    }
}

#[derive(Debug, Default, Clone)]
pub struct NoopPublisher;

#[async_trait::async_trait]
impl GossipPublisher for NoopPublisher {
    async fn publish(&self, env: &GossipEnvelope) -> Result<(), crate::error::FedError> {
        tracing::trace!(
            scope = env.scope.as_str(),
            primary_type = %env.primary_type,
            signature_hash = %env.signature_hash,
            "gossip disabled; action applied locally, not published (peers reconcile via snapshot pull)"
        );
        Ok(())
    }
    fn is_live(&self) -> bool {
        false
    }
}

pub async fn build_publisher(cfg: &GossipConfig) -> Arc<dyn GossipPublisher> {
    match cfg {
        GossipConfig::Disabled => Arc::new(NoopPublisher),
        GossipConfig::Nats { url, peer_id } => {
            #[cfg(feature = "nats")]
            {
                match nats::NatsPublisher::connect(url, peer_id.clone()).await {
                    Ok(p) => Arc::new(p),
                    Err(e) => {
                        tracing::error!(error = %e, url = %url, "NATS gossip connect failed; falling back to noop (snapshot-pull only)");
                        Arc::new(NoopPublisher)
                    }
                }
            }
            #[cfg(not(feature = "nats"))]
            {
                let _ = (url, peer_id);
                tracing::warn!("FED_GOSSIP=nats but catalyrst-fed built without the `nats` feature; using noop (snapshot-pull only)");
                Arc::new(NoopPublisher)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Scope;
    use crate::sig::{Eip712Domain, Signed, TypedMessage};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Dummy {
        place_id: String,
    }
    impl TypedMessage for Dummy {
        const PRIMARY_TYPE: &'static str = "Dummy";
        fn encode_struct(&self) -> Vec<u8> {
            self.place_id.as_bytes().to_vec()
        }
    }

    fn signed() -> Signed<Dummy> {
        Signed {
            domain: Eip712Domain {
                name: "DecentralandPlaces".into(),
                version: "1".into(),
                chain_id: 137,
                verifying_contract: "0x0".into(),
            },
            message: Dummy {
                place_id: "abc".into(),
            },
            nonce: [7u8; 16],
            signed_at: 1000,
            signature: "0x".to_string() + &"00".repeat(65),
        }
    }

    #[test]
    fn envelope_roundtrips_and_carries_routing() {
        let s = signed();
        let env =
            GossipEnvelope::local(Scope::Places, &s, "deadbeef".into(), "0xabc".into()).unwrap();
        assert_eq!(env.primary_type, "Dummy");
        assert_eq!(env.scope, Scope::Places);
        assert_eq!(env.signed_at, 1000);
        assert_eq!(env.subject(), "fed.places.actions");
        assert!(env.origin_peer.is_none());

        let bytes = env.encode().unwrap();
        let back = GossipEnvelope::decode(&bytes).unwrap();
        assert_eq!(back.signature_hash, "deadbeef");

        let inner: Signed<Dummy> = serde_json::from_value(back.signed_json).unwrap();
        assert_eq!(inner.message.place_id, "abc");
    }

    #[tokio::test]
    async fn noop_publisher_is_not_live_and_succeeds() {
        let p = NoopPublisher;
        assert!(!p.is_live());
        let env =
            GossipEnvelope::local(Scope::Communities, &signed(), "h".into(), "0x1".into()).unwrap();
        assert!(p.publish(&env).await.is_ok());
    }

    #[test]
    fn config_from_env_defaults_disabled() {
        std::env::remove_var("FED_GOSSIP");
        assert!(matches!(GossipConfig::from_env(), GossipConfig::Disabled));
    }
}

#[cfg(feature = "nats")]
pub mod nats {

    use super::{stream_name, subject_actions, GossipEnvelope, GossipPublisher};
    use crate::error::FedError;
    use crate::session::Scope;
    use async_nats::jetstream;
    use tokio::sync::mpsc;

    pub struct NatsPublisher {
        js: jetstream::Context,
        peer_id: String,
    }

    impl NatsPublisher {
        pub async fn connect(url: &str, peer_id: String) -> Result<Self, FedError> {
            let cert = std::env::var("FED_NATS_CLIENT_CERT").ok();
            let key = std::env::var("FED_NATS_CLIENT_KEY").ok();
            let ca = std::env::var("FED_NATS_ROOT_CA").ok();

            let client = if cert.is_some() || key.is_some() || ca.is_some() {
                let mut opts = async_nats::ConnectOptions::new().require_tls(true);
                match (cert, key) {
                    (Some(c), Some(k)) => {
                        opts = opts.add_client_certificate(c.into(), k.into());
                    }
                    (Some(_), None) | (None, Some(_)) => {
                        return Err(FedError::Transport(
                            "mTLS misconfigured: FED_NATS_CLIENT_CERT and \
                             FED_NATS_CLIENT_KEY must both be set"
                                .into(),
                        ));
                    }
                    (None, None) => {}
                }
                if let Some(ca) = ca {
                    opts = opts.add_root_certificates(ca.into());
                }
                tracing::info!(url = %url, "connecting to federation NATS over mTLS");
                opts.connect(url)
                    .await
                    .map_err(|e| FedError::Transport(format!("nats mTLS connect {url}: {e}")))?
            } else {
                tracing::warn!(
                    url = %url,
                    "connecting to federation NATS WITHOUT mTLS — single-broker dev deploy only; \
                     set FED_NATS_CLIENT_CERT/KEY + FED_NATS_ROOT_CA before peering remotely"
                );
                async_nats::connect(url)
                    .await
                    .map_err(|e| FedError::Transport(format!("nats connect {url}: {e}")))?
            };
            let js = jetstream::new(client);
            Ok(Self { js, peer_id })
        }

        pub async fn ensure_stream(&self, scope: Scope) -> Result<(), FedError> {
            self.js
                .get_or_create_stream(jetstream::stream::Config {
                    name: stream_name(scope),
                    subjects: vec![subject_actions(scope)],
                    max_age: std::time::Duration::from_secs(30 * 24 * 3600),
                    ..Default::default()
                })
                .await
                .map_err(|e| FedError::Transport(format!("ensure_stream: {e}")))?;
            Ok(())
        }

        pub async fn consume(
            &self,
            scope: Scope,
            tx: mpsc::Sender<GossipEnvelope>,
        ) -> Result<(), FedError> {
            self.ensure_stream(scope).await?;
            let stream = self
                .js
                .get_stream(stream_name(scope))
                .await
                .map_err(|e| FedError::Transport(format!("get_stream: {e}")))?;
            let consumer = stream
                .create_consumer(jetstream::consumer::pull::Config {
                    durable_name: Some(format!("{}-{}", self.peer_id, scope.as_str())),
                    ..Default::default()
                })
                .await
                .map_err(|e| FedError::Transport(format!("create_consumer: {e}")))?;
            let peer_id = self.peer_id.clone();
            tokio::spawn(async move {
                use futures_util::StreamExt;
                let mut messages = match consumer.messages().await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!(error = %e, "jetstream consumer.messages failed");
                        return;
                    }
                };
                while let Some(item) = messages.next().await {
                    let msg = match item {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!(error = %e, "jetstream message error");
                            continue;
                        }
                    };
                    match GossipEnvelope::decode(&msg.payload) {
                        Ok(env) => {
                            if env.origin_peer.as_deref() == Some(peer_id.as_str()) {
                                let _ = msg.ack().await;
                                continue;
                            }
                            if tx.send(env).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "undecodable gossip envelope"),
                    }
                    let _ = msg.ack().await;
                }
            });
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl GossipPublisher for NatsPublisher {
        async fn subscribe(
            &self,
            scope: Scope,
        ) -> Result<Option<mpsc::Receiver<GossipEnvelope>>, FedError> {
            let (tx, rx) = mpsc::channel(1024);
            self.consume(scope, tx).await?;
            Ok(Some(rx))
        }

        async fn publish(&self, env: &GossipEnvelope) -> Result<(), FedError> {
            let mut env = env.clone();
            if env.origin_peer.is_none() {
                env.origin_peer = Some(self.peer_id.clone());
            }
            let payload = env.encode()?;
            self.js
                .publish(subject_actions(env.scope), payload.into())
                .await
                .map_err(|e| FedError::Transport(format!("nats publish: {e}")))?
                .await
                .map_err(|e| FedError::Transport(format!("nats publish-ack: {e}")))?;
            Ok(())
        }
    }
}
