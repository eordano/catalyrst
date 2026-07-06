use crate::dcl_schemas::{get_db_networks, NftCategory};
use crate::logic::sql_filters::{clamp_first, clamp_skip, where_from};
use crate::ports::items::ItemType;
use crate::MARKETPLACE_SQUID_SCHEMA;

use super::types::{NftFilters, NftSortBy};
use super::MAX_ORDER_TIMESTAMP;

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
    Float(f64),
}

pub fn build_nfts_query(filters: &NftFilters, for_count: bool) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    fn emit(b: Bind, bs: &mut Vec<Bind>, idx: &mut usize) -> String {
        bs.push(b);
        let s = format!("${}", *idx);
        *idx += 1;
        s
    }

    let mut inner_wheres: Vec<String> = Vec::new();

    if let Some(ref o) = filters.owner {
        let p = emit(Bind::Text(o.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" owner_address = {} ", p));
    }
    if let Some(c) = filters.category {
        let p = emit(
            Bind::Text(nft_category_db_str(c).to_string()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" category = {} ", p));
    }
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" token_id = {}::numeric ", p));
    }
    if let Some(ref iid) = filters.item_id {
        let p = emit(Bind::Text(iid.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" LOWER(item_id) = LOWER({}) ", p));
    }
    if let Some(n) = filters.network {
        let p = emit(
            Bind::TextArray(get_db_networks(n).into_iter().map(String::from).collect()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" network = ANY ({}) ", p));
    }
    if filters.is_wearable_head {
        inner_wheres.push(" search_is_wearable_head = true ".to_string());
    }
    if filters.is_land {
        inner_wheres.push(" search_is_land = true ".to_string());
    }
    if filters.is_wearable_accessory {
        inner_wheres.push(" search_is_wearable_accessory = true ".to_string());
    }
    if filters.is_wearable_smart {
        let p = emit(
            Bind::Text(ItemType::SmartWearableV1.as_str().to_string()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" item_type = {} ", p));
    }
    if !filters.contract_addresses.is_empty() {
        let p = emit(
            Bind::TextArray(filters.contract_addresses.clone()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" contract_address = ANY ({}) ", p));
    }
    if let Some(ref s) = filters.search {
        let p = emit(Bind::Text(s.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_text % {} ", p));
    }
    if let Some(mn) = filters.min_distance_to_plaza {
        let p = emit(Bind::Float(mn), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_distance_to_plaza >= {} ", p));
    }
    if let Some(mx) = filters.max_distance_to_plaza {
        let p = emit(Bind::Float(mx), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_distance_to_plaza <= {} ", p));
    }
    if filters.adjacent_to_road {
        inner_wheres.push(" search_adjacent_to_road = true ".to_string());
    }
    if !filters.ids.is_empty() {
        let p = emit(
            Bind::TextArray(filters.ids.clone()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" id = ANY ({}) ", p));
    }
    if let Some(ref mn) = filters.min_price {
        let p = emit(Bind::Text(mn.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_order_price >= {}::numeric ", p));
    }
    if let Some(ref mx) = filters.max_price {
        let p = emit(Bind::Text(mx.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_order_price <= {}::numeric ", p));
    }
    if filters.is_land
        || filters.category == Some(NftCategory::Parcel)
        || filters.category == Some(NftCategory::Estate)
    {
        inner_wheres.push(" search_estate_size > 0 ".to_string());
    }

    let inner_where = where_from(&inner_wheres);

    let inner_sort = match filters.sort_by {
        Some(NftSortBy::Name) => " ORDER BY name ASC, id ASC ",
        Some(NftSortBy::Newest) => " ORDER BY created_at DESC, id ASC ",
        Some(NftSortBy::RecentlySold) => " ORDER BY sold_at DESC, id ASC ",

        Some(NftSortBy::CheapestParcel) => " ORDER BY search_order_price ASC NULLS LAST, id ASC ",
        _ => "",
    };

    let inner_sort = if for_count { "" } else { inner_sort };
    let has_outer_filters = filters.emote_has_sound
        || filters.emote_has_geometry
        || filters.emote_outcome_type.is_some()
        || filters.emote_category.is_some()
        || filters.wearable_category.is_some()
        || emote_play_mode_clause(&filters.emote_play_mode).is_some()
        || body_shapes_for_genders(&filters.emote_genders).is_some()
        || body_shapes_for_genders(&filters.wearable_genders).is_some()
        || !filters.creator.is_empty()
        || !filters.item_rarities.is_empty()
        || filters.is_on_sale.is_some()
        || !filters.banned_names.is_empty()
        || filters.include_social_emotes == Some(false);
    let apply_inner_limit = !matches!(filters.sort_by, Some(NftSortBy::RecentlyListed))
        && filters.owner.is_none()
        && !has_outer_filters;
    let limit_val = clamp_first(filters.first, 100);
    let offset_val = clamp_skip(filters.skip);
    let inner_limit_offset = if apply_inner_limit && !for_count {
        let lp = emit(Bind::Int(limit_val), &mut binds, &mut next_idx);
        let op = emit(Bind::Int(offset_val), &mut binds, &mut next_idx);
        format!(" LIMIT {} OFFSET {} ", lp, op)
    } else {
        String::new()
    };

    let mut estate_wheres: Vec<String> = Vec::new();
    if let Some(mn) = filters.min_estate_size {
        let p = emit(Bind::Float(mn), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.size >= {} ", p));
    } else {
        estate_wheres.push(" est.size > 0 ".to_string());
    }
    if let Some(mx) = filters.max_estate_size {
        let p = emit(Bind::Float(mx), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.size <= {} ", p));
    }
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.token_id = {}::numeric ", p));
    }
    let estate_where = where_from(&estate_wheres);

    let mut parcel_wheres: Vec<String> = Vec::new();
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        parcel_wheres.push(format!(
            " (par.token_id = {p}::numeric OR par_est.token_id = {p}::numeric) ",
            p = p
        ));
    }
    let parcel_where = where_from(&parcel_wheres);

    let trades_cat = if let Some(c) = filters.category {
        let p = emit(
            Bind::Text(nft_category_db_str(c).to_string()),
            &mut binds,
            &mut next_idx,
        );
        format!(" WHERE sent_nft_category = {} ", p)
    } else {
        String::new()
    };

    let mut outer_wheres: Vec<String> = Vec::new();
    if filters.emote_has_sound {
        outer_wheres.push(" emote.has_sound = true ".to_string());
    }
    if filters.emote_has_geometry {
        outer_wheres.push(" emote.has_geometry = true ".to_string());
    }
    if filters.emote_outcome_type.is_some() {
        outer_wheres.push(" emote.outcome_type IS NOT NULL ".to_string());
    }
    if let Some(ref ec) = filters.emote_category {
        let p = emit(Bind::Text(ec.clone()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" emote.category = {} ", p));
    }
    if let Some(ref wc) = filters.wearable_category {
        let p = emit(Bind::Text(wc.clone()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" wearable.category = {} ", p));
    }
    if let Some(mode) = emote_play_mode_clause(&filters.emote_play_mode) {
        outer_wheres.push(format!(" nft.search_emote_loop = {} ", mode));
    }
    if let Some(arr) = body_shapes_for_genders(&filters.emote_genders) {
        let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" nft.search_emote_body_shapes @> {} ", p));
    }
    if let Some(arr) = body_shapes_for_genders(&filters.wearable_genders) {
        let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" nft.search_wearable_body_shapes @> {} ", p));
    }
    if !filters.creator.is_empty() {
        let lower: Vec<String> = filters.creator.iter().map(|c| c.to_lowercase()).collect();
        let p = emit(Bind::TextArray(lower), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" LOWER(item.creator) = ANY({}) ", p));
    }
    if !filters.item_rarities.is_empty() {
        let p = emit(
            Bind::TextArray(filters.item_rarities.clone()),
            &mut binds,
            &mut next_idx,
        );
        outer_wheres.push(format!(
            " (nft.search_wearable_rarity = ANY ({p}) OR nft.search_emote_rarity = ANY ({p})) ",
            p = p
        ));
    }
    match filters.is_on_sale {
        Some(true) => {
            outer_wheres.push(format!(
                " (trades.id IS NOT NULL OR (nft.search_order_status = 'open' \
                  AND nft.search_order_expires_at < {max_ts} \
                  AND ((LENGTH(nft.search_order_expires_at::text) = 13 \
                        AND TO_TIMESTAMP(nft.search_order_expires_at / 1000.0) > NOW()) \
                     OR (LENGTH(nft.search_order_expires_at::text) = 10 \
                        AND TO_TIMESTAMP(nft.search_order_expires_at) > NOW())))) ",
                max_ts = MAX_ORDER_TIMESTAMP
            ));
        }
        Some(false) => {
            outer_wheres
                .push(" (trades.id IS NULL AND nft.search_order_status IS NULL) ".to_string());
        }
        None => {}
    }
    if !filters.banned_names.is_empty() {
        let p = emit(
            Bind::TextArray(filters.banned_names.clone()),
            &mut binds,
            &mut next_idx,
        );
        outer_wheres.push(format!(
            " (nft.category != 'ens' OR nft.name <> ALL ({})) ",
            p
        ));
    }
    if filters.include_social_emotes == Some(false) {
        outer_wheres.push(" emote.outcome_type IS NULL ".to_string());
    }

    let outer_where = where_from(&outer_wheres);

    let main_sort = match filters.sort_by {
        Some(NftSortBy::RecentlyListed) => " ORDER BY order_created_at DESC, nft.id ASC ",
        Some(NftSortBy::Name) => " ORDER BY name ASC, nft.id ASC ",
        Some(NftSortBy::Newest) => " ORDER BY created_at DESC, nft.id ASC ",
        Some(NftSortBy::RecentlySold) => " ORDER BY sold_at DESC, nft.id ASC ",
        Some(NftSortBy::CheapestParcel) => " ORDER BY order_price ASC NULLS LAST, nft.id ASC ",
        _ => "",
    };

    let main_sort = if for_count { "" } else { main_sort };

    let outer_limit_offset = if apply_inner_limit || for_count {
        String::new()
    } else {
        let lp = emit(Bind::Int(limit_val), &mut binds, &mut next_idx);
        let op = emit(Bind::Int(offset_val), &mut binds, &mut next_idx);
        format!(" LIMIT {} OFFSET {} ", lp, op)
    };

    let core_sql = format!(
        "WITH unified_trades AS (
            SELECT * FROM marketplace.mv_trades {trades_cat}
         ),
         filtered_estate AS (
            SELECT est.id, est.token_id, est.size, est.data_id,
                -- JSONB_AGG (one jsonb array), not ARRAY_AGG(JSON_BUILD_OBJECT)
                -- which yields json[] and mismatches the Json<Vec<_>> decode.
                JSONB_AGG(JSONB_BUILD_OBJECT('x', est_parcel.x, 'y', est_parcel.y)) AS estate_parcels
            FROM {schema}.estate est
            LEFT JOIN {schema}.parcel est_parcel ON est.id = est_parcel.estate_id
            {estate_where}
            GROUP BY est.id, est.token_id, est.size, est.data_id
         ),
         parcel_estate_data AS (
            SELECT par.*, par_est.token_id::text AS parcel_estate_token_id,
                   est_data.name AS parcel_estate_name
            FROM {schema}.parcel par
            LEFT JOIN {schema}.estate par_est ON par.estate_id = par_est.id AND par_est.size > 0
            LEFT JOIN {schema}.data est_data ON par_est.data_id = est_data.id
            {parcel_where}
         ),
         filtered_nft AS (
            SELECT * FROM {schema}.nft {inner_where} {inner_sort} {inner_limit_offset}
         )
         SELECT
            COUNT(*) OVER() AS count,
            nft.id,
            nft.contract_address,
            nft.token_id::text as token_id,
            nft.network,
            nft.created_at::int8 as created_at,
            nft.token_uri AS url,
            nft.updated_at::int8 as updated_at,
            nft.sold_at::int8 as sold_at,
            nft.urn,
            account.address AS owner,
            nft.image,
            nft.issued_id::text AS issued_id,
            item.blockchain_id::text AS item_id,
            nft.category,
            COALESCE(wearable.rarity, emote.rarity) AS rarity,
            COALESCE(wearable.name, emote.name, land_data.name, ens.subdomain) AS name,
            parcel.x::text AS x,
            parcel.y::text AS y,
            ens.subdomain,
            wearable.body_shapes,
            wearable.category AS wearable_category,
            emote.category AS emote_category,
            nft.item_type,
            emote.loop,
            emote.has_sound,
            emote.has_geometry,
            emote.outcome_type AS emote_outcome_type,
            estate.estate_parcels,
            estate.size::int4 AS size,
            parcel.parcel_estate_token_id,
            parcel.parcel_estate_name,
            parcel.estate_id AS parcel_estate_id,
            COALESCE(wearable.description, emote.description, land_data.description) AS description,
            -- Sort key for sortBy=recently_listed. search_order_created_at is a
            -- NUMERIC unix-epoch on the nft row (NULL when not listed); upstream
            -- coalesces it with the trade created_at. The unified_trades CTE is
            -- the empty stub here, so trades.created_at is always NULL -- kept for
            -- parity. Without this projection the ORDER BY references a column that
            -- does not exist (column order_created_at does not exist) -> 500.
            COALESCE(TO_TIMESTAMP(nft.search_order_created_at), trades.created_at) AS order_created_at,
            -- Numeric listing price, projected so the outer ORDER BY for
            -- sortBy=cheapest_parcel sorts numerically (NULL when unlisted).
            nft.search_order_price AS order_price
         FROM filtered_nft nft
         LEFT JOIN {schema}.metadata metadata ON nft.metadata_id = metadata.id
         LEFT JOIN {schema}.wearable wearable ON metadata.wearable_id = wearable.id
         LEFT JOIN {schema}.emote emote ON metadata.emote_id = emote.id
         LEFT JOIN parcel_estate_data parcel ON nft.id = parcel.id
         LEFT JOIN filtered_estate estate ON nft.id = estate.id
         LEFT JOIN {schema}.data land_data ON (estate.data_id = land_data.id OR parcel.id = land_data.id)
         LEFT JOIN {schema}.ens ens ON ens.id = nft.ens_id
         LEFT JOIN {schema}.account account ON nft.owner_id = account.id
         LEFT JOIN {schema}.item item ON item.id = nft.item_id
         LEFT JOIN unified_trades trades ON trades.sent_contract_address = nft.contract_address
            AND trades.sent_token_id::numeric = nft.token_id
            AND trades.status = 'open' AND trades.signer = account.address
         {outer_where}
         {main_sort}
         {outer_limit_offset}",
        schema = MARKETPLACE_SQUID_SCHEMA,
        trades_cat = trades_cat,
        estate_where = estate_where,
        parcel_where = parcel_where,
        inner_where = inner_where,
        inner_sort = inner_sort,
        inner_limit_offset = inner_limit_offset,
        outer_where = outer_where,
        main_sort = main_sort,
        outer_limit_offset = outer_limit_offset,
    );

    let sql = if for_count {
        format!("SELECT COUNT(*)::int8 AS count FROM ({core_sql}) AS nft_count_sub")
    } else {
        core_sql
    };

    (sql, binds)
}

fn nft_category_db_str(c: NftCategory) -> &'static str {
    match c {
        NftCategory::Parcel => "parcel",
        NftCategory::Estate => "estate",
        NftCategory::Wearable => "wearable",
        NftCategory::Ens => "ens",
        NftCategory::Emote => "emote",
    }
}

fn body_shapes_for_genders(genders: &[String]) -> Option<Vec<String>> {
    if genders.is_empty() {
        return None;
    }
    let has_unisex = genders.iter().any(|g| g == "unisex");
    let has_male = has_unisex || genders.iter().any(|g| g == "male");
    let has_female = has_unisex || genders.iter().any(|g| g == "female");
    let mut out = Vec::new();
    if has_male {
        out.push("BaseMale".to_string());
    }
    if has_female {
        out.push("BaseFemale".to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn emote_play_mode_clause(modes: &[String]) -> Option<bool> {
    if modes.is_empty() || modes.len() == 2 {
        return None;
    }
    if modes.iter().any(|m| m == "loop") {
        Some(true)
    } else {
        Some(false)
    }
}
