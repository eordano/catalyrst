use std::collections::HashMap;

use catalyrst_types::MAX_AUTH_CHAIN_LINKS;
use prost::Message;
use sha2::{Digest, Sha256};

use crate::decentraland::pulse::{
    server_message, PulseV4Auth, PulseV4AuthTranscript, PulseV4Challenge, PulseV4ErrorCode,
    PulseV4Hello, PulseV4Limits, PulseV4Result, PulseV4RetryClass, ServerMessage,
};
use crate::handshake::{decode_v4_auth_chain, verify_v4_auth_chain, VerifiedHandshake};

pub const PROTOCOL_VERSION: u32 = 4;
pub const SIGNING_DOMAIN: &str = "dcl-pulse-auth-v4:";
pub const CAPABILITY_DELTA_BATCH: &str = "delta_batch";
pub const CAPABILITY_DELTA_BATCH_BASELINE: &str = "delta_batch_baseline";
pub const CAPABILITY_DELTA_BATCH_DICTIONARY: &str = "delta_batch_dictionary";
pub const CAPABILITY_DELTA_BATCH_SAMPLE_TICK: &str = "delta_batch_sample_tick";
pub const CAPABILITY_APPLICATION_RELAY: &str = "application_relay";
pub const SUPPORTED_CAPABILITIES: &[&str] = &[
    CAPABILITY_DELTA_BATCH,
    CAPABILITY_DELTA_BATCH_BASELINE,
    CAPABILITY_DELTA_BATCH_DICTIONARY,
    CAPABILITY_DELTA_BATCH_SAMPLE_TICK,
];
pub const DEFAULT_CHALLENGE_TTL_MS: u32 = 15_000;
pub const DEFAULT_MAX_PENDING: usize = 8_192;
pub const DEFAULT_MAX_AUTH_CHAIN_BYTES: usize = 3_072;
pub const DEFAULT_MAX_CAPABILITIES: usize = 32;
pub const DEFAULT_MAX_PUBLIC_DETAIL_BYTES: usize = 128;
pub const DEFAULT_MAX_FRAME_BYTES: u32 = 4_096;
const MAX_COMMITTED_PER_PEER: usize = 4;
const AUTH_ENVELOPE_RESERVE_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseV4Config {
    pub audience: String,
    pub issuer: String,
    pub challenge_ttl_ms: u32,
    pub max_pending: usize,
    pub max_auth_chain_bytes: usize,
    pub max_capabilities: usize,
    pub max_public_detail_bytes: usize,
    pub max_frame_bytes: u32,
}

impl PulseV4Config {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.audience.trim().is_empty() {
            return Err("Pulse v4 audience must not be empty");
        }
        if self.issuer.trim().is_empty() {
            return Err("Pulse v4 issuer must not be empty");
        }
        if self.audience.len() > 256 || self.issuer.len() > 256 {
            return Err("Pulse v4 audience and issuer must be at most 256 bytes");
        }
        if self.challenge_ttl_ms == 0 {
            return Err("Pulse v4 challenge TTL must be positive");
        }
        if self.max_pending == 0 {
            return Err("Pulse v4 pending capacity must be positive");
        }
        if self.max_auth_chain_bytes == 0
            || self.max_capabilities == 0
            || self.max_public_detail_bytes == 0
            || self.max_frame_bytes == 0
        {
            return Err("Pulse v4 advertised limits must be positive");
        }
        if self.max_pending > u32::MAX as usize
            || self.max_auth_chain_bytes > u32::MAX as usize
            || self.max_capabilities > u32::MAX as usize
            || self.max_public_detail_bytes > u32::MAX as usize
        {
            return Err("Pulse v4 advertised limit exceeds uint32");
        }
        if self.max_frame_bytes as usize <= AUTH_ENVELOPE_RESERVE_BYTES
            || self.max_auth_chain_bytes
                > self.max_frame_bytes as usize - AUTH_ENVELOPE_RESERVE_BYTES
        {
            return Err("Pulse v4 auth-chain limit does not fit the advertised frame");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct PendingChallenge {
    hello: PulseV4Hello,
    hello_fingerprint: [u8; 32],
    challenge: PulseV4Challenge,
}

#[derive(Debug, Clone)]
struct CommittedResult {
    auth_fingerprint: [u8; 32],
    challenge_id: Vec<u8>,
    result: PulseV4Result,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedV4Auth {
    pub auth: PulseV4Auth,
    pub hello: PulseV4Hello,
    pub verified: VerifiedHandshake,
    pub negotiated_capabilities: Vec<String>,
    pub auth_fingerprint: [u8; 32],
    pub relay_nonce: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) enum V4AuthOutcome {
    Verified(Box<VerifiedV4Auth>),
    Reply(PulseV4Result),
}

/// Process-local challenge authority.
///
/// A challenge is usable only on the live peer that received it. Its signed transcript binds the
/// deployment audience, replica, random process incarnation and random peer binding. Another
/// replica has no pending entry and a different issuer/incarnation; a restarted process has a new
/// incarnation and no pending entry; a reconnected peer has a new binding. This is the exclusion
/// boundary that makes a shared challenge database unnecessary. Terminal results remain cached
/// only on the same live peer and are discarded with the connection.
#[derive(Debug, Clone)]
pub struct PulseV4Authority {
    config: Option<PulseV4Config>,
    application_relay: bool,
    process_incarnation: [u8; 16],
    peer_bindings: HashMap<u32, [u8; 32]>,
    pending: HashMap<u32, PendingChallenge>,
    committed: HashMap<u32, Vec<CommittedResult>>,
    admitted: HashMap<u32, (Vec<u8>, u64)>,
}

impl Default for PulseV4Authority {
    fn default() -> Self {
        Self::disabled()
    }
}

impl PulseV4Authority {
    pub fn disabled() -> Self {
        Self {
            config: None,
            application_relay: false,
            process_incarnation: [0; 16],
            peer_bindings: HashMap::new(),
            pending: HashMap::new(),
            committed: HashMap::new(),
            admitted: HashMap::new(),
        }
    }

    pub fn enable(&mut self, config: PulseV4Config) -> Result<(), &'static str> {
        config.validate()?;
        let mut incarnation = [0; 16];
        getrandom::fill(&mut incarnation).map_err(|_| "cannot generate Pulse v4 incarnation")?;
        self.config = Some(config);
        self.process_incarnation = incarnation;
        self.peer_bindings.clear();
        self.pending.clear();
        self.committed.clear();
        self.admitted.clear();
        Ok(())
    }

    pub fn set_application_relay(&mut self, enabled: bool) {
        self.application_relay = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_some()
    }

    pub fn frame_exceeded(&self, frame_len: usize) -> bool {
        self.config
            .as_ref()
            .is_some_and(|config| frame_len > config.max_frame_bytes as usize)
    }

    pub fn connected(&mut self, peer: u32) -> Result<(), &'static str> {
        if self.config.is_none() {
            return Ok(());
        }
        let mut binding = [0; 32];
        getrandom::fill(&mut binding).map_err(|_| "cannot generate Pulse v4 peer binding")?;
        self.peer_bindings.insert(peer, binding);
        self.pending.remove(&peer);
        self.committed.remove(&peer);
        self.admitted.remove(&peer);
        Ok(())
    }

    pub fn disconnected(&mut self, peer: u32) {
        self.peer_bindings.remove(&peer);
        self.pending.remove(&peer);
        self.committed.remove(&peer);
        self.admitted.remove(&peer);
    }

    pub fn expire(&mut self, now_ms: i64) {
        self.pending
            .retain(|_, entry| entry.challenge.expires_at_ms > now_ms);
    }

    pub(crate) fn hello(&mut self, peer: u32, hello: PulseV4Hello, now_ms: i64) -> ServerMessage {
        self.expire(now_ms);
        let Some(config) = self.config.as_ref() else {
            return server_result(self.result_for_hello(
                &hello,
                PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability,
                PulseV4RetryClass::PulseV4RetryNone,
                "Pulse v4 is not enabled",
                Vec::new(),
            ));
        };
        let fingerprint = digest(&hello.encode_to_vec());
        if let Some(existing) = self.pending.get(&peer) {
            if existing.hello_fingerprint == fingerprint {
                return server_challenge(existing.challenge.clone());
            }
            return server_result(self.result_for_hello(
                &hello,
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "another handshake is pending",
                Vec::new(),
            ));
        }

        let negotiated = match validate_hello(&hello, config, self.application_relay) {
            Ok(capabilities) => capabilities,
            Err((code, detail)) => {
                return server_result(self.result_for_hello(
                    &hello,
                    code,
                    PulseV4RetryClass::PulseV4RetrySameConnection,
                    detail,
                    Vec::new(),
                ))
            }
        };
        if self.pending.len() >= config.max_pending {
            return server_result(self.result_for_hello(
                &hello,
                PulseV4ErrorCode::PulseV4ErrorCapacity,
                PulseV4RetryClass::PulseV4RetryLater,
                "handshake capacity exhausted",
                Vec::new(),
            ));
        }

        let binding = match self.peer_bindings.get(&peer).copied() {
            Some(binding) => binding,
            None => {
                let mut binding = [0; 32];
                if getrandom::fill(&mut binding).is_err() {
                    return server_result(self.result_for_hello(
                        &hello,
                        PulseV4ErrorCode::PulseV4ErrorInternal,
                        PulseV4RetryClass::PulseV4RetryLater,
                        "challenge generation failed",
                        Vec::new(),
                    ));
                }
                self.peer_bindings.insert(peer, binding);
                binding
            }
        };
        let mut challenge_id = [0; 32];
        let mut challenge_bytes = [0; 32];
        if getrandom::fill(&mut challenge_id).is_err()
            || getrandom::fill(&mut challenge_bytes).is_err()
        {
            return server_result(self.result_for_hello(
                &hello,
                PulseV4ErrorCode::PulseV4ErrorInternal,
                PulseV4RetryClass::PulseV4RetryLater,
                "challenge generation failed",
                Vec::new(),
            ));
        }
        let challenge = PulseV4Challenge {
            request_id: hello.request_id.clone(),
            session_id: hello.session_id.clone(),
            connection_epoch: hello.connection_epoch,
            challenge_id: challenge_id.to_vec(),
            challenge: challenge_bytes.to_vec(),
            audience: config.audience.clone(),
            issuer: config.issuer.clone(),
            process_incarnation: self.process_incarnation.to_vec(),
            peer_binding: binding.to_vec(),
            expires_at_ms: now_ms.saturating_add(config.challenge_ttl_ms as i64),
            negotiated_capabilities: negotiated,
            limits: Some(limits(config)),
        };
        self.pending.insert(
            peer,
            PendingChallenge {
                hello,
                hello_fingerprint: fingerprint,
                challenge: challenge.clone(),
            },
        );
        server_challenge(challenge)
    }

    pub(crate) fn authenticate(
        &mut self,
        peer: u32,
        auth: PulseV4Auth,
        now_ms: i64,
    ) -> V4AuthOutcome {
        let Some(config) = self.config.as_ref() else {
            return V4AuthOutcome::Reply(self.result_for_auth(
                &auth,
                PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability,
                PulseV4RetryClass::PulseV4RetryNone,
                "Pulse v4 is not enabled",
                Vec::new(),
            ));
        };
        let auth_fingerprint = digest(&auth.encode_to_vec());
        if let Some(committed) = self.committed.get(&peer).and_then(|items| {
            items
                .iter()
                .find(|item| item.auth_fingerprint == auth_fingerprint)
        }) {
            return V4AuthOutcome::Reply(committed.result.clone());
        }
        if let Some(committed) = self.committed.get(&peer).and_then(|items| {
            items
                .iter()
                .find(|item| item.challenge_id == auth.challenge_id)
        }) {
            let _ = committed;
            return V4AuthOutcome::Reply(self.result_for_auth(
                &auth,
                PulseV4ErrorCode::PulseV4ErrorAlreadyUsed,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "challenge was already consumed",
                Vec::new(),
            ));
        }

        let Some(pending) = self.pending.remove(&peer) else {
            return V4AuthOutcome::Reply(self.result_for_auth(
                &auth,
                PulseV4ErrorCode::PulseV4ErrorUnknownChallenge,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "unknown challenge",
                Vec::new(),
            ));
        };
        let negotiated = pending.challenge.negotiated_capabilities.clone();
        let reject = |authority: &mut Self,
                      code: PulseV4ErrorCode,
                      retry: PulseV4RetryClass,
                      detail: &'static str| {
            let result = authority.result_for_auth(&auth, code, retry, detail, negotiated.clone());
            authority.commit(
                peer,
                auth_fingerprint,
                auth.challenge_id.clone(),
                result.clone(),
            );
            V4AuthOutcome::Reply(result)
        };
        if pending.challenge.expires_at_ms <= now_ms {
            return reject(
                self,
                PulseV4ErrorCode::PulseV4ErrorExpired,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "challenge expired",
            );
        }
        let Some(wire_chain) = auth.auth_chain.as_ref() else {
            return reject(
                self,
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "auth chain is missing",
            );
        };
        if wire_chain.links.len() > MAX_AUTH_CHAIN_LINKS
            || wire_chain.encoded_len() > config.max_auth_chain_bytes
        {
            return reject(
                self,
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "auth chain exceeds limit",
            );
        }
        let crypto_chain = match decode_v4_auth_chain(wire_chain) {
            Ok(chain) => chain,
            Err(_) => {
                return reject(
                    self,
                    PulseV4ErrorCode::PulseV4ErrorMalformed,
                    PulseV4RetryClass::PulseV4RetrySameConnection,
                    "auth chain is malformed",
                )
            }
        };
        if auth.request_id != pending.hello.request_id
            || auth.session_id != pending.hello.session_id
            || auth.connection_epoch != pending.hello.connection_epoch
            || auth.challenge_id != pending.challenge.challenge_id
            || auth.idempotency_key.len() != 16
            || !normalized_address(&auth.wallet)
            || !normalized_address(&auth.auth_session)
        {
            return reject(
                self,
                PulseV4ErrorCode::PulseV4ErrorIntentMismatch,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "authentication intent does not match challenge",
            );
        }
        let transcript = transcript(&pending.hello, &pending.challenge, &auth);
        let payload = signing_payload(&transcript);
        let verified = match verify_v4_auth_chain(&crypto_chain, &payload, now_ms) {
            Ok(verified) => verified,
            Err(_) => {
                return reject(
                    self,
                    PulseV4ErrorCode::PulseV4ErrorAuthInvalid,
                    PulseV4RetryClass::PulseV4RetrySameConnection,
                    "authentication rejected",
                )
            }
        };
        if verified.user_address != auth.wallet || verified.session != auth.auth_session {
            return reject(
                self,
                PulseV4ErrorCode::PulseV4ErrorIntentMismatch,
                PulseV4RetryClass::PulseV4RetrySameConnection,
                "signed identity does not match claims",
            );
        }
        V4AuthOutcome::Verified(Box::new(VerifiedV4Auth {
            auth,
            hello: pending.hello,
            verified,
            negotiated_capabilities: negotiated,
            auth_fingerprint,
            relay_nonce: pending.challenge.challenge.clone(),
        }))
    }

    pub(crate) fn reject_hello(&self, hello: &PulseV4Hello, detail: &'static str) -> ServerMessage {
        server_result(self.result_for_hello(
            hello,
            PulseV4ErrorCode::PulseV4ErrorInvalidHandshakeField,
            PulseV4RetryClass::PulseV4RetrySameConnection,
            detail,
            Vec::new(),
        ))
    }

    pub(crate) fn success_result(&self, verified: &VerifiedV4Auth) -> PulseV4Result {
        self.result_for_auth(
            &verified.auth,
            PulseV4ErrorCode::PulseV4ErrorNone,
            PulseV4RetryClass::PulseV4RetryNone,
            "",
            verified.negotiated_capabilities.clone(),
        )
    }

    pub(crate) fn rejection_result(
        &self,
        verified: &VerifiedV4Auth,
        code: PulseV4ErrorCode,
        retry: PulseV4RetryClass,
        detail: &'static str,
    ) -> PulseV4Result {
        self.result_for_auth(
            &verified.auth,
            code,
            retry,
            detail,
            verified.negotiated_capabilities.clone(),
        )
    }

    pub(crate) fn admit(&mut self, peer: u32, verified: &VerifiedV4Auth) {
        self.admitted.insert(
            peer,
            (
                verified.hello.session_id.clone(),
                verified.hello.connection_epoch,
            ),
        );
    }

    pub(crate) fn holds_this_session_at_or_after(
        &self,
        holder: u32,
        verified: &VerifiedV4Auth,
    ) -> bool {
        self.admitted.get(&holder).is_some_and(|(session, epoch)| {
            *session == verified.hello.session_id && *epoch >= verified.hello.connection_epoch
        })
    }

    pub(crate) fn commit_verified(
        &mut self,
        peer: u32,
        verified: &VerifiedV4Auth,
        result: PulseV4Result,
    ) {
        self.commit(
            peer,
            verified.auth_fingerprint,
            verified.auth.challenge_id.clone(),
            result,
        );
    }

    fn commit(
        &mut self,
        peer: u32,
        auth_fingerprint: [u8; 32],
        challenge_id: Vec<u8>,
        result: PulseV4Result,
    ) {
        let entries = self.committed.entry(peer).or_default();
        if entries.len() == MAX_COMMITTED_PER_PEER {
            entries.remove(0);
        }
        entries.push(CommittedResult {
            auth_fingerprint,
            challenge_id,
            result,
        });
    }

    fn result_for_hello(
        &self,
        hello: &PulseV4Hello,
        code: PulseV4ErrorCode,
        retry: PulseV4RetryClass,
        detail: &str,
        negotiated_capabilities: Vec<String>,
    ) -> PulseV4Result {
        self.result(
            hello.request_id.clone(),
            hello.session_id.clone(),
            hello.connection_epoch,
            Vec::new(),
            code,
            retry,
            detail,
            negotiated_capabilities,
        )
    }

    fn result_for_auth(
        &self,
        auth: &PulseV4Auth,
        code: PulseV4ErrorCode,
        retry: PulseV4RetryClass,
        detail: &str,
        negotiated_capabilities: Vec<String>,
    ) -> PulseV4Result {
        self.result(
            auth.request_id.clone(),
            auth.session_id.clone(),
            auth.connection_epoch,
            auth.idempotency_key.clone(),
            code,
            retry,
            detail,
            negotiated_capabilities,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn result(
        &self,
        request_id: Vec<u8>,
        session_id: Vec<u8>,
        connection_epoch: u64,
        idempotency_key: Vec<u8>,
        code: PulseV4ErrorCode,
        retry: PulseV4RetryClass,
        detail: &str,
        negotiated_capabilities: Vec<String>,
    ) -> PulseV4Result {
        let config = self.config.as_ref();
        let max_detail = config
            .map(|config| config.max_public_detail_bytes)
            .unwrap_or(DEFAULT_MAX_PUBLIC_DETAIL_BYTES);
        PulseV4Result {
            request_id,
            session_id,
            connection_epoch,
            idempotency_key,
            success: code == PulseV4ErrorCode::PulseV4ErrorNone,
            error_code: code as i32,
            retry: retry as i32,
            public_detail: bounded_detail(detail, max_detail),
            negotiated_capabilities,
            limits: config.map(limits),
            application_relay_nonce: Vec::new(),
        }
    }
}

pub fn transcript(
    hello: &PulseV4Hello,
    challenge: &PulseV4Challenge,
    auth: &PulseV4Auth,
) -> PulseV4AuthTranscript {
    PulseV4AuthTranscript {
        protocol_version: PROTOCOL_VERSION,
        audience: challenge.audience.clone(),
        issuer: challenge.issuer.clone(),
        process_incarnation: challenge.process_incarnation.clone(),
        peer_binding: challenge.peer_binding.clone(),
        challenge_id: challenge.challenge_id.clone(),
        challenge: challenge.challenge.clone(),
        expires_at_ms: challenge.expires_at_ms,
        request_id: hello.request_id.clone(),
        session_id: hello.session_id.clone(),
        connection_epoch: hello.connection_epoch,
        required_capabilities: hello.required_capabilities.clone(),
        optional_capabilities: hello.optional_capabilities.clone(),
        negotiated_capabilities: challenge.negotiated_capabilities.clone(),
        role: hello.role,
        profile_version: hello.profile_version,
        initial_state: hello.initial_state.clone(),
        listener_aoi: hello.listener_aoi.clone(),
        idempotency_key: auth.idempotency_key.clone(),
        wallet: auth.wallet.clone(),
        auth_session: auth.auth_session.clone(),
    }
}

pub fn signing_payload(transcript: &PulseV4AuthTranscript) -> String {
    let digest = Sha256::digest(transcript.encode_to_vec());
    format!("{SIGNING_DOMAIN}{}", lower_hex(&digest))
}

fn validate_hello(
    hello: &PulseV4Hello,
    config: &PulseV4Config,
    application_relay: bool,
) -> Result<Vec<String>, (PulseV4ErrorCode, &'static str)> {
    if hello.request_id.len() != 16 || hello.session_id.len() != 16 || hello.connection_epoch == 0 {
        return Err((
            PulseV4ErrorCode::PulseV4ErrorMalformed,
            "invalid correlation fields",
        ));
    }
    match crate::decentraland::pulse::PulseV4Role::try_from(hello.role) {
        Ok(crate::decentraland::pulse::PulseV4Role::Player) if !hello.listener_aoi.is_empty() => {
            return Err((
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                "player intent includes listener fields",
            ));
        }
        Ok(crate::decentraland::pulse::PulseV4Role::Listener)
            if hello.initial_state.is_some() || hello.listener_aoi.is_empty() =>
        {
            return Err((
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                "listener intent fields are invalid",
            ));
        }
        Ok(_) => {}
        Err(_) => {
            return Err((
                PulseV4ErrorCode::PulseV4ErrorMalformed,
                "unknown connection role",
            ))
        }
    }
    let total = hello.required_capabilities.len() + hello.optional_capabilities.len();
    if total > config.max_capabilities
        || !sorted_unique(&hello.required_capabilities)
        || !sorted_unique(&hello.optional_capabilities)
        || hello
            .required_capabilities
            .iter()
            .any(|cap| hello.optional_capabilities.binary_search(cap).is_ok())
        || hello
            .required_capabilities
            .iter()
            .chain(&hello.optional_capabilities)
            .any(|cap| cap.is_empty() || cap.len() > 64 || !cap.is_ascii())
    {
        return Err((
            PulseV4ErrorCode::PulseV4ErrorMalformed,
            "invalid capability list",
        ));
    }
    if hello.required_capabilities.iter().any(|cap| {
        !(SUPPORTED_CAPABILITIES.contains(&cap.as_str())
            || (application_relay && cap.as_str() == CAPABILITY_APPLICATION_RELAY))
    }) {
        return Err((
            PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability,
            "required capability is unsupported",
        ));
    }
    let mut negotiated: Vec<String> = hello
        .required_capabilities
        .iter()
        .chain(&hello.optional_capabilities)
        .filter(|cap| {
            SUPPORTED_CAPABILITIES.contains(&cap.as_str())
                || (application_relay && cap.as_str() == CAPABILITY_APPLICATION_RELAY)
        })
        .cloned()
        .collect();
    negotiated.sort();
    negotiated.dedup();
    let carries_sample_ticks = negotiated.iter().any(|cap| {
        cap == CAPABILITY_DELTA_BATCH_BASELINE || cap == CAPABILITY_DELTA_BATCH_DICTIONARY
    });
    if !carries_sample_ticks {
        if hello
            .required_capabilities
            .iter()
            .any(|cap| cap == CAPABILITY_DELTA_BATCH_SAMPLE_TICK)
        {
            return Err((
                PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability,
                "required capability is unsupported",
            ));
        }
        negotiated.retain(|cap| cap != CAPABILITY_DELTA_BATCH_SAMPLE_TICK);
    }
    Ok(negotiated)
}

fn normalized_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value
            .as_bytes()
            .iter()
            .skip(2)
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn bounded_detail(detail: &str, max_bytes: usize) -> String {
    if detail.len() <= max_bytes {
        return detail.to_string();
    }
    let mut end = max_bytes;
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail[..end].to_string()
}

fn limits(config: &PulseV4Config) -> PulseV4Limits {
    PulseV4Limits {
        max_frame_bytes: config.max_frame_bytes,
        max_pending_handshakes: config.max_pending as u32,
        handshake_deadline_ms: config.challenge_ttl_ms,
        max_auth_chain_bytes: config.max_auth_chain_bytes as u32,
        max_capabilities: config.max_capabilities as u32,
        max_public_detail_bytes: config.max_public_detail_bytes as u32,
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn server_result(result: PulseV4Result) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::V4Result(result)),
    }
}

fn server_challenge(challenge: PulseV4Challenge) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::V4Challenge(challenge)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decentraland::common::{
        AuthChain as ProtoAuthChain, AuthLink as ProtoAuthLink, AuthLinkType as ProtoAuthLinkType,
    };
    use crate::decentraland::pulse::{server_message, PulseV4Role};
    use catalyrst_crypto::Wallet;
    use catalyrst_types::{AuthChain, AuthLink, AuthLinkType};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";

    fn config(issuer: &str) -> PulseV4Config {
        PulseV4Config {
            audience: "https://realm.example".into(),
            issuer: issuer.into(),
            challenge_ttl_ms: 15_000,
            max_pending: 8,
            max_auth_chain_bytes: DEFAULT_MAX_AUTH_CHAIN_BYTES,
            max_capabilities: DEFAULT_MAX_CAPABILITIES,
            max_public_detail_bytes: DEFAULT_MAX_PUBLIC_DETAIL_BYTES,
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
        }
    }

    fn hello() -> PulseV4Hello {
        PulseV4Hello {
            request_id: vec![1; 16],
            session_id: vec![2; 16],
            connection_epoch: 7,
            required_capabilities: Vec::new(),
            optional_capabilities: vec![CAPABILITY_DELTA_BATCH.into()],
            role: PulseV4Role::Player as i32,
            profile_version: 9,
            initial_state: None,
            listener_aoi: Vec::new(),
        }
    }

    fn challenge(message: ServerMessage) -> PulseV4Challenge {
        match message.message.unwrap() {
            server_message::Message::V4Challenge(challenge) => challenge,
            other => panic!("expected v4 challenge, got {other:?}"),
        }
    }

    fn proto_chain(chain: AuthChain) -> ProtoAuthChain {
        ProtoAuthChain {
            links: chain
                .into_iter()
                .map(|link| ProtoAuthLink {
                    r#type: match link.link_type {
                        AuthLinkType::SIGNER => ProtoAuthLinkType::Signer,
                        AuthLinkType::EcdsaEphemeral => ProtoAuthLinkType::EcdsaEphemeral,
                        AuthLinkType::EcdsaSignedEntity => ProtoAuthLinkType::EcdsaSignedEntity,
                        AuthLinkType::EcdsaEip1654Ephemeral => {
                            ProtoAuthLinkType::EcdsaEip1654Ephemeral
                        }
                        AuthLinkType::EcdsaEip1654SignedEntity => {
                            ProtoAuthLinkType::EcdsaEip1654SignedEntity
                        }
                    } as i32,
                    payload: link.payload,
                    signature: link.signature,
                })
                .collect(),
        }
    }

    fn signed_chain(wallet: &Wallet, payload: &str) -> AuthChain {
        vec![
            AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: wallet.address(),
                signature: None,
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaSignedEntity,
                payload: payload.into(),
                signature: Some(wallet.sign_message(payload.as_bytes()).unwrap()),
            },
        ]
    }

    fn signed_auth(hello: &PulseV4Hello, challenge: &PulseV4Challenge) -> PulseV4Auth {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let mut auth = PulseV4Auth {
            request_id: hello.request_id.clone(),
            session_id: hello.session_id.clone(),
            connection_epoch: hello.connection_epoch,
            challenge_id: challenge.challenge_id.clone(),
            auth_chain: None,
            idempotency_key: vec![3; 16],
            wallet: wallet.address(),
            auth_session: wallet.address(),
        };
        let payload = signing_payload(&transcript(hello, challenge, &auth));
        let chain = signed_chain(&wallet, &payload);
        auth.auth_chain = Some(proto_chain(chain));
        auth
    }

    #[test]
    fn application_capability_requires_live_authority_and_reuses_authenticated_challenge() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let mut intent = hello();
        intent.required_capabilities = vec![CAPABILITY_APPLICATION_RELAY.into()];
        intent.optional_capabilities.clear();
        let rejected = authority.hello(11, intent.clone(), 1_000);
        assert!(
            matches!(rejected.message, Some(server_message::Message::V4Result(result))
            if result.error_code == PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability as i32)
        );
        authority.set_application_relay(true);
        let challenge = challenge(authority.hello(11, intent.clone(), 1_000));
        let auth = signed_auth(&intent, &challenge);
        let verified = match authority.authenticate(11, auth.clone(), 1_001) {
            V4AuthOutcome::Verified(verified) => verified,
            other => panic!("expected verified auth, got {other:?}"),
        };
        assert_eq!(verified.relay_nonce, challenge.challenge);
        let result = authority.success_result(&verified);
        assert!(result.application_relay_nonce.is_empty());
        assert!(result
            .negotiated_capabilities
            .iter()
            .any(|cap| cap == CAPABILITY_APPLICATION_RELAY));
        authority.commit_verified(11, &verified, result.clone());
        assert!(
            matches!(authority.authenticate(11, auth, 1_002), V4AuthOutcome::Reply(replay) if replay == result)
        );
    }

    #[test]
    fn signed_handshake_is_correlated_and_idempotent_on_its_peer() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let hello = hello();
        let challenge = challenge(authority.hello(11, hello.clone(), 1_000));
        assert_eq!(challenge.challenge.len(), 32);
        assert_eq!(challenge.challenge_id.len(), 32);
        assert_eq!(challenge.peer_binding.len(), 32);
        assert_eq!(challenge.process_incarnation.len(), 16);
        let auth = signed_auth(&hello, &challenge);

        let verified = match authority.authenticate(11, auth.clone(), 1_001) {
            V4AuthOutcome::Verified(verified) => verified,
            other => panic!("expected verified auth, got {other:?}"),
        };
        let result = authority.success_result(&verified);
        authority.commit_verified(11, &verified, result.clone());

        match authority.authenticate(11, auth, 1_002) {
            V4AuthOutcome::Reply(replayed) => assert_eq!(replayed, result),
            other => panic!("expected committed reply, got {other:?}"),
        }
    }

    #[test]
    fn captured_auth_is_rejected_by_another_peer_replica_and_restart() {
        let hello = hello();
        let mut issuing = PulseV4Authority::disabled();
        issuing.enable(config("replica-a")).unwrap();
        issuing.connected(11).unwrap();
        issuing.connected(12).unwrap();
        let challenge = challenge(issuing.hello(11, hello.clone(), 1_000));
        let auth = signed_auth(&hello, &challenge);

        for outcome in [
            issuing.authenticate(12, auth.clone(), 1_001),
            {
                let mut replica = PulseV4Authority::disabled();
                replica.enable(config("replica-b")).unwrap();
                replica.connected(11).unwrap();
                replica.authenticate(11, auth.clone(), 1_001)
            },
            {
                let mut restarted = PulseV4Authority::disabled();
                restarted.enable(config("replica-a")).unwrap();
                restarted.connected(11).unwrap();
                restarted.authenticate(11, auth.clone(), 1_001)
            },
        ] {
            match outcome {
                V4AuthOutcome::Reply(result) => assert_eq!(
                    result.error_code,
                    PulseV4ErrorCode::PulseV4ErrorUnknownChallenge as i32
                ),
                other => panic!("captured auth was accepted: {other:?}"),
            }
        }
    }

    #[test]
    fn required_capability_rejects_without_allocating_a_challenge() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let mut hello = hello();
        hello.required_capabilities = vec!["future_feature".into()];
        hello.optional_capabilities.clear();
        let message = authority.hello(11, hello, 1_000);
        let result = match message.message.unwrap() {
            server_message::Message::V4Result(result) => result,
            other => panic!("expected result, got {other:?}"),
        };
        assert_eq!(
            result.error_code,
            PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability as i32
        );
        assert!(authority.pending.is_empty());
    }

    #[test]
    fn required_sample_ticks_without_an_arm_11_codec_are_refused() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let mut hello = hello();
        hello.required_capabilities = vec![CAPABILITY_DELTA_BATCH_SAMPLE_TICK.into()];
        let message = authority.hello(11, hello.clone(), 1_000);
        let result = match message.message.unwrap() {
            server_message::Message::V4Result(result) => result,
            other => panic!("expected result, got {other:?}"),
        };
        assert_eq!(
            result.error_code,
            PulseV4ErrorCode::PulseV4ErrorUnsupportedCapability as i32
        );
        assert!(authority.pending.is_empty());

        hello.request_id = vec![3; 16];
        hello.optional_capabilities = vec![CAPABILITY_DELTA_BATCH_BASELINE.into()];
        let granted = challenge(authority.hello(11, hello, 1_001));
        assert_eq!(
            granted.negotiated_capabilities,
            [
                CAPABILITY_DELTA_BATCH_BASELINE,
                CAPABILITY_DELTA_BATCH_SAMPLE_TICK
            ]
        );
    }

    #[test]
    fn expired_challenge_is_consumed_before_authentication() {
        let mut cfg = config("replica-a");
        cfg.challenge_ttl_ms = 5;
        let mut authority = PulseV4Authority::disabled();
        authority.enable(cfg).unwrap();
        authority.connected(11).unwrap();
        let hello = hello();
        let challenge = challenge(authority.hello(11, hello.clone(), 1_000));
        let auth = signed_auth(&hello, &challenge);
        match authority.authenticate(11, auth.clone(), 1_006) {
            V4AuthOutcome::Reply(result) => assert_eq!(
                result.error_code,
                PulseV4ErrorCode::PulseV4ErrorExpired as i32
            ),
            other => panic!("expected expiry, got {other:?}"),
        }
        match authority.authenticate(11, auth, 1_007) {
            V4AuthOutcome::Reply(result) => assert_eq!(
                result.error_code,
                PulseV4ErrorCode::PulseV4ErrorExpired as i32
            ),
            other => panic!("expected identical committed expiry, got {other:?}"),
        }
    }

    #[test]
    fn unknown_auth_link_type_is_rejected_as_malformed_before_verification() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let hello = hello();
        let challenge = challenge(authority.hello(11, hello.clone(), 1_000));
        let mut auth = signed_auth(&hello, &challenge);
        auth.auth_chain.as_mut().unwrap().links[0].r#type = 99;
        match authority.authenticate(11, auth, 1_001) {
            V4AuthOutcome::Reply(result) => assert_eq!(
                result.error_code,
                PulseV4ErrorCode::PulseV4ErrorMalformed as i32
            ),
            other => panic!("expected malformed result, got {other:?}"),
        }
    }

    #[test]
    fn auth_chain_link_count_is_rejected_before_verification() {
        let mut authority = PulseV4Authority::disabled();
        authority.enable(config("replica-a")).unwrap();
        authority.connected(11).unwrap();
        let hello = hello();
        let challenge = challenge(authority.hello(11, hello.clone(), 1_000));
        let mut auth = signed_auth(&hello, &challenge);
        let link = auth.auth_chain.as_ref().unwrap().links[0].clone();
        auth.auth_chain.as_mut().unwrap().links = vec![link; MAX_AUTH_CHAIN_LINKS + 1];
        match authority.authenticate(11, auth, 1_001) {
            V4AuthOutcome::Reply(result) => assert_eq!(
                result.error_code,
                PulseV4ErrorCode::PulseV4ErrorMalformed as i32
            ),
            other => panic!("expected malformed result, got {other:?}"),
        }
    }

    #[test]
    fn auth_chain_encoded_size_is_rejected_before_verification() {
        let mut cfg = config("replica-a");
        cfg.max_auth_chain_bytes = 32;
        let mut authority = PulseV4Authority::disabled();
        authority.enable(cfg).unwrap();
        authority.connected(11).unwrap();
        let hello = hello();
        let challenge = challenge(authority.hello(11, hello.clone(), 1_000));
        let auth = signed_auth(&hello, &challenge);
        assert!(auth.auth_chain.as_ref().unwrap().encoded_len() > 32);
        match authority.authenticate(11, auth, 1_001) {
            V4AuthOutcome::Reply(result) => assert_eq!(
                result.error_code,
                PulseV4ErrorCode::PulseV4ErrorMalformed as i32
            ),
            other => panic!("expected malformed result, got {other:?}"),
        }
    }
}
