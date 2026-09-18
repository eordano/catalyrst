use crate::proto::archipelago::{ConnectionOffer, ConnectionSelection};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const PROTOCOL_VERSION: u32 = 4;
pub const FEATURE_TYPED_AUTH: u64 = 1;
pub const FEATURE_LANE_CONTEXT: u64 = 2;
pub const FEATURE_REQUEST_SEQUENCE: u64 = 4;
pub const FEATURE_REVISION_SNAPSHOTS: u64 = 8;
pub const FEATURE_INHERIT_CONTEXT: u64 = 16;
pub const CORE_FEATURES: u64 = 15;
pub const KNOWN_FEATURES: u64 = CORE_FEATURES | FEATURE_INHERIT_CONTEXT;
pub const MAX_LANES: usize = 16;
pub const MAX_STRING_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptError {
    #[error("unsupported protocol version")]
    Version,
    #[error("unsupported or inconsistent feature selection")]
    Features,
    #[error("invalid nonce or identifier length")]
    Identifier,
    #[error("invalid lane binding")]
    Lanes,
    #[error("invalid authenticated connection context")]
    Context,
    #[error("invalid connection limits")]
    Limits,
    #[error("invalid wallet address")]
    Address,
}

pub fn validate_offer(offer: &ConnectionOffer, supported: u64) -> Result<(), TranscriptError> {
    if offer.version != PROTOCOL_VERSION {
        return Err(TranscriptError::Version);
    }
    if offer.required_features & !offer.offered_features != 0
        || offer.required_features & !(supported & KNOWN_FEATURES) != 0
        || offer.required_features & CORE_FEATURES != CORE_FEATURES
    {
        return Err(TranscriptError::Features);
    }
    if offer.client_nonce.len() != 16 || offer.session_id.len() != 20 {
        return Err(TranscriptError::Identifier);
    }
    if offer.lanes.is_empty() || offer.lanes.len() > MAX_LANES {
        return Err(TranscriptError::Lanes);
    }
    let mut seen = HashSet::new();
    for (index, lane) in offer.lanes.iter().enumerate() {
        if !valid_lane(lane) || !seen.insert(lane) || (lane == "realm" && index != 0) {
            return Err(TranscriptError::Lanes);
        }
    }
    Ok(())
}

pub fn validate_selection(
    offer: &ConnectionOffer,
    selection: &ConnectionSelection,
    supported: u64,
) -> Result<(), TranscriptError> {
    validate_offer(offer, supported)?;
    if selection.version != PROTOCOL_VERSION {
        return Err(TranscriptError::Version);
    }
    if selection.selected_features & !(offer.offered_features & supported & KNOWN_FEATURES) != 0
        || offer.required_features & !selection.selected_features != 0
    {
        return Err(TranscriptError::Features);
    }
    let context = selection.context.as_ref().ok_or(TranscriptError::Context)?;
    if context.server_nonce.len() != 32
        || context.process_incarnation.len() != 16
        || context.connection_id.len() != 16
    {
        return Err(TranscriptError::Identifier);
    }
    if !valid_string(&context.audience, MAX_STRING_BYTES)
        || !valid_string(&context.issuer, MAX_STRING_BYTES)
        || !valid_string(&context.authority_incarnation, 128)
        || context.connection_epoch == 0
        || context.expires_at_ms == 0
    {
        return Err(TranscriptError::Context);
    }
    let limits = selection.limits.as_ref().ok_or(TranscriptError::Limits)?;
    if limits.max_frame_bytes == 0
        || limits.max_lanes == 0
        || limits.max_lanes as usize > MAX_LANES
        || (limits.max_lanes as usize) < offer.lanes.len()
        || limits.max_pending_requests == 0
        || limits.max_retained_responses == 0
        || limits.max_retained_response_bytes == 0
        || limits.handshake_timeout_ms == 0
        || limits.handshake_timeout_ms > 30_000
        || limits.max_public_detail_bytes == 0
        || limits.max_public_detail_bytes > MAX_STRING_BYTES as u32
    {
        return Err(TranscriptError::Limits);
    }
    if selection.lanes.len() != offer.lanes.len()
        || selection.lanes.iter().enumerate().any(|(index, binding)| {
            let offset = u32::from(offer.lanes[0] != "realm");
            binding.handle != index as u32 + offset || binding.name != offer.lanes[index]
        })
    {
        return Err(TranscriptError::Lanes);
    }
    Ok(())
}

pub fn canonical_preimage(
    address: &str,
    offer: &ConnectionOffer,
    selection: &ConnectionSelection,
) -> Result<Vec<u8>, TranscriptError> {
    validate_selection(offer, selection, KNOWN_FEATURES)?;
    let address = decode_address(address)?;
    let context = selection.context.as_ref().ok_or(TranscriptError::Context)?;
    let limits = selection.limits.as_ref().ok_or(TranscriptError::Limits)?;
    let mut bytes = b"dcl-archipelago-v4\0".to_vec();
    u32_be(&mut bytes, 1);
    bytes.extend_from_slice(&address);
    u32_be(&mut bytes, offer.version);
    u64_be(&mut bytes, offer.offered_features);
    u64_be(&mut bytes, offer.required_features);
    field(&mut bytes, &offer.client_nonce);
    field(&mut bytes, &offer.session_id);
    u32_be(&mut bytes, offer.lanes.len() as u32);
    for lane in &offer.lanes {
        field(&mut bytes, lane.as_bytes());
    }
    u32_be(&mut bytes, selection.version);
    u64_be(&mut bytes, selection.selected_features);
    field(&mut bytes, &context.server_nonce);
    field(&mut bytes, context.audience.as_bytes());
    field(&mut bytes, context.issuer.as_bytes());
    field(&mut bytes, &context.process_incarnation);
    field(&mut bytes, &context.connection_id);
    u64_be(&mut bytes, context.connection_epoch);
    u64_be(&mut bytes, context.expires_at_ms);
    field(&mut bytes, context.authority_incarnation.as_bytes());
    for limit in [
        limits.max_frame_bytes,
        limits.max_lanes,
        limits.max_pending_requests,
        limits.max_retained_responses,
        limits.max_retained_response_bytes,
        limits.handshake_timeout_ms,
        limits.max_public_detail_bytes,
    ] {
        u32_be(&mut bytes, limit);
    }
    u32_be(&mut bytes, selection.lanes.len() as u32);
    for lane in &selection.lanes {
        u32_be(&mut bytes, lane.handle);
        field(&mut bytes, lane.name.as_bytes());
    }
    Ok(bytes)
}

pub fn canonical_challenge(
    address: &str,
    offer: &ConnectionOffer,
    selection: &ConnectionSelection,
) -> Result<String, TranscriptError> {
    let digest = Sha256::digest(canonical_preimage(address, offer, selection)?);
    use std::fmt::Write;
    let mut challenge = String::with_capacity(68);
    challenge.push_str("dcl-");
    for byte in digest {
        write!(challenge, "{byte:02x}").expect("writing a string cannot fail");
    }
    Ok(challenge)
}

pub fn decode_address(address: &str) -> Result<[u8; 20], TranscriptError> {
    let raw = address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"))
        .ok_or(TranscriptError::Address)?;
    if raw.len() != 40 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(TranscriptError::Address);
    }
    let mut bytes = [0; 20];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16)
            .map_err(|_| TranscriptError::Address)?;
    }
    Ok(bytes)
}

fn valid_string(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_lane(value: &str) -> bool {
    crate::control_v4::LaneKey::parse(value).is_ok()
}

fn u32_be(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn u64_be(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn field(bytes: &mut Vec<u8>, value: &[u8]) {
    u32_be(bytes, value.len() as u32);
    bytes.extend_from_slice(value);
}

#[cfg(test)]
#[path = "control_extensions_tests.rs"]
mod tests;
