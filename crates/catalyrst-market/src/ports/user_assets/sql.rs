pub(super) fn wearables_data_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " UNION ALL \
                SELECT \
                  ug.urn || ':' || COALESCE(ug.token_id, '') AS id, \
                  ug.collection AS contract_address, \
                  COALESCE(ug.token_id, '') AS token_id, \
                  NULL::text AS network, \
                  extract(epoch FROM ug.granted_at)::int8 AS created_at, \
                  extract(epoch FROM ug.granted_at)::int8 AS updated_at, \
                  ug.urn AS urn, \
                  lower($1) AS owner, \
                  NULL::text AS image, \
                  NULL::text AS item_id, \
                  NULL::text AS category, \
                  NULL::text AS rarity, \
                  NULL::text AS name, \
                  NULL::text AS item_type, \
                  NULL::text AS description, \
                  extract(epoch FROM ug.granted_at)::int8 AS transferred_at, \
                  NULL::text AS price, \
                  true AS is_leased \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'wearable' \
                  AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT id, contract_address, token_id, network, created_at, updated_at, \
                   urn, owner, image, item_id, category, rarity, name, item_type, \
                   description, transferred_at, price, is_leased FROM ( \
                SELECT \
                  nft.id AS id, \
                  nft.contract_address AS contract_address, \
                  nft.token_id::text AS token_id, \
                  nft.network AS network, \
                  nft.created_at::int8 AS created_at, \
                  nft.updated_at::int8 AS updated_at, \
                  nft.urn AS urn, \
                  owner_address AS owner, \
                  nft.image AS image, \
                  nft.item_id AS item_id, \
                  wearable.category AS category, \
                  wearable.rarity AS rarity, \
                  wearable.name AS name, \
                  nft.item_type AS item_type, \
                  wearable.description AS description, \
                  transferred_at::int8 AS transferred_at, \
                  item.price::text AS price, \
                  false AS is_leased \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
                LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 \
                  AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1'){grants_leg} \
            ) combined \
            ORDER BY created_at DESC \
            LIMIT $2 OFFSET $3"
    )
}

pub(super) fn wearables_count_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " \
                 + (SELECT COUNT(*) FROM marketplace.usage_grants ug \
                    WHERE ug.status = 'active' AND ug.category = 'wearable' \
                      AND ug.grantee_address = lower($1))"
    } else {
        ""
    };
    format!(
        "SELECT (SELECT COUNT(*) FROM squid_marketplace.nft nft \
                    WHERE owner_address = $1 \
                      AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')){grants_leg}"
    )
}

pub(super) fn wearables_unique_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " \
                 + (SELECT COUNT(DISTINCT ug.urn) FROM marketplace.usage_grants ug \
                    WHERE ug.status = 'active' AND ug.category = 'wearable' \
                      AND ug.grantee_address = lower($1))"
    } else {
        ""
    };
    format!(
        "SELECT (SELECT COUNT(DISTINCT nft.item_id) FROM squid_marketplace.nft nft \
                    WHERE owner_address = $1 \
                      AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')){grants_leg}"
    )
}

pub(super) fn wearables_urn_token_data_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " UNION ALL \
                SELECT ug.urn AS urn, COALESCE(ug.token_id, '') AS token_id, \
                       extract(epoch FROM ug.granted_at)::numeric AS created_at \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'wearable' \
                  AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT urn, token_id FROM ( \
                SELECT nft.urn AS urn, nft.token_id::text AS token_id, \
                       nft.created_at::numeric AS created_at \
                FROM squid_marketplace.nft nft \
                WHERE owner_address = $1 \
                  AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1'){grants_leg} \
            ) owned \
            ORDER BY created_at DESC \
            LIMIT $2 OFFSET $3"
    )
}

pub(super) fn emotes_data_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " UNION ALL \
                SELECT \
                  ug.urn || ':' || COALESCE(ug.token_id, '') AS id, \
                  ug.collection AS contract_address, \
                  COALESCE(ug.token_id, '') AS token_id, \
                  NULL::text AS network, \
                  extract(epoch FROM ug.granted_at)::int8 AS created_at, \
                  extract(epoch FROM ug.granted_at)::int8 AS updated_at, \
                  ug.urn AS urn, \
                  lower($1) AS owner, \
                  NULL::text AS image, \
                  NULL::text AS item_id, \
                  NULL::text AS category, \
                  NULL::text AS rarity, \
                  NULL::text AS name, \
                  NULL::text AS item_type, \
                  NULL::text AS description, \
                  extract(epoch FROM ug.granted_at)::int8 AS transferred_at, \
                  NULL::text AS price, \
                  true AS is_leased \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'emote' \
                  AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT id, contract_address, token_id, network, created_at, updated_at, \
                   urn, owner, image, item_id, category, rarity, name, item_type, \
                   description, transferred_at, price, is_leased FROM ( \
                SELECT \
                  nft.id AS id, \
                  nft.contract_address AS contract_address, \
                  nft.token_id::text AS token_id, \
                  nft.network AS network, \
                  nft.created_at::int8 AS created_at, \
                  nft.updated_at::int8 AS updated_at, \
                  nft.urn AS urn, \
                  owner_address AS owner, \
                  nft.image AS image, \
                  nft.item_id AS item_id, \
                  emote.category AS category, \
                  emote.rarity AS rarity, \
                  emote.name AS name, \
                  nft.item_type AS item_type, \
                  emote.description AS description, \
                  transferred_at::int8 AS transferred_at, \
                  item.price::text AS price, \
                  false AS is_leased \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 \
                  AND nft.item_type = 'emote_v1'{grants_leg} \
            ) combined \
            ORDER BY created_at DESC \
            LIMIT $2 OFFSET $3"
    )
}

pub(super) fn emotes_count_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " \
                 + (SELECT COUNT(*) FROM marketplace.usage_grants ug \
                    WHERE ug.status = 'active' AND ug.category = 'emote' \
                      AND ug.grantee_address = lower($1))"
    } else {
        ""
    };
    format!(
        "SELECT (SELECT COUNT(*) FROM squid_marketplace.nft nft \
                    WHERE owner_address = $1 AND nft.item_type = 'emote_v1'){grants_leg}"
    )
}

pub(super) fn emotes_unique_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " \
                 + (SELECT COUNT(DISTINCT ug.urn) FROM marketplace.usage_grants ug \
                    WHERE ug.status = 'active' AND ug.category = 'emote' \
                      AND ug.grantee_address = lower($1))"
    } else {
        ""
    };
    format!(
        "SELECT (SELECT COUNT(DISTINCT nft.item_id) FROM squid_marketplace.nft nft \
                    WHERE owner_address = $1 AND nft.item_type = 'emote_v1'){grants_leg}"
    )
}

pub(super) fn emotes_urn_token_data_sql(grants_present: bool) -> String {
    let grants_leg = if grants_present {
        " UNION ALL \
                SELECT ug.urn AS urn, COALESCE(ug.token_id, '') AS token_id, \
                       extract(epoch FROM ug.granted_at)::numeric AS created_at \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'emote' \
                  AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT urn, token_id FROM ( \
                SELECT nft.urn AS urn, nft.token_id::text AS token_id, \
                       nft.created_at::numeric AS created_at \
                FROM squid_marketplace.nft nft \
                WHERE owner_address = $1 \
                  AND nft.item_type = 'emote_v1'{grants_leg} \
            ) owned \
            ORDER BY created_at DESC \
            LIMIT $2 OFFSET $3"
    )
}

pub(super) fn grouped_wearables_data_sql(
    grants_present: bool,
    inner_where: &str,
    outer_where: &str,
    order: &str,
    limit_idx: usize,
    offset_idx: usize,
) -> String {
    let (grouped_grants_cte, grouped_grants_union) = if grants_present {
        (
            ", grouped_grants AS ( \
                SELECT ug.urn, NULL::varchar AS category, NULL::text AS rarity, \
                    NULL::text AS name, NULL::varchar AS item_type, \
                    COUNT(*) AS amount, \
                    MIN(extract(epoch FROM ug.granted_at))::int8 AS min_transferred_at, \
                    MAX(extract(epoch FROM ug.granted_at))::int8 AS max_transferred_at, \
                    MIN(extract(epoch FROM ug.granted_at))::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', ug.urn || ':' || COALESCE(ug.token_id, ''), \
                            'tokenId', COALESCE(ug.token_id, ''), \
                            'transferredAt', extract(epoch FROM ug.granted_at)::int8::text, \
                            'price', '0' \
                        ) ORDER BY ug.granted_at DESC \
                    ) AS individual_data, \
                    0 AS rarity_order, \
                    true AS is_leased \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'wearable' \
                  AND ug.grantee_address = lower($1) \
                GROUP BY ug.urn \
            )",
            " UNION ALL SELECT * FROM grouped_grants",
        )
    } else {
        ("", "")
    };
    format!(
        "WITH grouped_wearables AS ( \
                SELECT nft.urn, wearable.category, wearable.rarity, wearable.name, metadata.item_type, \
                    COUNT(*) AS amount, \
                    MIN(nft.transferred_at)::int8 AS min_transferred_at, \
                    MAX(nft.transferred_at)::int8 AS max_transferred_at, \
                    MIN(nft.created_at)::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', nft.urn || ':' || nft.token_id::text, \
                            'tokenId', nft.token_id::text, \
                            'transferredAt', COALESCE(nft.transferred_at, 0)::text, \
                            'price', COALESCE(item.price, 0)::text \
                        ) ORDER BY nft.created_at DESC \
                    ) AS individual_data, \
                    CASE wearable.rarity \
                        WHEN 'unique' THEN 8 WHEN 'mythic' THEN 7 WHEN 'exotic' THEN 6 \
                        WHEN 'legendary' THEN 5 WHEN 'epic' THEN 4 WHEN 'rare' THEN 3 \
                        WHEN 'uncommon' THEN 2 WHEN 'common' THEN 1 ELSE 0 \
                    END AS rarity_order, \
                    false AS is_leased \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
                LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 {inner_where} \
                GROUP BY nft.urn, wearable.category, wearable.rarity, wearable.name, metadata.item_type \
            ){grouped_grants_cte} SELECT * FROM ( \
                SELECT * FROM grouped_wearables{grouped_grants_union} \
            ) gw {outer_where} {order} LIMIT ${limit_idx} OFFSET ${offset_idx}"
    )
}

pub(super) fn grouped_wearables_count_sql(
    grants_present: bool,
    count_where: &str,
    count_item_type: &str,
) -> String {
    let grouped_grants_count = if grants_present {
        " UNION \
                 SELECT DISTINCT ug.urn FROM marketplace.usage_grants ug \
                 WHERE ug.status = 'active' AND ug.category = 'wearable' \
                   AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT COUNT(*) FROM ( \
                 SELECT DISTINCT nft.urn FROM squid_marketplace.nft nft \
                 LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
                 LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
                 WHERE owner_address = $1 {count_where} {count_item_type}{grouped_grants_count} \
             ) d"
    )
}

pub(super) fn grouped_emotes_data_sql(
    grants_present: bool,
    inner_where: &str,
    outer_where: &str,
    order: &str,
    limit_idx: usize,
    offset_idx: usize,
) -> String {
    let (grouped_grants_cte, grouped_grants_union) = if grants_present {
        (
            ", grouped_grants AS ( \
                SELECT ug.urn, NULL::varchar AS category, NULL::text AS rarity, \
                    NULL::text AS name, \
                    COUNT(*) AS amount, \
                    MIN(extract(epoch FROM ug.granted_at))::int8 AS min_transferred_at, \
                    MAX(extract(epoch FROM ug.granted_at))::int8 AS max_transferred_at, \
                    MIN(extract(epoch FROM ug.granted_at))::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', ug.urn || ':' || COALESCE(ug.token_id, ''), \
                            'tokenId', COALESCE(ug.token_id, ''), \
                            'transferredAt', extract(epoch FROM ug.granted_at)::int8::text, \
                            'price', '0' \
                        ) ORDER BY ug.granted_at DESC \
                    ) AS individual_data, \
                    0 AS rarity_order, \
                    true AS is_leased \
                FROM marketplace.usage_grants ug \
                WHERE ug.status = 'active' AND ug.category = 'emote' \
                  AND ug.grantee_address = lower($1) \
                GROUP BY ug.urn \
            )",
            " UNION ALL SELECT * FROM grouped_grants",
        )
    } else {
        ("", "")
    };
    format!(
        "WITH grouped_emotes AS ( \
                SELECT nft.urn, emote.category, emote.rarity, emote.name, \
                    COUNT(*) AS amount, \
                    MIN(nft.transferred_at)::int8 AS min_transferred_at, \
                    MAX(nft.transferred_at)::int8 AS max_transferred_at, \
                    MIN(nft.created_at)::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', nft.urn || ':' || nft.token_id::text, \
                            'tokenId', nft.token_id::text, \
                            'transferredAt', COALESCE(nft.transferred_at, 0)::text, \
                            'price', COALESCE(item.price, 0)::text \
                        ) ORDER BY nft.created_at DESC \
                    ) AS individual_data, \
                    CASE emote.rarity \
                        WHEN 'unique' THEN 8 WHEN 'mythic' THEN 7 WHEN 'exotic' THEN 6 \
                        WHEN 'legendary' THEN 5 WHEN 'epic' THEN 4 WHEN 'rare' THEN 3 \
                        WHEN 'uncommon' THEN 2 WHEN 'common' THEN 1 ELSE 0 \
                    END AS rarity_order, \
                    false AS is_leased \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 \
                  AND nft.item_type = 'emote_v1' {inner_where} \
                GROUP BY nft.urn, emote.category, emote.rarity, emote.name \
            ){grouped_grants_cte} SELECT * FROM ( \
                SELECT * FROM grouped_emotes{grouped_grants_union} \
            ) ge {outer_where} {order} LIMIT ${limit_idx} OFFSET ${offset_idx}"
    )
}

pub(super) fn grouped_emotes_count_sql(grants_present: bool, count_where: &str) -> String {
    let grouped_grants_count = if grants_present {
        " UNION \
                 SELECT DISTINCT ug.urn FROM marketplace.usage_grants ug \
                 WHERE ug.status = 'active' AND ug.category = 'emote' \
                   AND ug.grantee_address = lower($1)"
    } else {
        ""
    };
    format!(
        "SELECT COUNT(*) FROM ( \
                 SELECT DISTINCT nft.urn FROM squid_marketplace.nft nft \
                 LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
                 WHERE owner_address = $1 AND nft.item_type = 'emote_v1' {count_where}{grouped_grants_count} \
             ) d"
    )
}

#[cfg(test)]
mod grants_gating_tests {
    use super::*;

    #[test]
    fn absent_table_never_references_usage_grants() {
        let builders = [
            ("wearables_data", wearables_data_sql(false)),
            ("wearables_count", wearables_count_sql(false)),
            ("wearables_unique", wearables_unique_sql(false)),
            (
                "wearables_urn_token_data",
                wearables_urn_token_data_sql(false),
            ),
            ("emotes_data", emotes_data_sql(false)),
            ("emotes_count", emotes_count_sql(false)),
            ("emotes_unique", emotes_unique_sql(false)),
            ("emotes_urn_token_data", emotes_urn_token_data_sql(false)),
            (
                "grouped_wearables_data",
                grouped_wearables_data_sql(false, "", "", "", 2, 3),
            ),
            (
                "grouped_wearables_count",
                grouped_wearables_count_sql(false, "", ""),
            ),
            (
                "grouped_emotes_data",
                grouped_emotes_data_sql(false, "", "", "", 2, 3),
            ),
            ("grouped_emotes_count", grouped_emotes_count_sql(false, "")),
        ];
        for (name, sql) in &builders {
            assert!(
                !sql.contains("usage_grants"),
                "{name} leaked a usage_grants reference when the table is absent: {sql}"
            );
        }
    }

    #[test]
    fn present_table_applies_overlay() {
        assert!(wearables_data_sql(true).contains("marketplace.usage_grants"));
        assert!(wearables_data_sql(true).contains("UNION ALL"));
        assert!(emotes_data_sql(true).contains("marketplace.usage_grants"));
        assert!(emotes_data_sql(true).contains("UNION ALL"));

        assert!(wearables_count_sql(true).contains("marketplace.usage_grants"));
        assert!(emotes_count_sql(true).contains("marketplace.usage_grants"));
        assert!(wearables_unique_sql(true).contains("marketplace.usage_grants"));
        assert!(emotes_unique_sql(true).contains("marketplace.usage_grants"));
        assert!(wearables_urn_token_data_sql(true).contains("marketplace.usage_grants"));
        assert!(emotes_urn_token_data_sql(true).contains("marketplace.usage_grants"));

        assert!(grouped_wearables_data_sql(true, "", "", "", 2, 3).contains("grouped_grants"));
        assert!(grouped_emotes_data_sql(true, "", "", "", 2, 3).contains("grouped_grants"));
        assert!(grouped_wearables_count_sql(true, "", "").contains("marketplace.usage_grants"));
        assert!(grouped_emotes_count_sql(true, "").contains("marketplace.usage_grants"));
    }

    #[test]
    fn onchain_leg_untouched_when_absent() {
        let w = wearables_data_sql(false);
        assert!(w.contains("FROM squid_marketplace.nft nft"));
        assert!(w.contains("transferred_at::int8 AS transferred_at"));
        let e = emotes_data_sql(false);
        assert!(e.contains("FROM squid_marketplace.nft nft"));
        assert!(e.contains("transferred_at::int8 AS transferred_at"));
    }
}
