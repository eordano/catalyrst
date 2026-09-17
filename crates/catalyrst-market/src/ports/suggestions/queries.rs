use super::constants::{NEIGHBORS_TABLE, PROFILE_SQL_LIMIT, TASTE_ITEMS_PER_CREATOR};
use crate::MARKETPLACE_SQUID_SCHEMA;

/// What the wallet BOUGHT and still holds, newest first.
///
/// Same paid-only rule as the offline job (`select_acquisitions`): an airdropped item says
/// nothing about taste, and a wallet holding fifty of them would otherwise drown out its three
/// real purchases. The `nft` join is what makes it "still holds" -- a resold item stops being a
/// statement about what this wallet likes.
///
/// `$1` is the lowercased wallet address.
pub fn select_owned_acquisitions() -> String {
    format!(
        "SELECT item_id, acquired_at FROM (\n\
           SELECT DISTINCT ON (n.item_id)\n\
                  n.item_id::text AS item_id,\n\
                  GREATEST(COALESCE(s.timestamp, 0), COALESCE(m.timestamp, 0))::bigint AS acquired_at\n\
             FROM {MARKETPLACE_SQUID_SCHEMA}.nft n\n\
             LEFT JOIN {MARKETPLACE_SQUID_SCHEMA}.sale s\n\
               ON s.buyer = $1 AND s.item_id = n.item_id\n\
             LEFT JOIN {MARKETPLACE_SQUID_SCHEMA}.mint m\n\
               ON split_part(m.beneficiary, '-', 1) = $1\n\
              AND m.item_id = n.item_id\n\
              AND COALESCE(m.search_primary_sale_price, 0) > 0\n\
            WHERE n.owner_address = $1\n\
              AND n.item_id IS NOT NULL\n\
              AND (s.id IS NOT NULL OR m.id IS NOT NULL)\n\
         ) owned\n\
         ORDER BY acquired_at DESC\n\
         LIMIT {PROFILE_SQL_LIMIT}"
    )
}

/// The attributes the taste scorer needs, for an explicit id list (`$1`), at the MANA/USD rate
/// (`$2`).
///
/// Read straight from the squid `item` row rather than through the unified core: these ids are
/// things the wallet already OWNS, and an item it bought a year ago that nobody is selling today
/// still describes its taste. Filtering them by sellability would quietly empty the profile of
/// exactly the wallets that buy the most.
///
/// The price is converted the way the catalogue converts it -- MANA to USD at the live rate, USD
/// to credits at ten per dollar -- because the band this builds is compared against candidates
/// whose prices come out of the unified core already in credits. A fixed divisor would only be
/// right if one MANA were worth exactly one dollar, and the price term would otherwise be the
/// out-of-band decay for every candidate.
///
/// `rarity` is lowercased here for the same reason every other rarity read in this crate is: the
/// squid column is mixed-case, and the affinity map it feeds is looked up with the lowercase tier
/// names, so an un-lowered key is a term that silently scores zero.
pub fn select_item_attributes() -> String {
    format!(
        "SELECT\n\
           id::text AS item_id,\n\
           COALESCE(creator, '') AS creator,\n\
           COALESCE(collection_id, '') AS collection_id,\n\
           CASE WHEN item_type LIKE 'emote%' THEN 'emote' ELSE 'wearable' END\n\
             || ':' || COALESCE(search_wearable_category, search_emote_category, '') AS sub_category,\n\
           lower(COALESCE(rarity, '')) AS rarity,\n\
           ((COALESCE(price, 0) / 1e18) * $2::float8 * 10)::float8 AS price_credits,\n\
           (item_type NOT LIKE 'emote%') AS is_wearable\n\
         FROM {MARKETPLACE_SQUID_SCHEMA}.item\n\
        WHERE id::text = ANY($1)"
    )
}

/// Which of THESE items (`$2`) the wallet (`$1`) already holds.
///
/// The profile cannot answer it: the profile is capped and paid-only, so an airdropped holding,
/// or one past the cap, is absent from it and would be offered back to its own owner. Asking
/// `nft` about the handful of candidates already on the table keeps this an index probe on
/// `owner_address` narrowed by a small array -- which is why `owner_address` is NOT wrapped in
/// `lower()` here: addresses are stored lowercased, and wrapping the column makes the owner index
/// unusable and turns a millisecond lookup into a sequential scan of every NFT.
pub fn select_owned_among() -> String {
    format!(
        "SELECT DISTINCT n.item_id::text AS item_id\n\
           FROM {MARKETPLACE_SQUID_SCHEMA}.nft n\n\
          WHERE n.owner_address = $1 AND n.item_id = ANY($2)"
    )
}

/// Precomputed neighbours of the profile's items (`$1`), both sources, best first.
///
/// `rank` is the cut the job already made per anchor; `$2` takes the head of it so a profile of
/// two hundred items cannot pull ten thousand rows for a rail of twelve.
pub fn select_neighbors() -> String {
    format!(
        "SELECT item_id, source, neighbor_id, sim, support\n\
           FROM {NEIGHBORS_TABLE}\n\
          WHERE item_id = ANY($1) AND rank < $2\n\
          ORDER BY item_id, source, rank"
    )
}

/// Items by the creators the wallet collects most (`$1`), newest first.
///
/// The cold path: a wallet with a profile but no neighbour coverage -- a brand-new item, a
/// rebuild that has not run yet -- still gets a rail that is about IT rather than about the
/// catalogue. Ranked by recency inside each creator so the rail is not that creator's back
/// catalogue.
pub fn select_creator_items() -> String {
    format!(
        "SELECT item_id FROM (\n\
           SELECT id::text AS item_id,\n\
                  row_number() OVER (PARTITION BY creator ORDER BY created_at DESC) AS rn\n\
             FROM {MARKETPLACE_SQUID_SCHEMA}.item\n\
            WHERE creator = ANY($1)\n\
              AND search_is_collection_approved = true\n\
              AND search_emote_outcome_type IS NULL\n\
         ) ranked\n\
         WHERE rn <= {TASTE_ITEMS_PER_CREATOR}"
    )
}

/// Windowed sale counts per item (`$1` = days back), normalised 0..1 by the caller.
///
/// Popularity is the ONLY blend component that is not relative to this wallet, which is what
/// makes it the sensible tie-breaker between two candidates the wallet has no opinion about --
/// and the whole rail for a wallet with no profile at all.
pub fn select_popularity() -> String {
    format!(
        "SELECT COALESCE(item_id, search_contract_address || '-' || search_item_id::text) AS item_id,\n\
                count(*)::bigint AS sales\n\
           FROM {MARKETPLACE_SQUID_SCHEMA}.sale\n\
          WHERE timestamp >= $1\n\
            AND (item_id IS NOT NULL OR search_item_id IS NOT NULL)\n\
          GROUP BY 1\n\
          ORDER BY sales DESC\n\
          LIMIT $2"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The offline job and the request path have to agree on what an acquisition is, or the
    /// profile and the neighbour space describe different populations.
    #[test]
    fn the_request_time_profile_reads_paid_acquisitions_only() {
        let sql = select_owned_acquisitions();
        assert!(
            sql.contains("COALESCE(m.search_primary_sale_price, 0) > 0"),
            "{sql}"
        );
        assert!(
            sql.contains("s.id IS NOT NULL OR m.id IS NOT NULL"),
            "an unpaid holding is not a purchase: {sql}"
        );
        assert!(sql.contains("n.owner_address = $1"), "still holds: {sql}");
        assert!(sql.contains(&format!("LIMIT {PROFILE_SQL_LIMIT}")), "{sql}");
    }

    /// Profile attributes must NOT be gated on sellability: the wallet's own history is the
    /// signal, and most of it is not for sale today.
    #[test]
    fn profile_attributes_are_not_filtered_by_sellability() {
        let sql = select_item_attributes();
        assert!(!sql.contains("mv_trades"), "{sql}");
        assert!(!sql.contains("search_is_collection_approved"), "{sql}");
        assert!(sql.contains("id::text = ANY($1)"), "{sql}");
        assert!(sql.contains("lower(COALESCE(rarity"), "{sql}");
    }

    /// The band this builds is compared against candidates priced in credits, so it has to be
    /// built in credits: MANA -> USD at the live rate, USD -> credits at ten per dollar.
    #[test]
    fn the_profiles_price_band_is_converted_to_credits_at_the_live_rate() {
        let sql = select_item_attributes();
        assert!(sql.contains("* $2::float8 * 10"), "{sql}");
        assert!(sql.contains("AS price_credits"), "{sql}");
    }

    /// Showing a wallet what it already holds is the single most damaging thing this rail can do,
    /// and the owner index is the only affordable way to ask.
    #[test]
    fn the_owned_check_probes_the_owner_index_unlowered() {
        let sql = select_owned_among();
        assert!(sql.contains("n.owner_address = $1"), "{sql}");
        assert!(!sql.contains("lower(n.owner_address)"), "{sql}");
        assert!(sql.contains("n.item_id = ANY($2)"), "{sql}");
    }

    #[test]
    fn the_neighbor_lookup_cuts_by_the_rank_the_job_already_wrote() {
        let sql = select_neighbors();
        assert!(sql.contains("rank < $2"), "{sql}");
        assert!(sql.contains("ORDER BY item_id, source, rank"), "{sql}");
    }

    /// The cold path is still personal: it is the wallet's creators, not the catalogue's.
    #[test]
    fn the_creator_fallback_is_per_creator_recency_and_excludes_social_emotes() {
        let sql = select_creator_items();
        assert!(
            sql.contains("PARTITION BY creator ORDER BY created_at DESC"),
            "{sql}"
        );
        assert!(
            sql.contains(&format!("rn <= {TASTE_ITEMS_PER_CREATOR}")),
            "{sql}"
        );
        assert!(sql.contains("search_emote_outcome_type IS NULL"), "{sql}");
    }
}
