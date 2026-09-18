use super::{wire, DenialReason};
use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use catalyrst_pulse::application_relay::auth::{ProofVerifier, VerifiedRoom, MAX_CLAIMS_BYTES};
use hmac::{Hmac, KeyInit, Mac};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Semaphore, time::Instant};
use uuid::Uuid;

pub const ENDPOINT: &str = "/internal/pulse/room-authority/v1";
const CONTENT_TYPE: &str = "application/x-protobuf";
const MAX_BODY: usize = 16_384;
const MAX_REPLAYS: usize = MAX_LEASES * (REPLAY_TTL.as_secs() as usize + 5);
const MAX_LEASES: usize = 16_384;
const MAX_CHECKS: usize = 128;
const CHECK_TIMEOUT: Duration = Duration::from_millis(650);
const LEASE_TTL: Duration = Duration::from_secs(2);
const REPLAY_TTL: Duration = Duration::from_secs(11);
type HmacSha256 = Hmac<Sha256>;

#[async_trait]
pub trait RelayPolicy: Send + Sync {
    async fn authorize(&self, room: &VerifiedRoom) -> Result<(), DenialReason>;
}

pub struct RelayAuthority {
    policy: Arc<dyn RelayPolicy>,
    verifier: ProofVerifier,
    service_key: [u8; 32],
    incarnation: [u8; 16],
    requests: Semaphore,
    checks: Semaphore,
    leases: Mutex<LeaseRegistry>,
    replays: Mutex<HashMap<[u8; 16], Instant>>,
}

struct LeaseRegistry {
    entries: HashMap<[u8; 16], StoredLease>,
    next_sweep: Instant,
}

struct StoredLease {
    instance: [u8; 16],
    generation: [u8; 16],
    room: Arc<VerifiedRoom>,
    expires: Instant,
    checking: bool,
}

struct Binding {
    instance: [u8; 16],
    generation: [u8; 16],
    id: [u8; 16],
}

impl RelayAuthority {
    pub fn new(
        policy: Arc<dyn RelayPolicy>,
        livekit_api_key: String,
        livekit_secret: Vec<u8>,
        service_key: [u8; 32],
    ) -> Result<Self, &'static str> {
        if livekit_secret.as_slice() == service_key {
            return Err("relay authority requires a dedicated service key");
        }
        Ok(Self {
            policy,
            verifier: ProofVerifier::new(livekit_api_key, livekit_secret)?,
            service_key,
            incarnation: *Uuid::new_v4().as_bytes(),
            requests: Semaphore::new(MAX_CHECKS),
            checks: Semaphore::new(MAX_CHECKS),
            leases: Mutex::new(LeaseRegistry {
                entries: HashMap::new(),
                next_sweep: Instant::now(),
            }),
            replays: Mutex::new(HashMap::new()),
        })
    }

    async fn handle(&self, request: Request<Body>) -> Response {
        let started = Instant::now();
        let _request_permit = match self.requests.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, DenialReason::Capacity)
            }
        };
        let wall = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(time) => time,
            Err(_) => {
                return error_response(StatusCode::SERVICE_UNAVAILABLE, DenialReason::Unavailable)
            }
        };
        if request.method() != Method::POST {
            return error_response(StatusCode::METHOD_NOT_ALLOWED, DenialReason::Invalid);
        }
        if request.uri().path() != ENDPOINT || request.uri().query().is_some() {
            return error_response(StatusCode::NOT_FOUND, DenialReason::Invalid);
        }
        if one_header(request.headers(), header::CONTENT_TYPE.as_str()) != Some(CONTENT_TYPE)
            || one_header(request.headers(), header::ACCEPT.as_str()) != Some(CONTENT_TYPE)
        {
            return error_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, DenialReason::Invalid);
        }
        let (parts, body) = request.into_parts();
        let bytes = match tokio::time::timeout_at(started + CHECK_TIMEOUT, to_bytes(body, MAX_BODY))
            .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(_)) => {
                return error_response(StatusCode::PAYLOAD_TOO_LARGE, DenialReason::Invalid)
            }
            Err(_) => {
                return error_response(StatusCode::REQUEST_TIMEOUT, DenialReason::Unavailable)
            }
        };
        if let Err(reason) = self.authenticate(&parts.headers, &bytes, wall.as_secs(), started) {
            let status = if reason == DenialReason::Capacity || reason == DenialReason::Unavailable
            {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::UNAUTHORIZED
            };
            return error_response(status, reason);
        }
        let request = match wire::AuthorityRequest::decode(bytes) {
            Ok(request) => request,
            Err(_) => return error_response(StatusCode::BAD_REQUEST, DenialReason::Invalid),
        };
        let result = self.execute(request, wall, started).await;
        protobuf_response(StatusCode::OK, result)
    }

    fn authenticate(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        now_secs: u64,
        now: Instant,
    ) -> Result<(), DenialReason> {
        let timestamp =
            one_header(headers, "x-pulse-authority-timestamp").ok_or(DenialReason::Invalid)?;
        let seconds = timestamp
            .parse::<u64>()
            .map_err(|_| DenialReason::Invalid)?;
        if seconds.to_string() != timestamp || seconds.abs_diff(now_secs) > 5 {
            return Err(DenialReason::Invalid);
        }
        let nonce_text =
            one_header(headers, "x-pulse-authority-nonce").ok_or(DenialReason::Invalid)?;
        let nonce = decode_fixed::<16>(nonce_text)?;
        let signature = decode_fixed::<32>(
            one_header(headers, "x-pulse-authority-signature").ok_or(DenialReason::Invalid)?,
        )?;
        let preimage = format!(
            "POST\n{ENDPOINT}\n{timestamp}\n{nonce_text}\n{}",
            hex::encode(Sha256::digest(body))
        );
        let mut mac =
            HmacSha256::new_from_slice(&self.service_key).map_err(|_| DenialReason::Unavailable)?;
        mac.update(preimage.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| DenialReason::NotAuthorized)?;
        let mut replays = self.replays.lock().map_err(|_| DenialReason::Unavailable)?;
        if replays.get(&nonce).is_some_and(|expires| *expires > now) {
            return Err(DenialReason::NotAuthorized);
        }
        if replays.len() >= MAX_REPLAYS {
            replays.retain(|_, expires| *expires > now);
            if replays.len() >= MAX_REPLAYS {
                return Err(DenialReason::Capacity);
            }
        }
        replays.insert(nonce, now + REPLAY_TTL);
        Ok(())
    }

    async fn execute(
        &self,
        request: wire::AuthorityRequest,
        wall: Duration,
        started: Instant,
    ) -> wire::AuthorityResponse {
        use wire::authority_request::Operation;
        let result = match request.operation {
            Some(Operation::Join(join)) => self.join(join, wall, started).await,
            Some(Operation::Renew(renew)) => self.renew(renew, started).await,
            Some(Operation::Release(release)) => self.release(release),
            None => Err(DenialReason::Invalid),
        };
        wire::AuthorityResponse {
            result: Some(result.unwrap_or_else(denied)),
        }
    }

    async fn join(
        &self,
        join: wire::Join,
        wall: Duration,
        started: Instant,
    ) -> Result<wire::authority_response::Result, DenialReason> {
        let instance = fixed::<16>(&join.pulse_instance)?;
        let generation = fixed::<16>(&join.connection_generation)?;
        let wallet = fixed::<20>(&join.wallet)?;
        let session = fixed::<20>(&join.session)?;
        let nonce = fixed::<32>(&join.nonce)?;
        fixed::<32>(&join.proof)?;
        if join.header_payload.len() > MAX_CLAIMS_BYTES {
            return Err(DenialReason::Invalid);
        }
        let room = self
            .verifier
            .verify(
                &join.header_payload,
                &join.proof,
                &nonce,
                &format!("0x{}", hex::encode(wallet)),
                &format!("0x{}", hex::encode(session)),
                wall.saturating_add(started.elapsed()).as_secs(),
            )
            .map_err(|_| DenialReason::NotAuthorized)?;
        {
            let mut leases = self.leases.lock().map_err(|_| DenialReason::Unavailable)?;
            leases.sweep(Instant::now());
            if leases.entries.len() >= MAX_LEASES {
                return Err(DenialReason::Capacity);
            }
        }
        self.check_policy(&room, started).await?;
        if room.claims.exp <= wall.saturating_add(started.elapsed()).as_secs() {
            return Err(DenialReason::Expired);
        }
        let now = Instant::now();
        let expires = started + LEASE_TTL;
        if expires <= now {
            return Err(DenialReason::Expired);
        }
        let mut leases = self.leases.lock().map_err(|_| DenialReason::Unavailable)?;
        leases.sweep(now);
        if leases.entries.len() >= MAX_LEASES {
            return Err(DenialReason::Capacity);
        }
        let id = *Uuid::new_v4().as_bytes();
        if leases.entries.contains_key(&id) {
            return Err(DenialReason::Unavailable);
        }
        leases.entries.insert(
            id,
            StoredLease {
                instance,
                generation,
                room: Arc::new(room),
                expires,
                checking: false,
            },
        );
        Ok(self.lease_response(id))
    }

    async fn renew(
        &self,
        renew: wire::Renew,
        started: Instant,
    ) -> Result<wire::authority_response::Result, DenialReason> {
        let binding = self.binding(
            &renew.pulse_instance,
            &renew.connection_generation,
            &renew.lease_id,
            &renew.authority_incarnation,
        )?;
        let room = {
            let mut leases = self.leases.lock().map_err(|_| DenialReason::Unavailable)?;
            let entry = leases
                .entries
                .get(&binding.id)
                .ok_or(DenialReason::Expired)?;
            if !binding.matches(entry) {
                return Err(DenialReason::Invalid);
            }
            if entry.expires <= Instant::now() {
                leases.entries.remove(&binding.id);
                return Err(DenialReason::Expired);
            }
            if entry.checking {
                leases.entries.remove(&binding.id);
                return Err(DenialReason::Capacity);
            }
            let entry = leases
                .entries
                .get_mut(&binding.id)
                .ok_or(DenialReason::Expired)?;
            entry.checking = true;
            Arc::clone(&entry.room)
        };
        let authorized = self.check_policy(&room, started).await;
        let mut leases = self.leases.lock().map_err(|_| DenialReason::Unavailable)?;
        let entry = leases
            .entries
            .get(&binding.id)
            .ok_or(DenialReason::Expired)?;
        if !binding.matches(entry) {
            return Err(DenialReason::Invalid);
        }
        if let Err(reason) = authorized {
            leases.entries.remove(&binding.id);
            return Err(reason);
        }
        if entry.expires <= Instant::now() || started + LEASE_TTL <= Instant::now() {
            leases.entries.remove(&binding.id);
            return Err(DenialReason::Expired);
        }
        let entry = leases
            .entries
            .get_mut(&binding.id)
            .ok_or(DenialReason::Expired)?;
        entry.expires = started + LEASE_TTL;
        entry.checking = false;
        Ok(self.lease_response(binding.id))
    }

    fn release(
        &self,
        release: wire::Release,
    ) -> Result<wire::authority_response::Result, DenialReason> {
        let binding = self.binding(
            &release.pulse_instance,
            &release.connection_generation,
            &release.lease_id,
            &release.authority_incarnation,
        )?;
        let mut leases = self.leases.lock().map_err(|_| DenialReason::Unavailable)?;
        let entry = leases
            .entries
            .get(&binding.id)
            .ok_or(DenialReason::Expired)?;
        if !binding.matches(entry) {
            return Err(DenialReason::Invalid);
        }
        leases.entries.remove(&binding.id);
        Ok(wire::authority_response::Result::Released(
            wire::Released {},
        ))
    }

    async fn check_policy(
        &self,
        room: &VerifiedRoom,
        started: Instant,
    ) -> Result<(), DenialReason> {
        if started.elapsed() >= CHECK_TIMEOUT {
            return Err(DenialReason::Unavailable);
        }
        let _permit = self
            .checks
            .try_acquire()
            .map_err(|_| DenialReason::Capacity)?;
        let result =
            tokio::time::timeout_at(started + CHECK_TIMEOUT, self.policy.authorize(room)).await;
        if started.elapsed() >= CHECK_TIMEOUT {
            return Err(DenialReason::Unavailable);
        }
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(DenialReason::Unspecified)) | Err(_) => Err(DenialReason::Unavailable),
            Ok(Err(reason)) => Err(reason),
        }
    }

    fn binding(
        &self,
        instance: &[u8],
        generation: &[u8],
        id: &[u8],
        incarnation: &[u8],
    ) -> Result<Binding, DenialReason> {
        if fixed::<16>(incarnation)? != self.incarnation {
            return Err(DenialReason::Expired);
        }
        Ok(Binding {
            instance: fixed(instance)?,
            generation: fixed(generation)?,
            id: fixed(id)?,
        })
    }

    fn lease_response(&self, id: [u8; 16]) -> wire::authority_response::Result {
        wire::authority_response::Result::Lease(wire::Lease {
            lease_id: id.to_vec(),
            authority_incarnation: self.incarnation.to_vec(),
            ttl_ms: LEASE_TTL.as_millis() as u32,
        })
    }
}

impl LeaseRegistry {
    fn sweep(&mut self, now: Instant) {
        if now >= self.next_sweep || self.entries.len() >= MAX_LEASES {
            self.entries.retain(|_, entry| entry.expires > now);
            self.next_sweep = now + Duration::from_millis(100);
        }
    }
}

impl Binding {
    fn matches(&self, entry: &StoredLease) -> bool {
        self.instance == entry.instance && self.generation == entry.generation
    }
}

pub fn router<S: Clone + Send + Sync + 'static>(authority: Arc<RelayAuthority>) -> Router<S> {
    Router::new().route(
        ENDPOINT,
        any(move |request: Request<Body>| {
            let authority = Arc::clone(&authority);
            async move { authority.handle(request).await }
        }),
    )
}

fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], DenialReason> {
    bytes.try_into().map_err(|_| DenialReason::Invalid)
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], DenialReason> {
    if value.len() != (N * 8).div_ceil(6) {
        return Err(DenialReason::Invalid);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| DenialReason::Invalid)?;
    if URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(DenialReason::Invalid);
    }
    fixed(&bytes)
}

fn denied(reason: DenialReason) -> wire::authority_response::Result {
    wire::authority_response::Result::Denied(wire::Denied {
        reason: reason as i32,
    })
}

fn error_response(status: StatusCode, reason: DenialReason) -> Response {
    protobuf_response(
        status,
        wire::AuthorityResponse {
            result: Some(denied(reason)),
        },
    )
}

fn protobuf_response(status: StatusCode, body: wire::AuthorityResponse) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, CONTENT_TYPE),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.encode_to_vec(),
    )
        .into_response()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
