//! `urn:decentraland:<chain>:collections-v2:<contract>:<itemId>` <-> the `contract-itemId` the
//! rest of the pipeline keys on.
//!
//! Both the Polygon mainnet (`matic`) and testnet (`amoy`) chains are accepted so a dev profile
//! resolves the same way a production one does. Base avatars
//! (`urn:decentraland:off-chain:base-avatars:...`) resolve to nothing on purpose: they are given
//! to every account, so they say nothing about taste and nobody can buy one.

const URN_PREFIX: &str = "urn:decentraland:";
const COLLECTIONS_V2: &str = ":collections-v2:";

fn is_hex_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_blockchain_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

pub fn urn_to_item_id(urn: &str) -> Option<String> {
    let rest = urn.trim().strip_prefix(URN_PREFIX)?;
    let (chain, rest) = rest.split_once(':')?;
    if chain != "matic" && chain != "amoy" {
        return None;
    }
    let rest = format!(":{rest}");
    let tail = rest.strip_prefix(COLLECTIONS_V2)?;
    let (contract, item_id) = tail.split_once(':')?;
    if !is_hex_address(contract) || !is_blockchain_id(item_id) {
        return None;
    }
    Some(format!("{}-{}", contract.to_lowercase(), item_id))
}

/// `<contract>-<itemId>` as sent by the client, validated rather than trusted: it reaches SQL.
pub fn normalize_item_id(value: &str) -> Option<String> {
    let (contract, item_id) = value.trim().split_once('-')?;
    if !is_hex_address(contract) || !is_blockchain_id(item_id) {
        return None;
    }
    Some(format!("{}-{}", contract.to_lowercase(), item_id))
}

fn collect_capped<F>(values: &[String], limit: usize, resolve: F) -> Vec<String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut out: Vec<String> = Vec::new();
    for value in values {
        if out.len() >= limit {
            break;
        }
        if let Some(id) = resolve(value) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

pub fn normalize_item_ids(values: &[String], limit: usize) -> Vec<String> {
    collect_capped(values, limit, normalize_item_id)
}

/// Either spelling of the same thing, because the equipped list is the one place the client has
/// URNs rather than ids.
///
/// A URN is 83 characters against the id's 45, and thirty of them is most of the query string, so
/// the shop converts before sending. Accepting both is what lets that change ship without the two
/// deploys having to land in order -- an older shop keeps working against a newer server.
pub fn to_item_ids(values: &[String], limit: usize) -> Vec<String> {
    collect_capped(values, limit, |value| {
        if value.contains(':') {
            urn_to_item_id(value)
        } else {
            normalize_item_id(value)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = "0x1234567890abcdef1234567890abcdef12345678";

    #[test]
    fn a_collections_v2_urn_resolves_on_both_polygon_chains() {
        for chain in ["matic", "amoy"] {
            assert_eq!(
                urn_to_item_id(&format!(
                    "urn:decentraland:{chain}:collections-v2:{CONTRACT}:7"
                )),
                Some(format!("{CONTRACT}-7"))
            );
        }
    }

    #[test]
    fn a_urn_is_lowercased_and_trimmed() {
        let mixed = "0x1234567890ABCDEF1234567890abcdef12345678";
        assert_eq!(
            urn_to_item_id(&format!(
                "  urn:decentraland:matic:collections-v2:{mixed}:10  "
            )),
            Some(format!("{CONTRACT}-10"))
        );
    }

    /// Given to every account, so it says nothing about taste and nobody can buy one.
    #[test]
    fn a_base_avatar_or_a_foreign_chain_resolves_to_nothing() {
        for urn in [
            "urn:decentraland:off-chain:base-avatars:eyebrows_00",
            &format!("urn:decentraland:ethereum:collections-v2:{CONTRACT}:1"),
            &format!("urn:decentraland:matic:collections-v1:{CONTRACT}:1"),
            &format!("urn:decentraland:matic:collections-v2:{CONTRACT}:x"),
            &format!("urn:decentraland:matic:collections-v2:{CONTRACT}"),
            "urn:decentraland:matic:collections-v2:0xshort:1",
        ] {
            assert_eq!(urn_to_item_id(urn), None, "{urn}");
        }
    }

    /// It reaches SQL, so it is validated rather than trusted.
    #[test]
    fn a_compact_id_is_validated_not_trusted() {
        assert_eq!(
            normalize_item_id(&format!("{CONTRACT}-3")),
            Some(format!("{CONTRACT}-3"))
        );
        for junk in [
            "0xnothex-1",
            "1-2",
            &format!("{CONTRACT}-"),
            &format!("{CONTRACT}-1; DROP TABLE item"),
            "",
        ] {
            assert_eq!(normalize_item_id(junk), None, "{junk}");
        }
    }

    /// The equipped list may arrive in either spelling, and both must reach the same id.
    #[test]
    fn equipped_accepts_both_spellings_deduped_and_capped() {
        let values = vec![
            format!("urn:decentraland:matic:collections-v2:{CONTRACT}:1"),
            format!("{CONTRACT}-1"),
            format!("{CONTRACT}-2"),
            format!("{CONTRACT}-3"),
        ];
        assert_eq!(
            to_item_ids(&values, 30),
            vec![
                format!("{CONTRACT}-1"),
                format!("{CONTRACT}-2"),
                format!("{CONTRACT}-3")
            ]
        );
        assert_eq!(to_item_ids(&values, 2).len(), 2);
    }

    /// Seeds and excludes are compact-only: a URN there is a client bug, not an older client.
    #[test]
    fn the_compact_only_lists_ignore_urns() {
        let values = vec![
            format!("urn:decentraland:matic:collections-v2:{CONTRACT}:1"),
            format!("{CONTRACT}-2"),
        ];
        assert_eq!(
            normalize_item_ids(&values, 20),
            vec![format!("{CONTRACT}-2")]
        );
    }
}
