/// Base-emote classification from @dcl/schemas 27.2.0 `isBaseEmote`.
pub fn is_base_emote(urn: &str) -> bool {
    let parts: Vec<_> = urn.split(':').collect();
    parts.len() == 5
        && parts[1] == "decentraland"
        && matches!(parts[3], "base-emotes" | "base-scene-emotes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_27_2_base_emote_classification() {
        for collection in ["base-emotes", "base-scene-emotes"] {
            assert!(is_base_emote(&format!(
                "urn:decentraland:off-chain:{collection}:wave"
            )));
        }
        for urn in [
            "urn:decentraland:off-chain:base-avatars:wave",
            "urn:decentraland:off-chain:base-scene-emotes-fake:wave",
            "urn:decentraland:off-chain:base-scene-emotes:wave:extra",
            "urn:other:off-chain:base-scene-emotes:wave",
            "invalid",
        ] {
            assert!(!is_base_emote(urn), "{urn}");
        }
    }
}
