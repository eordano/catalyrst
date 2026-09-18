//! Internal authority client. It never reconstructs or transmits a bearer JWT.
use super::{auth::VerifiedRoom, RoomAuthority, MAX_AUTHORITY_LEASE};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

pub const AUTHORITY_PATH: &str = "/internal/pulse/room-authority/v1";
const MAX_BODY: usize = 16_384;
const MAX_LEASES: usize = 16_384;
type ScopeKey = ([u8; 32], [u8; 32]);

pub mod wire {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct AuthorityRequest {
        #[prost(oneof = "operation::Operation", tags = "1,2,3")]
        pub operation: Option<operation::Operation>,
    }
    pub mod operation {
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum Operation {
            #[prost(message, tag = "1")]
            Join(super::Join),
            #[prost(message, tag = "2")]
            Renew(super::Context),
            #[prost(message, tag = "3")]
            Release(super::Context),
        }
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Join {
        #[prost(bytes = "vec", tag = "1")]
        pub pulse_instance: Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub connection_generation: Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub wallet: Vec<u8>,
        #[prost(bytes = "vec", tag = "4")]
        pub session: Vec<u8>,
        #[prost(string, tag = "5")]
        pub header_payload: String,
        #[prost(bytes = "vec", tag = "6")]
        pub nonce: Vec<u8>,
        #[prost(bytes = "vec", tag = "7")]
        pub proof: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Context {
        #[prost(bytes = "vec", tag = "1")]
        pub pulse_instance: Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub connection_generation: Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub lease_id: Vec<u8>,
        #[prost(bytes = "vec", tag = "4")]
        pub authority_incarnation: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct AuthorityResponse {
        #[prost(oneof = "result::Result", tags = "1,2,3")]
        pub result: Option<result::Result>,
    }
    pub mod result {
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum Result {
            #[prost(message, tag = "1")]
            Lease(super::Lease),
            #[prost(message, tag = "2")]
            Denied(super::Denied),
            #[prost(message, tag = "3")]
            Released(super::Released),
        }
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Lease {
        #[prost(bytes = "vec", tag = "1")]
        pub lease_id: Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub authority_incarnation: Vec<u8>,
        #[prost(uint32, tag = "3")]
        pub ttl_ms: u32,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Denied {
        #[prost(uint32, tag = "1")]
        pub reason: u32,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Released {}
}

struct Entry {
    serial: u64,
    deadline: Instant,
    lease: Option<wire::Lease>,
    in_flight: bool,
}
#[derive(Default)]
struct Registry {
    entries: HashMap<ScopeKey, Entry>,
    serial: u64,
    last_prune: Option<Instant>,
}
struct Inner {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    service_key: [u8; 32],
    instance: [u8; 16],
    registry: Mutex<Registry>,
    permits: Arc<Semaphore>,
}
#[derive(Clone)]
pub struct HttpAuthority {
    inner: Arc<Inner>,
}

impl HttpAuthority {
    pub fn new(
        endpoint: &str,
        encoded_service_key: &str,
        allow_loopback_http: bool,
    ) -> anyhow::Result<Self> {
        let endpoint = reqwest::Url::parse(endpoint)
            .map_err(|_| anyhow::anyhow!("invalid room authority URL"))?;
        let loopback = endpoint
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if (endpoint.scheme() != "https"
            && !(endpoint.scheme() == "http" && allow_loopback_http && loopback))
            || endpoint.path() != AUTHORITY_PATH
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
        {
            anyhow::bail!("room authority requires its exact HTTPS endpoint (explicit loopback HTTP fixtures only)");
        }
        let service_key: [u8; 32] = URL_SAFE_NO_PAD
            .decode(encoded_service_key)
            .ok()
            .and_then(|key| key.try_into().ok())
            .ok_or_else(|| {
                anyhow::anyhow!("room authority service key must be 32 base64url bytes")
            })?;
        let mut instance = [0; 16];
        getrandom::fill(&mut instance)
            .map_err(|_| anyhow::anyhow!("cannot create room authority instance"))?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(Duration::from_millis(750))
            .build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                endpoint,
                service_key,
                instance,
                registry: Mutex::new(Registry::default()),
                permits: Arc::new(Semaphore::new(128)),
            }),
        })
    }
}

fn key(room: &VerifiedRoom) -> ScopeKey {
    (
        room.nonce,
        Sha256::digest(room.header_payload.as_bytes()).into(),
    )
}
fn context(inner: &Inner, nonce: &[u8; 32], lease: &wire::Lease) -> wire::Context {
    wire::Context {
        pulse_instance: inner.instance.to_vec(),
        connection_generation: nonce[..16].to_vec(),
        lease_id: lease.lease_id.clone(),
        authority_incarnation: lease.authority_incarnation.clone(),
    }
}
fn signed_headers(key: &[u8; 32], body: &[u8], timestamp: &str, nonce: &str) -> Result<String, ()> {
    let hash = Sha256::digest(body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let input = format!("POST\n{AUTHORITY_PATH}\n{timestamp}\n{nonce}\n{hash}");
    let mut signature = Hmac::<Sha256>::new_from_slice(key).map_err(|_| ())?;
    signature.update(input.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(signature.finalize().into_bytes()))
}
async fn request(
    inner: &Inner,
    operation: wire::operation::Operation,
) -> Result<wire::AuthorityResponse, ()> {
    let body = wire::AuthorityRequest {
        operation: Some(operation),
    }
    .encode_to_vec();
    if body.len() > MAX_BODY {
        return Err(());
    }
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| ())?;
    let nonce = URL_SAFE_NO_PAD.encode(nonce);
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signature = signed_headers(&inner.service_key, &body, &timestamp, &nonce)?;
    let response = inner
        .client
        .post(inner.endpoint.clone())
        .header("Content-Type", "application/x-protobuf")
        .header("Accept", "application/x-protobuf")
        .header("X-Pulse-Authority-Timestamp", timestamp)
        .header("X-Pulse-Authority-Nonce", nonce)
        .header("X-Pulse-Authority-Signature", signature)
        .body(body)
        .send()
        .await
        .map_err(|_| ())?;
    if response.status() != reqwest::StatusCode::OK
        || !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/x-protobuf"))
            })
        || response
            .content_length()
            .is_some_and(|len| len > MAX_BODY as u64)
    {
        return Err(());
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if body.len() + chunk.len() > MAX_BODY {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    wire::AuthorityResponse::decode(body.as_slice()).map_err(|_| ())
}

impl RoomAuthority for HttpAuthority {
    fn authorize(
        &self,
        room: VerifiedRoom,
        renewal: bool,
    ) -> Pin<Box<dyn Future<Output = Result<Duration, ()>> + Send + '_>> {
        Box::pin(async move {
            let _permit = self
                .inner
                .permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| ())?;
            let scope = key(&room);
            let started = Instant::now();
            let (serial, previous) = {
                let mut registry = self.inner.registry.lock().map_err(|_| ())?;
                if registry
                    .last_prune
                    .is_none_or(|last| started.duration_since(last) >= Duration::from_secs(1))
                {
                    registry.entries.retain(|_, entry| entry.deadline > started);
                    registry.last_prune = Some(started);
                }
                if registry
                    .entries
                    .get(&scope)
                    .is_some_and(|entry| entry.deadline <= started)
                {
                    registry.entries.remove(&scope);
                }
                if let Some(entry) = registry.entries.get(&scope) {
                    if entry.in_flight {
                        return Err(());
                    }
                    if !renewal {
                        return Ok(entry.deadline.duration_since(started));
                    }
                } else if renewal || registry.entries.len() >= MAX_LEASES {
                    return Err(());
                }
                let previous = registry
                    .entries
                    .get(&scope)
                    .and_then(|entry| entry.lease.clone());
                registry.serial = registry.serial.checked_add(1).ok_or(())?;
                let serial = registry.serial;
                let entry = registry.entries.entry(scope).or_insert(Entry {
                    serial,
                    deadline: started + Duration::from_millis(750),
                    lease: None,
                    in_flight: true,
                });
                entry.serial = serial;
                entry.in_flight = true;
                (serial, previous)
            };
            let operation = match previous.as_ref() {
                Some(lease) => {
                    wire::operation::Operation::Renew(context(&self.inner, &room.nonce, lease))
                }
                None => wire::operation::Operation::Join(wire::Join {
                    pulse_instance: self.inner.instance.to_vec(),
                    connection_generation: room.nonce[..16].to_vec(),
                    wallet: super::auth::address_bytes(&room.wallet).ok_or(())?.to_vec(),
                    session: super::auth::address_bytes(&room.session)
                        .ok_or(())?
                        .to_vec(),
                    header_payload: room.header_payload.clone(),
                    nonce: room.nonce.to_vec(),
                    proof: room.proof.clone(),
                }),
            };
            let result = request(&self.inner, operation).await;
            let mut registry = self.inner.registry.lock().map_err(|_| ())?;
            let Some(entry) = registry
                .entries
                .get_mut(&scope)
                .filter(|entry| entry.serial == serial)
            else {
                return Err(());
            };
            let valid = result
                .ok()
                .and_then(|response| match response.result {
                    Some(wire::result::Result::Lease(lease)) => Some(lease),
                    _ => None,
                })
                .filter(|lease| {
                    lease.lease_id.len() == 16
                        && lease.authority_incarnation.len() == 16
                        && lease.ttl_ms > 0
                        && Duration::from_millis(u64::from(lease.ttl_ms)) <= MAX_AUTHORITY_LEASE
                        && started + Duration::from_millis(u64::from(lease.ttl_ms)) > Instant::now()
                        && previous.as_ref().is_none_or(|old| {
                            old.lease_id == lease.lease_id
                                && old.authority_incarnation == lease.authority_incarnation
                        })
                });
            if let Some(lease) = valid.filter(|_| entry.deadline > Instant::now()) {
                let ttl = Duration::from_millis(u64::from(lease.ttl_ms));
                entry.deadline = started + ttl;
                entry.lease = Some(lease);
                entry.in_flight = false;
                Ok(ttl)
            } else {
                registry.entries.remove(&scope);
                Err(())
            }
        })
    }
    fn release(&self, room: VerifiedRoom) {
        let lease = self
            .inner
            .registry
            .lock()
            .ok()
            .and_then(|mut registry| registry.entries.remove(&key(&room)))
            .and_then(|entry| entry.lease);
        let Some(lease) = lease else {
            return;
        };
        let Ok(permit) = self.inner.permits.clone().try_acquire_owned() else {
            return;
        };
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let operation =
                wire::operation::Operation::Release(context(&inner, &room.nonce, &lease));
            let _ = request(&inner, operation).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::auth::{RoomClaims, RoomGrant};
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn credential() -> VerifiedRoom {
        VerifiedRoom {
            claims: RoomClaims {
                iss: "key".into(),
                sub: "0x1111111111111111111111111111111111111111".into(),
                exp: 100,
                nbf: 0,
                video: RoomGrant {
                    room: "scene".into(),
                    room_join: true,
                    can_publish_data: true,
                },
                metadata: String::new(),
            },
            wallet: "0x1111111111111111111111111111111111111111".into(),
            session: "0x2222222222222222222222222222222222222222".into(),
            is_guest: true,
            header_payload: "header.payload".into(),
            nonce: [3; 32],
            proof: vec![4; 32],
        }
    }
    fn lease(id: u8, ttl_ms: u32) -> Vec<u8> {
        wire::AuthorityResponse {
            result: Some(wire::result::Result::Lease(wire::Lease {
                lease_id: vec![id; 16],
                authority_incarnation: vec![8; 16],
                ttl_ms,
            })),
        }
        .encode_to_vec()
    }
    async fn fixture(
        responses: Vec<(u16, Vec<u8>, Duration)>,
    ) -> (
        HttpAuthority,
        tokio::task::JoinHandle<Vec<wire::AuthorityRequest>>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}{AUTHORITY_PATH}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut seen = Vec::new();
            let mut nonces = std::collections::HashSet::new();
            for (status, response, delay) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let split = loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..split].to_vec()).unwrap();
                assert!(headers.starts_with(&format!("POST {AUTHORITY_PATH} HTTP/1.1\r\n")));
                let header = |name: &str| {
                    headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                                .map(|(_, v)| v.trim().to_owned())
                        })
                        .unwrap()
                };
                let len: usize = header("content-length").parse().unwrap();
                while bytes.len() - split < len {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let body = &bytes[split..split + len];
                assert_eq!(header("content-type"), "application/x-protobuf");
                assert_eq!(
                    header("x-pulse-authority-signature"),
                    signed_headers(
                        &[5; 32],
                        body,
                        &header("x-pulse-authority-timestamp"),
                        &header("x-pulse-authority-nonce")
                    )
                    .unwrap()
                );
                assert!(nonces.insert(header("x-pulse-authority-nonce")));
                seen.push(wire::AuthorityRequest::decode(body).unwrap());
                tokio::time::sleep(delay).await;
                let headers=format!("HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nContent-Type: application/x-protobuf\r\nLocation: http://127.0.0.1:1/forbidden\r\nConnection: close\r\n\r\n",response.len());
                let _ = stream.write_all(headers.as_bytes()).await;
                let _ = stream.write_all(&response).await;
            }
            seen
        });
        (
            HttpAuthority::new(&endpoint, &URL_SAFE_NO_PAD.encode([5; 32]), true).unwrap(),
            handle,
        )
    }
    #[tokio::test]
    async fn protobuf_hmac_join_renew_release_preserve_exact_context() {
        let released = wire::AuthorityResponse {
            result: Some(wire::result::Result::Released(wire::Released {})),
        }
        .encode_to_vec();
        let (authority, server) = fixture(vec![
            (200, lease(7, 2000), Duration::ZERO),
            (200, lease(7, 2000), Duration::ZERO),
            (200, released, Duration::ZERO),
        ])
        .await;
        let room = credential();
        assert_eq!(
            authority.authorize(room.clone(), false).await,
            Ok(Duration::from_secs(2))
        );
        assert_eq!(
            authority.authorize(room.clone(), true).await,
            Ok(Duration::from_secs(2))
        );
        authority.release(room);
        let seen = server.await.unwrap();
        let Some(wire::operation::Operation::Join(join)) = &seen[0].operation else {
            panic!()
        };
        let Some(wire::operation::Operation::Renew(renew)) = &seen[1].operation else {
            panic!()
        };
        let Some(wire::operation::Operation::Release(release)) = &seen[2].operation else {
            panic!()
        };
        assert_eq!(join.header_payload, "header.payload");
        assert_eq!(join.proof, vec![4; 32]);
        assert_eq!(join.pulse_instance, renew.pulse_instance);
        assert_eq!(join.connection_generation, renew.connection_generation);
        assert_eq!(renew, release);
        assert_eq!(renew.lease_id, vec![7; 16]);
    }
    #[tokio::test]
    async fn changed_lease_denial_redirect_malformed_oversize_and_timeout_fail_closed() {
        let (authority, server) = fixture(vec![
            (200, lease(7, 2000), Duration::ZERO),
            (200, lease(9, 2000), Duration::ZERO),
        ])
        .await;
        assert!(authority.authorize(credential(), false).await.is_ok());
        assert!(authority.authorize(credential(), true).await.is_err());
        assert!(authority.authorize(credential(), true).await.is_err());
        server.await.unwrap();
        for (status, body, delay) in [
            (302, lease(7, 2000), Duration::ZERO),
            (200, vec![255], Duration::ZERO),
            (200, vec![0; MAX_BODY + 1], Duration::ZERO),
            (200, lease(7, 2001), Duration::ZERO),
            (200, lease(7, 2000), Duration::from_millis(800)),
        ] {
            let (authority, server) = fixture(vec![(status, body, delay)]).await;
            assert!(authority.authorize(credential(), false).await.is_err());
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn release_during_pending_join_prevents_late_result_from_recreating_lease() {
        let (authority, server) =
            fixture(vec![(200, lease(7, 2000), Duration::from_millis(100))]).await;
        let copy = authority.clone();
        let job = tokio::spawn(async move { copy.authorize(credential(), false).await });
        while authority.inner.registry.lock().unwrap().entries.is_empty() {
            tokio::task::yield_now().await;
        }
        authority.release(credential());
        assert!(job.await.unwrap().is_err());
        assert!(authority.inner.registry.lock().unwrap().entries.is_empty());
        server.await.unwrap();
    }
    #[test]
    fn insecure_urls_and_wrong_service_keys_are_rejected() {
        let key = URL_SAFE_NO_PAD.encode([5; 32]);
        assert!(
            HttpAuthority::new(&format!("http://127.0.0.1{AUTHORITY_PATH}"), &key, false).is_err()
        );
        assert!(
            HttpAuthority::new(&format!("http://192.0.2.1{AUTHORITY_PATH}"), &key, true).is_err()
        );
        assert!(HttpAuthority::new("https://example.test/wrong", &key, false).is_err());
        assert!(HttpAuthority::new(
            &format!("https://example.test{AUTHORITY_PATH}"),
            "short",
            false
        )
        .is_err());
    }
}
