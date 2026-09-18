use catalyrst_types::{AuthChain, AuthLinkType};

/// The address that signed the last link of a verified auth chain, lower-cased: the ephemeral
/// (per-device) address when the chain delegates, the wallet itself when it does not. Two devices
/// of one wallet produce different keys, one device's reconnects produce the same one, which is
/// what the session-addressed island feed is routed on.
pub fn session_key_of(chain: &AuthChain, fallback: &str) -> String {
    chain
        .iter()
        .rev()
        .find(|link| {
            matches!(
                link.link_type,
                AuthLinkType::EcdsaEphemeral | AuthLinkType::EcdsaEip1654Ephemeral
            )
        })
        .and_then(|link| {
            catalyrst_crypto::auth_chain::parse_ephemeral_payload(&link.payload)
                .ok()
                .map(|(_, ephemeral, _)| ephemeral.trim().to_ascii_lowercase())
        })
        .or_else(|| {
            chain
                .first()
                .map(|link| link.payload.trim().to_ascii_lowercase())
        })
        .unwrap_or_else(|| fallback.to_ascii_lowercase())
}

/// A session key as the feed's subjects carry it. A malformed one must never become a subject
/// token, and a payload that is not one comes from a peer whose device cannot be told apart.
pub fn is_session_key(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The claimed address in a ChallengeRequest, lower-cased. Upstream validates it with
/// `EthAddress.validate` before it issues a challenge; the same shape is what a session key is
/// held to, so one predicate answers both.
pub fn is_address(value: &str) -> bool {
    is_session_key(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(links: &[(AuthLinkType, &str)]) -> AuthChain {
        links
            .iter()
            .map(|(link_type, payload)| catalyrst_types::AuthLink {
                link_type: *link_type,
                payload: (*payload).to_string(),
                signature: None,
            })
            .collect()
    }

    const EPHEMERAL_PAYLOAD: &str = "Decentraland Login\nEphemeral address: 0xAAAA00000000000000000000000000000000BBBB\nExpiration: 2026-12-31T00:00:00.000Z";

    #[test]
    fn a_delegating_chain_names_the_ephemeral_address() {
        let chain = chain(&[
            (
                AuthLinkType::SIGNER,
                "0x1111111111111111111111111111111111111111",
            ),
            (AuthLinkType::EcdsaEphemeral, EPHEMERAL_PAYLOAD),
        ]);
        assert_eq!(
            session_key_of(&chain, "0xfallback"),
            "0xaaaa00000000000000000000000000000000bbbb"
        );
    }

    #[test]
    fn an_undelegated_chain_names_the_wallet() {
        let chain = chain(&[(
            AuthLinkType::SIGNER,
            "0x1111111111111111111111111111111111111111",
        )]);
        assert_eq!(
            session_key_of(&chain, "0xfallback"),
            "0x1111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn the_last_delegation_wins_over_an_earlier_one() {
        let earlier = "Decentraland Login\nEphemeral address: 0xCCCC00000000000000000000000000000000DDDD\nExpiration: 2026-12-31T00:00:00.000Z";
        let chain = chain(&[
            (
                AuthLinkType::SIGNER,
                "0x1111111111111111111111111111111111111111",
            ),
            (AuthLinkType::EcdsaEphemeral, earlier),
            (AuthLinkType::EcdsaEip1654Ephemeral, EPHEMERAL_PAYLOAD),
        ]);
        assert_eq!(
            session_key_of(&chain, "0xfallback"),
            "0xaaaa00000000000000000000000000000000bbbb"
        );
    }

    #[test]
    fn an_unparseable_delegation_falls_back_to_the_signer() {
        let chain = chain(&[
            (
                AuthLinkType::SIGNER,
                "0x1111111111111111111111111111111111111111",
            ),
            (AuthLinkType::EcdsaEphemeral, "not an ephemeral payload"),
        ]);
        assert_eq!(
            session_key_of(&chain, "0xfallback"),
            "0x1111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn a_chain_whose_only_link_is_an_unparseable_delegation_yields_no_usable_session_key() {
        let chain = chain(&[(AuthLinkType::EcdsaEphemeral, "not an ephemeral payload")]);
        let key = session_key_of(&chain, "0xfallback");
        assert_eq!(key, "not an ephemeral payload");
        assert!(
            !is_session_key(&key),
            "a payload that is not an address must never become a subject token"
        );
    }

    #[test]
    fn only_a_lower_cased_hex_address_is_a_session_key() {
        assert!(is_session_key("0xaaaa00000000000000000000000000000000bbbb"));
        assert!(!is_session_key(
            "0xAAAA00000000000000000000000000000000BBBB"
        ));
        assert!(!is_session_key("0xaaaa"));
        assert!(!is_session_key(""));
        assert!(!is_session_key("aaaa00000000000000000000000000000000bbbb"));
    }
}
