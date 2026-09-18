use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Request, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_commons::cache::TtlMap;
use futures::future::join_all;
use serde_json::{json, Value};

use crate::errors::bad_request;
use crate::handlers::definitions::{
    extract_emote_definition, extract_wearable_definition, locale_cmp, rarity_rank,
};
use crate::handlers::nft_ownership;
use crate::query_params::{
    is_valid_eth_address, parse_pagination_with, parse_query_string, qs_get_array, qs_get_string,
    NonPositivePolicy, OversizePolicy, Pagination, QueryParams, MAX_PAGE_SIZE,
};
use crate::state::AppState;

fn pagination_object(params: &QueryParams, max_page_size: i64) -> Result<Pagination, Response> {
    parse_pagination_with(
        params,
        max_page_size,
        OversizePolicy::Reject,
        NonPositivePolicy::PassThrough,
    )
    .map_err(|msg| bad_request(&msg))
}

struct GroupedItem {
    urn: String,
    name: String,
    category: String,
    rarity: String,
    individual: Vec<IndividualData>,
    min_transferred_at: i64,
    max_transferred_at: i64,
}

struct IndividualData {
    token_id: String,
    transferred_at: i64,
    price: String,
}

/// One row per item urn; the representative fields come from the most recent copy.
#[derive(sqlx::FromRow)]
struct GroupRow {
    urn: String,
    name: String,
    category: String,
    rarity: String,
    token_ids: Vec<String>,
    transferred_ats: Vec<i64>,
    prices: Vec<String>,
    min_transferred_at: i64,
    max_transferred_at: i64,
}

impl From<GroupRow> for GroupedItem {
    fn from(r: GroupRow) -> Self {
        let individual = r
            .token_ids
            .into_iter()
            .zip(r.transferred_ats)
            .zip(r.prices)
            .map(|((token_id, transferred_at), price)| IndividualData {
                token_id,
                transferred_at,
                price,
            })
            .collect();
        GroupedItem {
            urn: r.urn,
            name: r.name,
            category: r.category,
            rarity: r.rarity,
            individual,
            min_transferred_at: r.min_transferred_at,
            max_transferred_at: r.max_transferred_at,
        }
    }
}

const OWNED_FETCH_LIMIT: i64 = (MAX_PAGE_SIZE as i64) * 100;

/// Groups and category/rarity-filters in SQL; name filter, sort, and paging stay in Rust (UCA order).
fn owned_grouped_sql(item_category: &str, overlay: bool) -> String {
    let meta_join = if item_category == "emote" {
        "LEFT JOIN squid_marketplace.metadata m ON n.metadata_id = m.id \
         LEFT JOIN squid_marketplace.emote md ON m.emote_id = md.id"
    } else {
        "LEFT JOIN squid_marketplace.metadata m ON n.metadata_id = m.id \
         LEFT JOIN squid_marketplace.wearable md ON m.wearable_id = md.id"
    };
    let grant_leg = if overlay {
        " UNION ALL \
             SELECT replace(ug.urn, ':mainnet:', ':ethereum:') AS urn, \
                    COALESCE(ug.token_id, '') AS token_id, \
                    extract(epoch FROM ug.granted_at)::bigint AS transferred_at, \
                    NULL::text AS rarity, \
                    NULL::text AS price, \
                    NULL::text AS name, \
                    ug.category::text AS category \
             FROM marketplace.usage_grants ug \
             WHERE ug.status = 'active' AND ug.category = $1 \
               AND ug.urn IS NOT NULL AND ug.grantee_address = lower($2)"
    } else {
        ""
    };
    const ORD: &str = "ORDER BY transferred_at DESC, token_id, price";
    format!(
        "SELECT * FROM ( \
             SELECT urn, \
                    (array_agg(COALESCE(name, '') {ORD}))[1] AS name, \
                    (array_agg(COALESCE(category, '') {ORD}))[1] AS category, \
                    (array_agg(COALESCE(rarity, '') {ORD}))[1] AS rarity, \
                    array_agg(token_id {ORD}) AS token_ids, \
                    array_agg(transferred_at {ORD}) AS transferred_ats, \
                    array_agg(COALESCE(price, '0') {ORD}) AS prices, \
                    min(transferred_at) AS min_transferred_at, \
                    max(transferred_at) AS max_transferred_at \
             FROM ( \
                 SELECT * FROM ( \
                     SELECT replace(n.urn, ':mainnet:', ':ethereum:') AS urn, \
                            n.token_id::text AS token_id, \
                            n.transferred_at::bigint AS transferred_at, \
                            i.rarity AS rarity, \
                            i.price::text AS price, \
                            md.name AS name, \
                            md.category AS category \
                     FROM squid_marketplace.nft n \
                     LEFT JOIN squid_marketplace.item i ON n.item_id = i.id \
                     {meta_join} \
                     WHERE n.category = $1 AND n.urn IS NOT NULL \
                       AND n.owner_address = lower($2) \
                   {grant_leg} \
                 ) copies \
                 ORDER BY transferred_at DESC \
                 LIMIT $3 \
             ) owned \
             GROUP BY urn \
         ) g \
         WHERE ($4::text IS NULL OR g.category = $4) \
           AND ($5::text IS NULL OR g.rarity = $5)"
    )
}

async fn fetch_owned_grouped(
    pool: &sqlx::PgPool,
    owner: &str,
    item_category: &str,
    filter_category: Option<&str>,
    filter_rarity: Option<&str>,
    filter_name: Option<&str>,
) -> Vec<GroupedItem> {
    let overlay = super::lease_overlay::usage_grants_present(pool).await;
    let rows: Vec<GroupRow> = sqlx::query_as(sqlx::AssertSqlSafe(owned_grouped_sql(
        item_category,
        overlay,
    )))
    .bind(item_category)
    .bind(owner)
    .bind(OWNED_FETCH_LIMIT)
    .bind(filter_category)
    .bind(filter_rarity)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut items: Vec<GroupedItem> = rows.into_iter().map(GroupedItem::from).collect();
    if let Some(n) = filter_name {
        let needle = n.to_lowercase();
        items.retain(|i| i.name.to_lowercase().contains(&needle));
    }
    items
}

fn validate_and_sort(items: &mut [GroupedItem], params: &QueryParams) -> Result<(), String> {
    let sort = qs_get_string(params, "orderBy")
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "rarity".to_string());
    let direction = qs_get_string(params, "direction")
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| {
            if sort == "name" {
                "ASC".to_string()
            } else {
                "DESC".to_string()
            }
        });

    let by_urn = |a: &GroupedItem, b: &GroupedItem| locale_cmp(&a.urn, &b.urn);

    match (sort.as_str(), direction.as_str()) {
        ("rarity", "DESC") => {
            items.sort_by(|a, b| {
                let c = rarity_rank(&b.rarity).cmp(&rarity_rank(&a.rarity));
                c.then_with(|| by_urn(a, b))
            });
        }
        ("rarity", "ASC") => {
            items.sort_by(|a, b| {
                let c = rarity_rank(&b.rarity).cmp(&rarity_rank(&a.rarity));
                c.then_with(|| by_urn(a, b)).reverse()
            });
        }
        ("name", "ASC") => {
            items.sort_by(|a, b| locale_cmp(&a.name, &b.name).then_with(|| by_urn(a, b)));
        }
        ("name", "DESC") => {
            items.sort_by(|a, b| {
                locale_cmp(&a.name, &b.name)
                    .then_with(|| by_urn(a, b))
                    .reverse()
            });
        }
        ("date", "ASC") => {
            items.sort_by(|a, b| {
                a.min_transferred_at
                    .cmp(&b.min_transferred_at)
                    .then_with(|| by_urn(b, a))
            });
        }
        ("date", "DESC") => {
            items.sort_by(|a, b| {
                b.max_transferred_at
                    .cmp(&a.max_transferred_at)
                    .then_with(|| by_urn(a, b))
            });
        }
        _ => return Err(format!("Invalid sorting requested: {} {}", sort, direction)),
    }
    Ok(())
}

fn grouped_to_json(item: &GroupedItem) -> Value {
    let individual: Vec<Value> = item
        .individual
        .iter()
        .map(|d| {
            json!({
                "id": format!("{}:{}", item.urn, d.token_id),
                "tokenId": d.token_id,
                "transferredAt": d.transferred_at.to_string(),
                "price": d.price,
            })
        })
        .collect();
    json!({
        "urn": item.urn,
        "amount": item.individual.len(),
        "individualData": individual,
        "name": item.name,
        "category": item.category,
        "rarity": item.rarity,
    })
}

async fn owned_items_response(
    state: &AppState,
    address: &str,
    item_category: &str,
    query_string: &str,
) -> Response {
    let params = parse_query_string(query_string);

    let include_definitions = params.contains_key("includeDefinitions");
    let include_entities = params.contains_key("includeEntities");
    if include_definitions && include_entities {
        return bad_request("Cannot use includeEntities and includeDefinitions together");
    }

    let pagination = match pagination_object(&params, MAX_PAGE_SIZE as i64) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => {
            return Json(json!({
                "elements": [],
                "totalAmount": 0,
                "pageNum": pagination.page_num,
                "pageSize": pagination.page_size,
            }))
            .into_response()
        }
    };

    let filter_category = qs_get_string(&params, "category");
    let filter_rarity = qs_get_string(&params, "rarity");
    let filter_name = qs_get_string(&params, "name");

    let mut items = fetch_owned_grouped(
        pool,
        address,
        item_category,
        filter_category.as_deref(),
        filter_rarity.as_deref(),
        filter_name.as_deref(),
    )
    .await;

    if let Err(msg) = validate_and_sort(&mut items, &params) {
        return bad_request(&msg);
    }

    let total_amount = items.len();

    let len = total_amount as i64;
    let js_index = |i: i64| -> i64 {
        if i < 0 {
            (len + i).max(0)
        } else {
            i.min(len)
        }
    };
    let start_idx = js_index(pagination.offset);
    let end_idx = js_index(pagination.offset + pagination.limit);
    let page: Vec<&GroupedItem> = if start_idx < end_idx {
        items[start_idx as usize..end_idx as usize].iter().collect()
    } else {
        Vec::new()
    };

    let mut definitions: Vec<Value> = Vec::new();
    if include_definitions || include_entities {
        let pointers: Vec<String> = page.iter().map(|i| i.urn.to_lowercase()).collect();
        let entities = state
            .database
            .active_entities_by_pointers(&pointers)
            .await
            .unwrap_or_default();

        use std::collections::HashMap;
        let mut by_pointer: HashMap<String, Value> = HashMap::new();
        for ent in entities {
            if let Some(ptrs) = ent.get("pointers").and_then(|p| p.as_array()) {
                for p in ptrs {
                    if let Some(s) = p.as_str() {
                        by_pointer.insert(s.to_lowercase(), ent.clone());
                    }
                }
            }
        }
        definitions = page
            .iter()
            .map(|i| {
                by_pointer
                    .get(&i.urn.to_lowercase())
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .collect();
    }

    let content_public_url = &state.content_public_url;
    let elements: Vec<Value> = page
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let mut obj = grouped_to_json(item);
            if include_definitions {
                let ent = definitions.get(idx).cloned().unwrap_or(Value::Null);
                let definition = if ent.is_null() {
                    Value::Null
                } else if item_category == "emote" {
                    extract_emote_definition(&ent, content_public_url).unwrap_or(Value::Null)
                } else {
                    extract_wearable_definition(&ent, content_public_url).unwrap_or(Value::Null)
                };
                obj["definition"] = definition;
            }
            if include_entities {
                let ent = definitions.get(idx).cloned().unwrap_or(Value::Null);
                obj["entity"] = ent;
            }
            obj
        })
        .collect();

    Json(json!({
        "elements": elements,
        "totalAmount": total_amount,
        "pageNum": pagination.page_num,
        "pageSize": pagination.page_size,
    }))
    .into_response()
}

pub async fn user_wearables(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    request: Request,
) -> Response {
    let query_string = request.uri().query().unwrap_or("").to_string();
    owned_items_response(&state, &address, "wearable", &query_string).await
}

pub async fn user_emotes(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    request: Request,
) -> Response {
    let query_string = request.uri().query().unwrap_or("").to_string();
    owned_items_response(&state, &address, "emote", &query_string).await
}

pub struct TpwElement {
    pub urn: String,

    pub individual_data: Vec<Value>,
    pub name: String,
    pub category: String,
    pub entity: Value,
}

impl TpwElement {
    fn to_value(&self) -> Value {
        json!({
            "urn": self.urn,
            "individualData": self.individual_data,
            "amount": self.individual_data.len(),
            "name": self.name,
            "category": self.category,
            "entity": self.entity,
        })
    }
}

pub async fn fetch_all_third_party_wearables(
    state: &AppState,
    owner: &str,
    only_name: Option<&str>,
) -> Vec<TpwElement> {
    use crate::handlers::external_graph;
    use std::collections::{HashMap, HashSet};

    let owner = owner.to_lowercase();
    let mut providers = external_graph::third_party_providers(&state.eth_network).await;

    providers.retain(|p| !p.contracts.is_empty());
    if let Some(name) = only_name {
        providers.retain(|p| p.id.split(':').nth(4) == Some(name));
    }
    if providers.is_empty() {
        return Vec::new();
    }

    let mut contracts_by_network: HashMap<String, Vec<String>> = HashMap::new();
    for p in &providers {
        for c in &p.contracts {
            let set = contracts_by_network.entry(c.network.clone()).or_default();
            if !set.contains(&c.address) {
                set.push(c.address.clone());
            }
        }
    }

    let (owned_nft_urns, entity_lists) = if external_graph::nft_worker_configured() {
        let owned = external_graph::owned_nfts(&owner, &contracts_by_network).await;
        if owned.is_empty() {
            return Vec::new();
        }
        let mut providers_to_check: HashSet<String> = HashSet::new();
        for urn in &owned {
            let parts: Vec<&str> = urn.split(':').collect();
            if parts.len() < 2 {
                continue;
            }
            let (network, contract) = (parts[0], parts[1]);
            for p in &providers {
                if p.contracts
                    .iter()
                    .any(|c| c.network == network && c.address == contract)
                {
                    providers_to_check.insert(p.id.clone());
                }
            }
        }
        if providers_to_check.is_empty() {
            return Vec::new();
        }
        let lists =
            collection_entity_lists(state, providers_to_check.iter().map(String::as_str)).await;
        (owned, lists)
    } else {
        let lists = collection_entity_lists(state, providers.iter().map(|p| p.id.as_str())).await;
        let mut candidates =
            nft_ownership::candidates_from_mappings(lists.iter().flat_map(|l| l.iter()));
        candidates.retain(|(network, contract), _| {
            contracts_by_network
                .get(network)
                .is_some_and(|cs| cs.contains(contract))
        });
        let owned = nft_ownership::owned_via_rpc(&owner, &candidates).await;
        if owned.is_empty() {
            return Vec::new();
        }
        (owned, lists)
    };

    let mut by_urn: HashMap<String, TpwElement> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for entity in entity_lists.iter().flat_map(|l| l.iter()) {
        let meta = match entity.get("metadata") {
            Some(m) => m,
            None => continue,
        };
        let mappings = match meta.get("mappings") {
            Some(m) if !m.is_null() => m,
            _ => continue,
        };
        let entity_urn = match meta.get("id").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };

        for nft in &owned_nft_urns {
            let parts: Vec<&str> = nft.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let (network, contract, token_id) = (parts[0], parts[1], parts[2]);
            if !external_graph::mappings_includes_nft(mappings, network, contract, token_id) {
                continue;
            }
            let elem = by_urn.entry(entity_urn.clone()).or_insert_with(|| {
                order.push(entity_urn.clone());
                let name = meta
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let category = meta
                    .get("data")
                    .and_then(|d| d.get("category"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                TpwElement {
                    urn: entity_urn.clone(),
                    individual_data: Vec::new(),
                    name,
                    category,
                    entity: entity.clone(),
                }
            });
            elem.individual_data.push(json!({
                "id": format!("{}:{}", entity_urn, nft),
                "tokenId": nft,
            }));
        }
    }

    order
        .into_iter()
        .filter_map(|u| by_urn.remove(&u))
        .collect()
}

const COLLECTION_ENTITIES_TTL: Duration = Duration::from_secs(300);

fn collection_entities_cache() -> &'static TtlMap<String, Arc<Vec<Value>>> {
    static C: OnceLock<TtlMap<String, Arc<Vec<Value>>>> = OnceLock::new();
    C.get_or_init(|| {
        TtlMap::bounded(
            "third-party-collection-entities",
            COLLECTION_ENTITIES_TTL,
            256,
        )
    })
}

async fn collection_entity_lists<'a>(
    state: &AppState,
    providers: impl Iterator<Item = &'a str>,
) -> Vec<Arc<Vec<Value>>> {
    join_all(providers.map(|id| {
        let id = id.to_string();
        async move {
            collection_entities_cache()
                .get_or_fetch(id.clone(), || async {
                    Ok::<Arc<Vec<Value>>, ()>(Arc::new(fetch_collection_entities(state, &id).await))
                })
                .await
                .unwrap_or_default()
        }
    }))
    .await
}

fn has_mappings(entity: &Value) -> bool {
    entity
        .get("metadata")
        .and_then(|m| m.get("mappings"))
        .map(|m| !m.is_null())
        .unwrap_or(false)
}

/// First page sequentially for the total, the remaining pages concurrently in page order.
async fn fetch_collection_entities(state: &AppState, collection_id: &str) -> Vec<Value> {
    const PAGE: i64 = 1000;
    const PAGE_CONCURRENCY: usize = 4;
    use futures::stream::StreamExt;

    let Ok(first) = state
        .database
        .active_entities_by_prefix(collection_id, 0, PAGE)
        .await
    else {
        return Vec::new();
    };
    let short = (first.entities.len() as i64) < PAGE;
    let mut out: Vec<Value> = first.entities.into_iter().filter(has_mappings).collect();
    if short || first.total <= PAGE {
        return out;
    }
    let rest = (PAGE..first.total).step_by(PAGE as usize).map(|offset| {
        state
            .database
            .active_entities_by_prefix(collection_id, offset, PAGE)
    });
    let mut pages = futures::stream::iter(rest).buffered(PAGE_CONCURRENCY);
    while let Some(page) = pages.next().await {
        let Ok(page) = page else {
            break;
        };
        let short = (page.entities.len() as i64) < PAGE;
        out.extend(page.entities.into_iter().filter(has_mappings));
        if short {
            break;
        }
    }
    out
}

fn tpw_passes_filter(e: &TpwElement, categories: &[String], name: &Option<String>) -> bool {
    if !categories.is_empty() && !categories.contains(&e.category) {
        return false;
    }
    if let Some(n) = name {
        if !e.name.to_lowercase().contains(&n.to_lowercase()) {
            return false;
        }
    }
    true
}

pub async fn user_third_party_wearables(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    request: Request,
) -> Response {
    if !is_valid_eth_address(&address.to_lowercase()) {
        return bad_request("Address must be a valid Ethereum address");
    }

    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);

    let pagination = match pagination_object(&params, MAX_PAGE_SIZE as i64) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let categories: Vec<String> = qs_get_array(&params, "category");
    let name = qs_get_string(&params, "name");

    let sort = qs_get_string(&params, "orderBy")
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "name".to_string());
    let direction = qs_get_string(&params, "direction")
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| {
            if sort == "name" {
                "ASC".into()
            } else {
                "DESC".into()
            }
        });
    if !(sort == "name" && (direction == "ASC" || direction == "DESC")) {
        return bad_request(&format!(
            "Invalid sorting requested: {} {}",
            sort, direction
        ));
    }

    let mut elements = fetch_all_third_party_wearables(&state, &address, None).await;
    elements.retain(|e| tpw_passes_filter(e, &categories, &name));

    elements.sort_by(|a, b| {
        let primary = locale_cmp(&a.name, &b.name).then_with(|| locale_cmp(&a.urn, &b.urn));
        if direction == "DESC" {
            primary.reverse()
        } else {
            primary
        }
    });

    let include_definitions = params.contains_key("includeDefinitions");
    finish_tpw_response(&state, elements, &pagination, include_definitions).await
}

pub async fn user_third_party_collection_wearables(
    State(state): State<Arc<AppState>>,
    Path((address, collection_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !is_valid_eth_address(&address.to_lowercase()) {
        return bad_request("Address must be a valid Ethereum address");
    }

    let cleaned: String = collection_id
        .split(':')
        .take(5)
        .collect::<Vec<_>>()
        .join(":");

    if !is_third_party_name_urn(&cleaned) {
        return bad_request(&format!(
            "Invalid collection id: {} not a valid URN",
            collection_id
        ));
    }

    let name = cleaned.split(':').nth(4).unwrap_or("").to_string();

    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);
    let pagination = match pagination_object(&params, MAX_PAGE_SIZE as i64) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let mut elements = fetch_all_third_party_wearables(&state, &address, Some(&name)).await;

    elements.sort_by(|a, b| locale_cmp(&a.name, &b.name).then_with(|| locale_cmp(&a.urn, &b.urn)));

    let include_definitions = params.contains_key("includeDefinitions");
    finish_tpw_response(&state, elements, &pagination, include_definitions).await
}

async fn finish_tpw_response(
    state: &AppState,
    elements: Vec<TpwElement>,
    pagination: &Pagination,
    include_definitions: bool,
) -> Response {
    let total = elements.len() as i64;

    let js_index = |i: i64| -> i64 {
        if i < 0 {
            (total + i).max(0)
        } else {
            i.min(total)
        }
    };
    let start = js_index(pagination.offset);
    let end = js_index(pagination.offset + pagination.limit);
    let page: Vec<&TpwElement> = if start < end {
        elements[start as usize..end as usize].iter().collect()
    } else {
        Vec::new()
    };

    let content_public_url = &state.content_public_url;
    let out: Vec<Value> = page
        .iter()
        .map(|e| {
            let mut v = e.to_value();
            if include_definitions {
                let def = extract_wearable_definition(&e.entity, content_public_url)
                    .unwrap_or(Value::Null);
                v.as_object_mut()
                    .unwrap()
                    .insert("definition".to_string(), def);
            }
            v
        })
        .collect();

    Json(json!({
        "elements": out,
        "totalAmount": total,
        "pageNum": pagination.page_num,
        "pageSize": pagination.page_size,
    }))
    .into_response()
}

pub(crate) fn is_third_party_name_urn(urn: &str) -> bool {
    let p: Vec<&str> = urn.split(':').collect();
    p.len() == 5
        && p[0] == "urn"
        && p[1] == "decentraland"
        && p[3] == "collections-thirdparty"
        && !p[4].is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "0xaaaa000000000000000000000000000000000001";
    const HAT: &str = "urn:decentraland:matic:collections-v2:0xc0ffee:1";
    const CAPE: &str = "urn:decentraland:matic:collections-v2:0xc0ffee:2";
    const OLD_HAT: &str = "urn:decentraland:ethereum:collections-v1:oldies:hat";

    const SQUID_DDL: &str = "
        CREATE TABLE squid_marketplace.item (id TEXT PRIMARY KEY, rarity TEXT, price NUMERIC);
        CREATE TABLE squid_marketplace.wearable (id TEXT PRIMARY KEY, name TEXT, category TEXT);
        CREATE TABLE squid_marketplace.emote (id TEXT PRIMARY KEY, name TEXT, category TEXT);
        CREATE TABLE squid_marketplace.metadata (id TEXT PRIMARY KEY, wearable_id TEXT, emote_id TEXT);
        CREATE TABLE squid_marketplace.nft (
            id TEXT PRIMARY KEY, owner_address TEXT NOT NULL, category TEXT, urn TEXT,
            token_id NUMERIC, transferred_at BIGINT, item_id TEXT, metadata_id TEXT);
        INSERT INTO squid_marketplace.item VALUES ('i-hat', 'epic', 10), ('i-cape', 'rare', 5);
        INSERT INTO squid_marketplace.wearable VALUES ('w-hat', 'Hat', 'hat'), ('w-cape', 'Cape', 'upper_body');
        INSERT INTO squid_marketplace.metadata VALUES ('m-hat', 'w-hat', NULL), ('m-cape', 'w-cape', NULL);
        INSERT INTO squid_marketplace.nft VALUES
            ('n-1', '0xaaaa000000000000000000000000000000000001', 'wearable', 'urn:decentraland:matic:collections-v2:0xc0ffee:1', 7, 100, 'i-hat', 'm-hat'),
            ('n-2', '0xaaaa000000000000000000000000000000000001', 'wearable', 'urn:decentraland:matic:collections-v2:0xc0ffee:1', 3, 300, 'i-hat', 'm-hat'),
            ('n-3', '0xaaaa000000000000000000000000000000000001', 'wearable', 'urn:decentraland:matic:collections-v2:0xc0ffee:1', 5, 200, 'i-hat', 'm-hat'),
            ('n-4', '0xaaaa000000000000000000000000000000000001', 'wearable', 'urn:decentraland:matic:collections-v2:0xc0ffee:2', 9, 250, 'i-cape', 'm-cape'),
            ('n-5', '0xaaaa000000000000000000000000000000000001', 'wearable', 'urn:decentraland:mainnet:collections-v1:oldies:hat', 1, 50, NULL, NULL),
            ('n-6', '0xaaaa000000000000000000000000000000000001', 'emote', 'urn:decentraland:matic:collections-v2:0xc0ffee:3', 1, 400, NULL, NULL),
            ('n-7', '0xbbbb000000000000000000000000000000000002', 'wearable', 'urn:decentraland:matic:collections-v2:0xc0ffee:1', 8, 500, 'i-hat', 'm-hat');
    ";

    struct PagedDb {
        entities: Vec<Value>,
        fail_at: Option<i64>,
        offsets: std::sync::Mutex<Vec<i64>>,
    }

    #[async_trait::async_trait]
    impl crate::state::Database for PagedDb {
        async fn active_entities_by_pointers(
            &self,
            _pointers: &[String],
        ) -> Result<Vec<Value>, crate::state::DatabaseError> {
            Ok(Vec::new())
        }
        async fn active_entities_by_ids(
            &self,
            _ids: &[String],
        ) -> Result<Vec<Value>, crate::state::DatabaseError> {
            Ok(Vec::new())
        }
        async fn active_entities_by_prefix(
            &self,
            _prefix: &str,
            offset: i64,
            limit: i64,
        ) -> Result<crate::state::PrefixQueryResult, crate::state::DatabaseError> {
            self.offsets.lock().unwrap().push(offset);
            if self.fail_at == Some(offset) {
                return Err(crate::state::DatabaseError::Unsupported("boom".into()));
            }
            let page = self
                .entities
                .iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect();
            Ok(crate::state::PrefixQueryResult {
                total: self.entities.len() as i64,
                entities: page,
            })
        }
        async fn active_entity_ids_by_content_hash(
            &self,
            _hash: &str,
        ) -> Result<Vec<String>, crate::state::DatabaseError> {
            Ok(Vec::new())
        }
        async fn get_deployments(
            &self,
            _options: &crate::state::DeploymentQueryOptions,
        ) -> Result<crate::state::DeploymentQueryResult, crate::state::DatabaseError> {
            Err(crate::state::DatabaseError::Unsupported("stub".into()))
        }
        async fn get_pointer_changes(
            &self,
            _options: &crate::state::PointerChangesQueryOptions,
        ) -> Result<crate::state::PointerChangesQueryResult, crate::state::DatabaseError> {
            Err(crate::state::DatabaseError::Unsupported("stub".into()))
        }
        async fn get_failed_deployments(&self) -> Result<Vec<Value>, crate::state::DatabaseError> {
            Ok(Vec::new())
        }
        async fn get_audit_info(
            &self,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<Option<Value>, crate::state::DatabaseError> {
            Ok(None)
        }
        async fn find_entity_by_pointer(
            &self,
            _pointer: &str,
        ) -> Result<Option<Value>, crate::state::DatabaseError> {
            Ok(None)
        }
    }

    fn paged_state(n: usize, fail_at: Option<i64>) -> (Arc<AppState>, Arc<PagedDb>) {
        let entities = (0..n)
            .map(|i| {
                let mappings = if i % 3 == 0 { Value::Null } else { json!({}) };
                json!({ "id": format!("Qm{i}"), "metadata": { "mappings": mappings } })
            })
            .collect();
        let db = Arc::new(PagedDb {
            entities,
            fail_at,
            offsets: Default::default(),
        });
        (crate::test_support::app_state_with_database(db.clone()), db)
    }

    #[tokio::test]
    async fn collection_pages_are_fetched_concurrently_in_order() {
        let (state, db) = paged_state(2500, None);
        let got = fetch_collection_entities(&state, "urn:x").await;
        let ids: Vec<&str> = got.iter().map(|e| e["id"].as_str().unwrap()).collect();
        let want: Vec<String> = (0..2500)
            .filter(|i| i % 3 != 0)
            .map(|i| format!("Qm{i}"))
            .collect();
        assert_eq!(ids, want, "every page, page order, mappings-only");
        let mut offsets = db.offsets.lock().unwrap().clone();
        offsets.sort();
        assert_eq!(offsets, vec![0, 1000, 2000]);

        let (state, db) = paged_state(900, None);
        assert_eq!(fetch_collection_entities(&state, "urn:x").await.len(), 600);
        assert_eq!(
            *db.offsets.lock().unwrap(),
            vec![0],
            "a short first page ends it"
        );

        let (state, _) = paged_state(2500, Some(1000));
        let got = fetch_collection_entities(&state, "urn:x").await;
        assert_eq!(got.len(), 666, "a failed page keeps the pages before it");

        let (state, _) = paged_state(10, Some(0));
        assert!(fetch_collection_entities(&state, "urn:x").await.is_empty());
    }

    #[tokio::test]
    async fn owned_groups_come_back_grouped_and_filtered_by_sql() {
        let Some(db) =
            crate::handlers::profile_processing::pg_scratch::ScratchSquid::new(SQUID_DDL).await
        else {
            return;
        };
        let pool = &db.pool;

        let mut items = fetch_owned_grouped(pool, OWNER, "wearable", None, None, None).await;
        items.sort_by(|a, b| a.urn.cmp(&b.urn));
        assert_eq!(urns(&items), vec![OLD_HAT, HAT, CAPE]);

        let hat = &items[1];
        assert_eq!(
            (
                hat.name.as_str(),
                hat.category.as_str(),
                hat.rarity.as_str()
            ),
            ("Hat", "hat", "epic")
        );
        assert_eq!((hat.min_transferred_at, hat.max_transferred_at), (100, 300));
        let copies: Vec<(&str, i64, &str)> = hat
            .individual
            .iter()
            .map(|d| (d.token_id.as_str(), d.transferred_at, d.price.as_str()))
            .collect();
        assert_eq!(
            copies,
            vec![("3", 300, "10"), ("5", 200, "10"), ("7", 100, "10")],
            "copies newest first"
        );
        let old = &items[0];
        assert_eq!(
            (
                old.name.as_str(),
                old.category.as_str(),
                old.rarity.as_str()
            ),
            ("", "", "")
        );
        assert_eq!(old.individual[0].price, "0", "no item row means price 0");

        let by_category =
            fetch_owned_grouped(pool, OWNER, "wearable", Some("hat"), None, None).await;
        assert_eq!(urns(&by_category), vec![HAT]);
        let by_rarity =
            fetch_owned_grouped(pool, OWNER, "wearable", None, Some("rare"), None).await;
        assert_eq!(urns(&by_rarity), vec![CAPE]);
        let by_name = fetch_owned_grouped(pool, OWNER, "wearable", None, None, Some("cAp")).await;
        assert_eq!(urns(&by_name), vec![CAPE]);
        let none =
            fetch_owned_grouped(pool, OWNER, "wearable", Some("hat"), Some("rare"), None).await;
        assert!(none.is_empty());
        let empty_category =
            fetch_owned_grouped(pool, OWNER, "wearable", Some(""), None, None).await;
        assert_eq!(
            urns(&empty_category),
            vec![OLD_HAT],
            "an empty filter keeps unnamed items"
        );
        let upper =
            fetch_owned_grouped(pool, &OWNER.to_uppercase(), "wearable", None, None, None).await;
        assert_eq!(upper.len(), 3, "owner lookups are case-insensitive");
        let emotes = fetch_owned_grouped(pool, OWNER, "emote", None, None, None).await;
        assert_eq!(emotes.len(), 1);

        db.drop().await;
    }

    fn item(urn: &str, name: &str, rarity: &str, min_at: i64, max_at: i64) -> GroupedItem {
        GroupedItem {
            urn: urn.to_string(),
            name: name.to_string(),
            category: "hat".to_string(),
            rarity: rarity.to_string(),
            individual: Vec::new(),
            min_transferred_at: min_at,
            max_transferred_at: max_at,
        }
    }

    fn urns(items: &[GroupedItem]) -> Vec<&str> {
        items.iter().map(|i| i.urn.as_str()).collect()
    }

    #[test]
    fn rarity_rank_matches_reference_order() {
        assert_eq!(rarity_rank("common"), 0);
        assert_eq!(rarity_rank("legendary"), 4);
        assert_eq!(rarity_rank("unique"), 7);

        assert_eq!(rarity_rank("not-a-rarity"), -1);
        assert_eq!(rarity_rank(""), -1);
    }

    #[test]
    fn sort_rarest_then_urn() {
        let mut items = vec![
            item("urn:b", "B", "common", 0, 0),
            item("urn:a", "A", "legendary", 0, 0),
            item("urn:c", "C", "legendary", 0, 0),
        ];
        let p = parse_query_string("orderBy=rarity&direction=DESC");
        validate_and_sort(&mut items, &p).unwrap();

        assert_eq!(urns(&items), vec!["urn:a", "urn:c", "urn:b"]);
    }

    #[test]
    fn sort_least_rare_is_full_reverse() {
        let mut rarest = vec![
            item("urn:b", "B", "common", 0, 0),
            item("urn:a", "A", "legendary", 0, 0),
            item("urn:c", "C", "legendary", 0, 0),
        ];
        let mut least = vec![
            item("urn:b", "B", "common", 0, 0),
            item("urn:a", "A", "legendary", 0, 0),
            item("urn:c", "C", "legendary", 0, 0),
        ];
        validate_and_sort(
            &mut rarest,
            &parse_query_string("orderBy=rarity&direction=DESC"),
        )
        .unwrap();
        validate_and_sort(
            &mut least,
            &parse_query_string("orderBy=rarity&direction=ASC"),
        )
        .unwrap();

        let rev: Vec<&str> = urns(&rarest).into_iter().rev().collect();
        assert_eq!(urns(&least), rev);
        assert_eq!(urns(&least), vec!["urn:b", "urn:c", "urn:a"]);
    }

    #[test]
    fn sort_name_az_and_za() {
        let mut az = vec![
            item("urn:2", "Zebra", "common", 0, 0),
            item("urn:1", "Apple", "common", 0, 0),
        ];
        validate_and_sort(&mut az, &parse_query_string("orderBy=name&direction=ASC")).unwrap();
        assert_eq!(urns(&az), vec!["urn:1", "urn:2"]);

        let mut za = vec![
            item("urn:1", "Apple", "common", 0, 0),
            item("urn:2", "Zebra", "common", 0, 0),
        ];
        validate_and_sort(&mut za, &parse_query_string("orderBy=name&direction=DESC")).unwrap();
        assert_eq!(urns(&za), vec!["urn:2", "urn:1"]);
    }

    #[test]
    fn name_default_direction_is_asc() {
        let mut items = vec![
            item("urn:2", "Zebra", "common", 0, 0),
            item("urn:1", "Apple", "common", 0, 0),
        ];

        validate_and_sort(&mut items, &parse_query_string("orderBy=name")).unwrap();
        assert_eq!(urns(&items), vec!["urn:1", "urn:2"]);
    }

    #[test]
    fn sort_date_oldest_has_reversed_urn_tiebreak() {
        let mut oldest = vec![
            item("urn:a", "A", "common", 10, 10),
            item("urn:b", "B", "common", 10, 10),
        ];
        validate_and_sort(
            &mut oldest,
            &parse_query_string("orderBy=date&direction=ASC"),
        )
        .unwrap();

        assert_eq!(urns(&oldest), vec!["urn:b", "urn:a"]);

        let mut newest = vec![
            item("urn:b", "B", "common", 10, 10),
            item("urn:a", "A", "common", 10, 10),
        ];
        validate_and_sort(
            &mut newest,
            &parse_query_string("orderBy=date&direction=DESC"),
        )
        .unwrap();

        assert_eq!(urns(&newest), vec!["urn:a", "urn:b"]);
    }

    #[test]
    fn sort_date_newest_orders_by_max_desc() {
        let mut items = vec![
            item("urn:a", "A", "common", 5, 5),
            item("urn:b", "B", "common", 9, 9),
        ];
        validate_and_sort(
            &mut items,
            &parse_query_string("orderBy=date&direction=DESC"),
        )
        .unwrap();
        assert_eq!(urns(&items), vec!["urn:b", "urn:a"]);
    }

    #[test]
    fn invalid_sort_combination_errors_with_reference_message() {
        let mut items = vec![item("urn:a", "A", "common", 0, 0)];
        let err = validate_and_sort(
            &mut items,
            &parse_query_string("orderBy=bogus&direction=ASC"),
        )
        .unwrap_err();
        assert_eq!(err, "Invalid sorting requested: bogus ASC");

        let err2 = validate_and_sort(
            &mut items,
            &parse_query_string("orderBy=rarity&direction=sideways"),
        )
        .unwrap_err();
        assert_eq!(err2, "Invalid sorting requested: rarity SIDEWAYS");
    }

    #[test]
    fn default_orderby_is_rarity_desc() {
        let mut items = vec![
            item("urn:b", "B", "common", 0, 0),
            item("urn:a", "A", "legendary", 0, 0),
        ];

        validate_and_sort(&mut items, &parse_query_string("")).unwrap();
        assert_eq!(urns(&items), vec!["urn:a", "urn:b"]);
    }
}
