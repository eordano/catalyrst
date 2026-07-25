use std::collections::HashMap;

use crate::types::*;

pub(super) fn is_ipfs_v2_hash(hash: &str) -> bool {
    hash.starts_with("bafy") || hash.starts_with("bafk")
}

pub(super) fn should_validate_face_thumbnail(
    entity: &Entity,
    files: &HashMap<String, Vec<u8>>,
) -> bool {
    let ts = entity.timestamp;
    if (adr_timestamps::ADR_45..adr_timestamps::ADR_290_OPTIONAL).contains(&ts) {
        return true;
    }
    if (adr_timestamps::ADR_290_OPTIONAL..adr_timestamps::ADR_290_REJECTED).contains(&ts) {
        let has_content = !entity.content.is_empty();
        let has_files = files.len() > 1;
        let has_snapshots = entity
            .metadata
            .as_ref()
            .and_then(|m| m.get("avatars"))
            .and_then(|a| a.as_array())
            .map(|avatars| {
                avatars
                    .iter()
                    .any(|a| a.get("avatar").and_then(|av| av.get("snapshots")).is_some())
            })
            .unwrap_or(false);
        return has_content || has_files || has_snapshots;
    }
    false
}

pub(super) fn validate_profile_wearable_urns(
    metadata: &serde_json::Value,
    timestamp: Timestamp,
) -> Result<(), String> {
    if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
        for avatar in avatars {
            if let Some(wearables) = avatar
                .get("avatar")
                .and_then(|a| a.get("wearables"))
                .and_then(|w| w.as_array())
            {
                for w in wearables {
                    if let Some(pointer) = w.as_str() {
                        if is_old_emote(pointer) {
                            continue;
                        }
                        if !pointer.starts_with("urn:") && !pointer.starts_with("dcl://") {
                            return Err(format!(
                                "Each profile wearable pointer should be a urn, for example \
                                 (urn:decentraland:{{protocol}}:collections-v2:\
                                 {{contract(0x[a-fA-F0-9]+)}}:{{name}}). Invalid pointer: ({pointer})"
                            ));
                        }

                        if timestamp >= adr_timestamps::ADR_244 {
                            let lower = pointer.to_lowercase();
                            let is_asset = (lower.contains("collections-v1:")
                                || lower.contains("collections-v2:"))
                                && count_urn_segments(&lower) <= 6;
                            if is_asset {
                                return Err(format!(
                                    "Wearable pointer {pointer} should be an item, not an asset. \
                                     The URN must include the tokenId."
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate_profile_emote_urns(
    metadata: &serde_json::Value,
    _timestamp: Timestamp,
) -> Result<(), String> {
    if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
        for avatar in avatars {
            if let Some(emotes) = avatar
                .get("avatar")
                .and_then(|a| a.get("emotes"))
                .and_then(|e| e.as_array())
            {
                for emote in emotes {
                    if let Some(urn) = emote.get("urn").and_then(|u| u.as_str()) {
                        if is_old_emote(urn) {
                            continue;
                        }
                        if !urn.starts_with("urn:") && !urn.starts_with("dcl://") {
                            return Err(format!(
                                "Each profile emote pointer should be a urn, for example \
                                 (urn:decentraland:{{protocol}}:collections-v2:\
                                 {{contract(0x[a-fA-F0-9]+)}}:{{name}}). Invalid pointer: ({urn})"
                            ));
                        }
                    }

                    if let Some(slot) = emote.get("slot").and_then(|s| s.as_i64()) {
                        if !(0..=9).contains(&slot) {
                            return Err(format!(
                                "The slot {slot} of the emote must be a number between \
                                 0 and 9 (inclusive)."
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn profile_has_emotes(metadata: &serde_json::Value) -> bool {
    if let Some(avatars) = metadata.get("avatars").and_then(|v| v.as_array()) {
        for avatar in avatars {
            if avatar.get("avatar").and_then(|a| a.get("emotes")).is_some() {
                return true;
            }
        }
    }
    false
}

fn is_old_emote(s: &str) -> bool {
    s.len() <= 20 && s.chars().all(|c| c.is_ascii_alphabetic())
}

pub(super) fn count_urn_segments(urn: &str) -> usize {
    urn.split(':').count()
}
