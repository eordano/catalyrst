// `account_id` is `0x<address>-<NETWORK>`; compare only the address segment and
// compare it whole. A prefix test would let a truncated address ("0x") match
// every account.
pub(super) fn address_matches_account_id(address: &str, account_id: &str) -> bool {
    account_id
        .split('-')
        .next()
        .is_some_and(|owner| owner.eq_ignore_ascii_case(address))
}

// Ownership comes from the NFT entity, never from `ens.owner_id`: the squid's
// ENS handler seeds the owner from the registrar *caller* and never updates it,
// so a DCLControllerV2 registration records the controller contract rather than
// the buyer. `nft.owner_id` is the ERC-721 owner and tracks later transfers.
pub(super) async fn resolve_name_owner_id(
    pool: &sqlx::PgPool,
    label: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT n.owner_id FROM squid_marketplace.nft n
         JOIN squid_marketplace.ens e ON n.ens_id = e.id
         WHERE n.category = 'ens' AND lower(e.subdomain) = lower($1)",
    )
    .bind(label)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_matching_is_case_insensitive() {
        assert!(address_matches_account_id(
            "0x959E104E1A4DB6317FA58F8295F586E1A978C297",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
        assert!(!address_matches_account_id(
            "0xdeadbeef",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
    }

    #[test]
    fn a_truncated_address_never_matches() {
        let account_id = "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM";
        for prefix in [
            "0x",
            "0x959e",
            "0x959e104e1a4db6317fa58f8295f586e1a978c29",
            "",
        ] {
            assert!(
                !address_matches_account_id(prefix, account_id),
                "prefix {prefix:?} must not authorize"
            );
        }
        assert!(!address_matches_account_id(
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM",
            account_id
        ));
    }

    #[test]
    fn name_label_ownership_match_is_case_insensitive() {
        let owner_id = "0x959E104E1A4DB6317FA58f8295F586e1A978C297-ETHEREUM";
        assert!(address_matches_account_id(
            "0x959e104e1a4db6317fa58f8295f586e1a978c297",
            owner_id
        ));
        assert!(address_matches_account_id(
            "0X959E104E1A4DB6317FA58F8295F586E1A978C297",
            owner_id
        ));
        assert!(!address_matches_account_id(
            "0x0000000000000000000000000000000000000001",
            owner_id
        ));
    }
}
