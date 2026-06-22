//! Federation gossip transport.
//!
//! Design notes:
//! - NATS is the federation transport, on an account-isolated, mTLS broker.
//!   Subjects are namespaced `fed.<scope>.*`.
//! - Places opinions gossip over a single JetStream subject (normalised to the
//!   `fed.<scope>.actions` convention so every service shares one wire shape).
//! - Communities deliberately federate via HTTP-snapshot pull, NOT live gossip.
//!   The HTTP `snapshot` / `changes` endpoints already implement that.
//!   Communities therefore use the [`NoopPublisher`] (local-authoritative writes
//!   apply + are durably logged in Postgres; remote peers pull the log). The
//!   gossip path below is what places (and, later, friends/messaging) use for
//!   sub-second push.
//!
//! Design: a [`GossipPublisher`] trait abstracts the transport so the rest of
//! the workspace never touches NATS directly. Two impls:
//!
//!  * [`NoopPublisher`] — the single-node / no-peers default. `publish` is a
//!    no-op that returns `Ok`; `consume` never yields. A deploy with no peers
//!    works unchanged: local writes apply immediately and are published into the
//!    void.
//!  * [`nats::NatsPublisher`] — real JetStream publish/subscribe, gated behind
//!    the `nats` cargo feature so the workspace still builds where `async-nats`
//!    is unavailable. Selected at runtime via [`GossipConfig`].
//!
//! Every gossiped action is wrapped in a [`GossipEnvelope`]: the original
//! `Signed<T>` re-serialised as JSON (the canonical signed wire format), plus
//! routing metadata (scope, primary type,
//! signature hash, recovered signer, origin peer). A receiver re-verifies the
//! inner `Signed<T>` through the same `verify` / replay / authority machinery a
//! local write goes through — gossip is never trusted just because a peer
//! forwarded it.

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

/// JetStream stream name per scope (durable, Postgres remains source of truth).
pub fn stream_name(scope: Scope) -> String {
    format!("FED_{}", scope.as_str().to_ascii_uppercase())
}

/// The envelope carried on the wire for one federated action.
///
/// `signed_json` is the verbatim JSON of the originating `Signed<T>` so a
/// receiver can deserialise it into the concrete per-service message type and
/// re-run `Signed::verify`. We do NOT transmit the decoded payload separately —
/// the signed bytes are the only thing a peer should trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    /// envelope schema version (forward-compat).
    #[serde(default = "default_env_version")]
    pub version: u32,
    /// service namespace (places / communities / ...).
    pub scope: Scope,
    /// `T::PRIMARY_TYPE` of the inner action, so the receiver knows which
    /// concrete type to deserialise `signed_json` into.
    pub primary_type: String,
    /// canonical content id = `Signed::hash()` hex; receivers dedup on this.
    pub signature_hash: String,
    /// wallet address recovered from the signature (post session-delegation).
    pub signer: String,
    /// `signed_at` of the inner action (unix seconds) — lets a receiver apply
    /// the configured clock-skew window before deserialising the full body.
    pub signed_at: i64,
    /// the originating `Signed<T>` re-serialised as JSON.
    pub signed_json: serde_json::Value,
    /// peer_id this envelope was first published by. `None` == produced by the
    /// local catalyst (set by the publisher); set to the source peer on ingest.
    #[serde(default)]
    pub origin_peer: Option<String>,
}

fn default_env_version() -> u32 {
    1
}

impl GossipEnvelope {
    /// Build an envelope for a locally-produced signed action.
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

/// Runtime selection of the gossip transport.
#[derive(Debug, Clone)]
pub enum GossipConfig {
    /// No live gossip. Local writes still apply + persist; peers reconcile via
    /// HTTP-snapshot pull. This is the single-node default and the communities
    /// transport.
    Disabled,
    /// NATS JetStream (requires the `nats` cargo feature). `url` is the broker
    /// (e.g. `nats://<HOST>:4222`). `peer_id` stamps the
    /// origin on outbound envelopes.
    Nats { url: String, peer_id: String },
}

impl GossipConfig {
    /// Parse from env: `FED_GOSSIP=nats|disabled`, `FED_NATS_URL`, `FED_PEER_ID`.
    /// Defaults to `Disabled` so an operator must opt in to live gossip.
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

/// Transport abstraction. `publish` emits a locally-applied action to peers;
/// `consume` yields remote envelopes for the service to re-verify and apply.
#[async_trait::async_trait]
pub trait GossipPublisher: Send + Sync {
    /// Publish an envelope for an action that has already been applied locally.
    /// MUST be best-effort: a transport failure does not roll back the local
    /// write (the action is durable in Postgres and recoverable via snapshot
    /// pull), it is logged and surfaced as `Err` for metrics.
    async fn publish(&self, env: &GossipEnvelope) -> Result<(), crate::error::FedError>;

    /// Subscribe to remote actions for `scope`. Returns a receiver that yields
    /// every [`GossipEnvelope`] a peer publishes on `fed.<scope>.actions`,
    /// EXCEPT envelopes this peer itself originated (those are filtered by the
    /// transport to avoid apply loops). The caller drives
    /// a loop that re-verifies + applies each envelope through the same
    /// verify/replay/authority machinery a local write goes through (gossip is
    /// never trusted because a peer forwarded it).
    ///
    /// Returns `None` for transports that never reach peers (the no-op /
    /// snapshot-pull deploy), so callers can skip spawning an apply loop.
    async fn subscribe(
        &self,
        _scope: Scope,
    ) -> Result<Option<mpsc::Receiver<GossipEnvelope>>, crate::error::FedError> {
        Ok(None)
    }

    /// Whether this publisher actually reaches peers (false for the no-op).
    fn is_live(&self) -> bool {
        true
    }
}

/// Single-node / no-peers publisher. Every `publish` is a no-op success.
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

/// Build the publisher selected by `cfg`. Returns the [`NoopPublisher`] for
/// `Disabled` and (when the `nats` feature is on) a connected JetStream
/// publisher for `Nats`. Without the `nats` feature, `Nats` falls back to noop
/// with a warning so a misconfigured single-binary still boots.
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
        // inner Signed<T> is preserved verbatim and re-deserialisable.
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
        // no FED_GOSSIP set in test env => Disabled
        std::env::remove_var("FED_GOSSIP");
        assert!(matches!(GossipConfig::from_env(), GossipConfig::Disabled));
    }
}

#[cfg(feature = "nats")]
pub mod nats {
    //! NATS JetStream transport. Gated behind the `nats` feature so the
    //! workspace builds without `async-nats` present. Implements the
    //! federation subjects and a 30-day retention window; the durable source
    //! of truth stays in Postgres.
    //!
    //! mTLS is wired in [`NatsPublisher::connect`]: client cert/key + CA root
    //! are read from `FED_NATS_CLIENT_CERT` / `FED_NATS_CLIENT_KEY` /
    //! `FED_NATS_ROOT_CA` (file paths). When none are set the connect path is
    //! plaintext for a single-broker dev deploy on loopback, which is the
    //! current state when peer-list entries have an empty `mtls_root_pem`.

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
            // mTLS: the federation NATS account uses an account-isolated,
            // mutually-authenticated broker, each peer cert rooted in
            // `mtls_root_pem` from the peer-list entry. mTLS material is sourced
            // from env so operators can roll it out without a rebuild:
            //
            //   FED_NATS_CLIENT_CERT  path to PEM client cert (this catalyst)
            //   FED_NATS_CLIENT_KEY   path to PEM client private key
            //   FED_NATS_ROOT_CA      path to PEM CA root (peers' `mtls_root_pem`)
            //
            // STATUS (single-broker dev deploy): when the broker ships no TLS
            // block and every peer-list entry has an empty `mtls_root_pem`,
            // none of the three vars are set and the connect path below is
            // plaintext against the loopback broker. A catalyst that runs
            // federation against a *remote* peer without these set is insecure
            // and should be omitted from the peer list at validation time.
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

        /// Ensure a per-scope JetStream stream exists (idempotent). 30-day
        /// retention; Postgres is the durable source of truth,
        /// the stream is a catch-up window for peers that were briefly offline.
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

        /// Subscribe to a scope's action subject and forward decoded envelopes
        /// onto `tx`. The caller drives a loop that re-verifies + applies each
        /// envelope. Self-published envelopes (origin_peer == our peer_id) are
        /// dropped to avoid apply loops.
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
                            // skip our own echoes
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
            // Channel depth = a generous in-flight window; backpressure on the
            // apply loop simply slows JetStream ack, which is fine (Postgres is
            // the durable source of truth and replay is idempotent).
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
