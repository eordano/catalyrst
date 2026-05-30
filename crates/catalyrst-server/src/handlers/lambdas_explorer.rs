use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Request, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::cache::ResponseCache;
use crate::errors::bad_request;
use crate::handlers::definitions::{rarity_rank, SORTED_RARITIES};
use crate::query_params::{
    parse_query_string, qs_get_array, qs_get_string, QueryParams, MAX_PAGE_SIZE as SHARED_MAX_PAGE_SIZE,
};
use crate::state::AppState;

const EXPLORER_CACHE_TTL: Duration = Duration::from_secs(30);
const EXPLORER_CACHE_MAX_ENTRIES: usize = 50_000;

type ExplorerKey = (String, String, String, i64, i64, String, &'static str, String);

fn explorer_cache() -> &'static Arc<ResponseCache<ExplorerKey, Value>> {
    static C: OnceLock<Arc<ResponseCache<ExplorerKey, Value>>> = OnceLock::new();
    C.get_or_init(|| {
        Arc::new(ResponseCache::new(
            "explorer",
            EXPLORER_CACHE_TTL,
            EXPLORER_CACHE_MAX_ENTRIES,
        ))
    })
}

const DEFAULT_PAGE_SIZE: i64 = 100;
const MAX_PAGE_SIZE: i64 = SHARED_MAX_PAGE_SIZE as i64;
const DEFAULT_PAGE_NUM: i64 = 1;

const VALID_COLLECTION_TYPES: [&str; 3] = ["base-wearable", "on-chain", "third-party"];

struct ExplorerQuery {
    page_num: i64,
    page_size: i64,
    name: Option<String>,
    categories: Vec<String>,
    rarity: Option<String>,
    sort: String,
    direction: String,

    collection_types: Vec<String>,

    // The unity backpack always requests trimmed=true and deserializes
    // elements[].entity.{id,thumbnail,metadata,individualData} — a different
    // element shape than the default (un-trimmed) response.
    trimmed: bool,
}

fn parse_query(query: Option<&str>, valid_collection_types: &[&str]) -> Result<ExplorerQuery, Response> {

    let params: QueryParams = parse_query_string(query.unwrap_or(""));

    let get_first = |key: &str| -> Option<String> { qs_get_string(&params, key) };

    let collection_types: Vec<String> = qs_get_array(&params, "collectionType");
    if collection_types
        .iter()
        .any(|t| !valid_collection_types.contains(&t.as_str()))
    {
        return Err(bad_request(&format!(
            "Invalid collection type. Valid types are: {}.",
            valid_collection_types.join(", ")
        )));
    }

    let page_size = match get_first("pageSize") {
        Some(s) => s.parse::<i64>().unwrap_or(DEFAULT_PAGE_SIZE),
        None => DEFAULT_PAGE_SIZE,
    };
    if page_size > MAX_PAGE_SIZE {
        return Err(bad_request(&format!("max allowed pageSize is {}", MAX_PAGE_SIZE)));
    }
    let page_num = match get_first("pageNum") {
        Some(s) => s.parse::<i64>().unwrap_or(DEFAULT_PAGE_NUM),
        None => DEFAULT_PAGE_NUM,
    };

    let name = get_first("name").map(|n| n.to_lowercase());

    let categories: Vec<String> = qs_get_array(&params, "category")
        .iter()
        .map(|v| v.to_lowercase())
        .collect();

    let rarity = get_first("rarity").map(|r| r.to_lowercase());
    if let Some(r) = &rarity {
        if !SORTED_RARITIES.contains(&r.as_str()) {
            return Err(bad_request(&format!("Invalid rarity requested: '{}'.", r)));
        }
    }

    let sort = get_first("orderBy")
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "rarity".to_string());
    let direction = match get_first("direction") {
        Some(d) => d.to_uppercase(),
        None => {
            if sort == "name" {
                "ASC".to_string()
            } else {
                "DESC".to_string()
            }
        }
    };

    let valid = matches!(
        (sort.as_str(), direction.as_str()),
        ("rarity", "ASC")
            | ("rarity", "DESC")
            | ("name", "ASC")
            | ("name", "DESC")
            | ("date", "ASC")
            | ("date", "DESC")
    );
    if !valid {
        return Err(bad_request(&format!(
            "Invalid sorting requested: '{} {}'. Valid options are '[rarity, name, date] [ASC, DESC]'.",
            sort, direction
        )));
    }

    let trimmed = matches!(
        get_first("trimmed").as_deref(),
        Some("true") | Some("1")
    );

    Ok(ExplorerQuery {
        page_num,
        page_size,
        name,
        categories,
        rarity,
        sort,
        direction,
        collection_types,
        trimmed,
    })
}

struct OwnedItem {
    urn: String,
    individual_data: Vec<Value>,
    rarity: String,
    item_type: Option<String>,
    amount: i64,
    name: String,
    category: String,
    min_transferred_at: f64,
    max_transferred_at: f64,
    entity: Value,

    is_base: bool,

    is_third_party: bool,

    has_date: bool,
}

async fn fetch_owned_items(state: &AppState, owner: &str, category: &str) -> Vec<OwnedItem> {
    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let rows: Vec<(String, String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT replace(n.urn, ':mainnet:', ':ethereum:') AS urn, \
                    n.token_id::text AS token_id, \
                    n.transferred_at::bigint::text AS transferred_at, \
                    COALESCE(i.rarity, n.search_wearable_rarity, n.search_emote_rarity) AS rarity, \
                    i.price::text AS price, \
                    n.item_type AS item_type \
             FROM squid_marketplace.nft n \
             LEFT JOIN squid_marketplace.item i ON i.id = n.item_id \
             WHERE n.category = $1 \
               AND n.urn IS NOT NULL \
               AND n.owner_address = lower($2) \
             ORDER BY n.transferred_at DESC",
        )
        .bind(category)
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    if rows.is_empty() {
        return Vec::new();
    }

    let mut grouped: HashMap<String, OwnedItem> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (urn, token_id, transferred_at, rarity, price, item_type) in rows {
        let transferred_num = transferred_at.parse::<f64>().unwrap_or(0.0);
        let individual = json!({
            "id": format!("{}:{}", urn, token_id),
            "tokenId": token_id,
            "transferredAt": transferred_at,
            "price": price.unwrap_or_default(),
        });
        match grouped.get_mut(&urn) {
            Some(existing) => {
                existing.individual_data.push(individual);
                existing.amount += 1;
                if transferred_num < existing.min_transferred_at {
                    existing.min_transferred_at = transferred_num;
                }
                if transferred_num > existing.max_transferred_at {
                    existing.max_transferred_at = transferred_num;
                }
            }
            None => {
                order.push(urn.clone());
                grouped.insert(
                    urn.clone(),
                    OwnedItem {
                        urn: urn.clone(),
                        individual_data: vec![individual],
                        rarity: rarity.unwrap_or_default(),
                        item_type,
                        amount: 1,
                        name: String::new(),
                        category: String::new(),
                        min_transferred_at: transferred_num,
                        max_transferred_at: transferred_num,
                        entity: Value::Null,
                        is_base: false,
                        is_third_party: false,
                        has_date: true,
                    },
                );
            }
        }
    }

    let pointers: Vec<String> = order.clone();
    let entities = state
        .database
        .active_entities_by_pointers(&pointers)
        .await
        .unwrap_or_default();

    let mut entity_by_pointer: HashMap<String, Value> = HashMap::new();
    for entity in &entities {
        if let Some(ptrs) = entity.get("pointers").and_then(|p| p.as_array()) {
            for ptr in ptrs {
                if let Some(s) = ptr.as_str() {
                    entity_by_pointer
                        .entry(s.to_lowercase())
                        .or_insert_with(|| entity.clone());
                }
            }
        }
    }

    let mut items: Vec<OwnedItem> = Vec::new();
    for urn in order {
        let mut item = match grouped.remove(&urn) {
            Some(it) => it,
            None => continue,
        };
        let entity = match entity_by_pointer.get(&urn.to_lowercase()) {
            Some(e) => e.clone(),
            None => continue,
        };
        let (name, cat) = extract_name_category(&entity, category);
        item.name = name;
        item.category = cat;
        item.entity = entity;
        items.push(item);
    }

    items
}

fn extract_name_category(entity: &Value, category: &str) -> (String, String) {
    let md = entity.get("metadata").cloned().unwrap_or(Value::Null);
    // v1-collection metadata ships name:"" with the display name only in the i18n
    // table — fall back to the first i18n text so sorting/searching and the
    // backpack grid label get a real name.
    let mut name = md
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        name = md
            .get("i18n")
            .and_then(|i| i.as_array())
            .and_then(|arr| arr.first())
            .and_then(|e| e.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
    }
    let cat = if category == "emote" {
        md.get("emoteDataADR74")
            .or_else(|| md.get("emoteDataV0"))
            .and_then(|d| d.get("category"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        md.get("data")
            .and_then(|d| d.get("category"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    };
    (name, cat)
}

use super::definitions::locale_cmp;

fn sort_items(items: &mut [OwnedItem], sort: &str, direction: &str) {
    let by_urn = |a: &OwnedItem, b: &OwnedItem| locale_cmp(&a.urn, &b.urn);
    match (sort, direction) {
        ("rarity", dir) => {
            items.sort_by(|a, b| {
                let ra = rarity_rank(&a.rarity);
                let rb = rarity_rank(&b.rarity);

                let has_a = !a.rarity.is_empty() && ra >= 0;
                let has_b = !b.rarity.is_empty() && rb >= 0;
                let cmp = match (has_a, has_b) {
                    (true, true) => rb.cmp(&ra).then_with(|| by_urn(a, b)),
                    (false, false) => by_urn(a, b),
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                };
                if dir == "ASC" {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        ("name", dir) => {
            items.sort_by(|a, b| {

                let cmp = locale_cmp(&a.name, &b.name).then_with(|| by_urn(a, b));
                if dir == "DESC" {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        ("date", dir) => {
            items.sort_by(|a, b| {
                use std::cmp::Ordering;
                if dir == "DESC" {

                    match (a.has_date, b.has_date) {
                        (true, true) => b
                            .max_transferred_at
                            .partial_cmp(&a.max_transferred_at)
                            .unwrap_or(Ordering::Equal)
                            .then_with(|| by_urn(a, b)),
                        (false, false) => by_urn(a, b),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                    }
                } else {

                    match (a.has_date, b.has_date) {
                        (true, true) => a
                            .min_transferred_at
                            .partial_cmp(&b.min_transferred_at)
                            .unwrap_or(Ordering::Equal)
                            .then_with(|| by_urn(b, a)),
                        (false, false) => by_urn(a, b),
                        (true, false) => Ordering::Greater,
                        (false, true) => Ordering::Less,
                    }
                }
            });
        }
        _ => {}
    }
}

fn passes_filters(item: &OwnedItem, q: &ExplorerQuery) -> bool {
    if let Some(rarity) = &q.rarity {
        if item.rarity.is_empty() || &item.rarity != rarity {
            return false;
        }
    }
    if let Some(name) = &q.name {
        if item.name.is_empty() || !item.name.to_lowercase().contains(name) {
            return false;
        }
    }
    if !q.categories.is_empty()
        && (item.category.is_empty() || !q.categories.contains(&item.category))
    {
        return false;
    }
    true
}

fn item_to_value(item: &OwnedItem, include_item_type: bool) -> Value {
    let mut obj = serde_json::Map::new();
    if item.is_base {

        obj.insert("type".to_string(), json!("base-wearable"));
        obj.insert("urn".to_string(), json!(item.urn));
        obj.insert("name".to_string(), json!(item.name));
        obj.insert("category".to_string(), json!(item.category));
        obj.insert(
            "individualData".to_string(),
            Value::Array(item.individual_data.clone()),
        );
        obj.insert("amount".to_string(), json!(item.amount));
        obj.insert("entity".to_string(), item.entity.clone());
        return Value::Object(obj);
    }
    if item.is_third_party {

        obj.insert("type".to_string(), json!("third-party"));
        obj.insert("urn".to_string(), json!(item.urn));
        obj.insert(
            "individualData".to_string(),
            Value::Array(item.individual_data.clone()),
        );
        obj.insert("amount".to_string(), json!(item.amount));
        obj.insert("name".to_string(), json!(item.name));
        obj.insert("category".to_string(), json!(item.category));
        obj.insert("entity".to_string(), item.entity.clone());
        return Value::Object(obj);
    }
    obj.insert("type".to_string(), json!("on-chain"));
    obj.insert("urn".to_string(), json!(item.urn));
    // amount is a NUMBER (upstream catalyst lambdas types.ts: `amount: number`);
    // the base/third-party/trimmed branches and /users all emit a number — only
    // this on-chain branch stringified it. Keep it numeric for consistency.
    obj.insert("amount".to_string(), json!(item.amount));
    obj.insert(
        "individualData".to_string(),
        Value::Array(item.individual_data.clone()),
    );
    obj.insert("name".to_string(), json!(item.name));
    obj.insert("rarity".to_string(), json!(item.rarity));
    if include_item_type {
        obj.insert(
            "itemType".to_string(),
            json!(item.item_type.clone().unwrap_or_default()),
        );
    }
    obj.insert("category".to_string(), json!(item.category));
    obj.insert("entity".to_string(), item.entity.clone());
    Value::Object(obj)
}

/// lamb2's `trimmed=true` element shape: everything the client reads lives INSIDE
/// `entity` — `id`, `thumbnail` (the content HASH of the thumbnail file, not its
/// name), `metadata` (must carry `name`; v1 wearables only store it in i18n) and
/// `individualData`. The unity backpack deserializes exactly this and silently
/// drops elements that miss it.
fn item_to_trimmed_value(item: &OwnedItem) -> Value {
    let mut entity = match &item.entity {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };

    if !entity.contains_key("id") {
        entity.insert("id".to_string(), json!(item.urn));
    }

    let thumb_file = entity
        .get("metadata")
        .and_then(|m| m.get("thumbnail"))
        .and_then(|t| t.as_str())
        .unwrap_or("thumbnail.png")
        .to_string();
    let thumb_hash = entity
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|files| {
            files.iter().find(|f| {
                f.get("file").and_then(|x| x.as_str()) == Some(thumb_file.as_str())
            })
        })
        .and_then(|f| f.get("hash").cloned());
    if let Some(h) = thumb_hash {
        entity.insert("thumbnail".to_string(), h);
    }

    if let Some(Value::Object(md)) = entity.get_mut("metadata") {
        // v1-collection metadata often carries name:"" with the display name only
        // in i18n — item.name was already resolved from there, so fill on empty too.
        let name_missing = md
            .get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.is_empty())
            .unwrap_or(true);
        if name_missing {
            md.insert("name".to_string(), json!(item.name));
        }
    }

    entity.insert(
        "individualData".to_string(),
        Value::Array(item.individual_data.clone()),
    );

    let mut obj = serde_json::Map::new();
    obj.insert("entity".to_string(), Value::Object(entity));
    obj.insert("amount".to_string(), json!(item.amount));
    Value::Object(obj)
}

async fn fetch_base_items(state: &AppState) -> Vec<OwnedItem> {
    let base = crate::handlers::base_wearables::fetch_base_wearables(state).await;
    base.into_iter()
        .map(|bw| OwnedItem {
            individual_data: vec![json!({ "id": bw.urn })],
            urn: bw.urn,
            rarity: String::new(),
            item_type: None,
            amount: 1,
            name: bw.name,
            category: bw.category,
            min_transferred_at: 0.0,
            max_transferred_at: 0.0,
            entity: bw.entity,
            is_base: true,
            is_third_party: false,
            has_date: false,
        })
        .collect()
}

async fn fetch_third_party_items(state: &AppState, owner: &str) -> Vec<OwnedItem> {
    let tpw = crate::handlers::lambdas_user_items::fetch_all_third_party_wearables(
        state, owner, None,
    )
    .await;
    tpw.into_iter()
        .map(|e| OwnedItem {
            amount: e.individual_data.len() as i64,
            individual_data: e.individual_data,
            urn: e.urn,
            rarity: String::new(),
            item_type: None,
            name: e.name,
            category: e.category,
            min_transferred_at: 0.0,
            max_transferred_at: 0.0,
            entity: e.entity,
            is_base: false,
            is_third_party: true,
            has_date: false,
        })
        .collect()
}

async fn explorer_items(
    state: &AppState,
    addr: &str,
    query: Option<&str>,
    category: &'static str,
    valid_collection_types: &[&'static str],
) -> Response {

    if !crate::query_params::is_valid_eth_address(&addr.to_lowercase()) {
        return bad_request("Address must be a valid Ethereum address");
    }

    let q = match parse_query(query, valid_collection_types) {
        Ok(q) => q,
        Err(resp) => return resp,
    };

    let addr_lc = addr.to_lowercase();
    let mut ct_sorted = q.collection_types.clone();
    ct_sorted.sort();
    let collection_types_key = ct_sorted.join(",");
    let mut cats_sorted = q.categories.clone();
    cats_sorted.sort();
    let filter_key = format!(
        "name={}|cats={}|rarity={}|trimmed={}",
        q.name.as_deref().unwrap_or(""),
        cats_sorted.join(","),
        q.rarity.as_deref().unwrap_or(""),
        q.trimmed,
    );
    // The cache holds the FULL computed item set (page params excluded from the
    // key, -1 sentinels keep the key type); pages are sliced per request. The
    // previous per-page keys recomputed the whole inventory for every page of a
    // backpack scroll (~600ms x pages).
    let cache_key: ExplorerKey = (
        addr_lc.clone(),
        q.sort.clone(),
        q.direction.clone(),
        -1,
        -1,
        collection_types_key,
        category,
        filter_key,
    );

    let page_num = q.page_num;
    let page_size = q.page_size;

    let valid_collection_types_owned: Vec<&'static str> = valid_collection_types.to_vec();
    let result: Result<Value, ()> = explorer_cache()
        .get_or_fetch(cache_key, move || async move {
            compute_explorer_items(state, &addr_lc, q, category, &valid_collection_types_owned)
                .await
        })
        .await;

    match result {
        Ok(v) => {
            let total = v.get("totalAmount").cloned().unwrap_or_else(|| json!(0));
            let offset = page_num.saturating_sub(1).saturating_mul(page_size).max(0) as usize;
            let limit = page_size.max(0) as usize;
            let page: Vec<Value> = v
                .get("elements")
                .and_then(|e| e.as_array())
                .map(|all| all.iter().skip(offset).take(limit).cloned().collect())
                .unwrap_or_default();
            Json(json!({
                "elements": page,
                "totalAmount": total,
                "pageNum": page_num,
                "pageSize": page_size,
            }))
            .into_response()
        }
        Err(()) => Json(json!({
            "elements": [],
            "totalAmount": 0,
            "pageNum": 1,
            "pageSize": 0,
        }))
        .into_response(),
    }
}

async fn compute_explorer_items(
    state: &AppState,
    addr: &str,
    q: ExplorerQuery,
    category: &'static str,
    valid_collection_types: &[&'static str],
) -> Result<Value, ()> {
    let base_allowed = valid_collection_types.contains(&"base-wearable")
        && (q.collection_types.is_empty()
            || q.collection_types.iter().any(|t| t == "base-wearable"));
    let on_chain_allowed = q.collection_types.is_empty()
        || q.collection_types.iter().any(|t| t == "on-chain");

    let third_party_allowed = valid_collection_types.contains(&"third-party")
        && (q.collection_types.is_empty()
            || q.collection_types.iter().any(|t| t == "third-party"));

    let mut items: Vec<OwnedItem> = Vec::new();
    if base_allowed {
        items.extend(fetch_base_items(state).await);
    }
    if on_chain_allowed {
        items.extend(fetch_owned_items(state, addr, category).await);
    }
    if third_party_allowed {
        items.extend(fetch_third_party_items(state, addr).await);
    }

    items.retain(|it| passes_filters(it, &q));
    sort_items(&mut items, &q.sort, &q.direction);

    let total = items.len() as i64;

    // Materialize the FULL set — the caller caches it and slices pages.
    let include_item_type = category == "wearable";
    let elements: Vec<Value> = items
        .iter()
        .map(|it| {
            if q.trimmed {
                item_to_trimmed_value(it)
            } else {
                item_to_value(it, include_item_type)
            }
        })
        .collect();

    Ok(json!({
        "elements": elements,
        "totalAmount": total,
    }))
}

pub async fn explorer_wearables(
    State(state): State<Arc<AppState>>,
    Path(addr): Path<String>,
    req: Request,
) -> Response {

    let query = req.uri().query().map(|q| q.to_string());

    explorer_items(
        &state,
        &addr,
        query.as_deref(),
        "wearable",
        &VALID_COLLECTION_TYPES,
    )
    .await
}

pub async fn explorer_emotes(
    State(state): State<Arc<AppState>>,
    Path(addr): Path<String>,
    req: Request,
) -> Response {
    let query = req.uri().query().map(|q| q.to_string());

    explorer_items(&state, &addr, query.as_deref(), "emote", &["on-chain"]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(urn: &str, name: &str, rarity: &str, has_date: bool, max_at: f64) -> OwnedItem {
        OwnedItem {
            urn: urn.to_string(),
            individual_data: Vec::new(),
            rarity: rarity.to_string(),
            item_type: None,
            amount: 1,
            name: name.to_string(),
            category: String::new(),
            min_transferred_at: max_at,
            max_transferred_at: max_at,
            entity: Value::Null,
            is_base: !has_date,
            is_third_party: false,
            has_date,
        }
    }

    fn urns(items: &[OwnedItem]) -> Vec<&str> {
        items.iter().map(|i| i.urn.as_str()).collect()
    }

    #[test]
    fn locale_cmp_places_basefemale_between_balbo_and_basketball() {
        use std::cmp::Ordering;
        let mut v = vec!["basketball_shorts", "BaseFemale", "balbo_beard"];
        v.sort_by(|a, b| locale_cmp(a, b));
        assert_eq!(v, vec!["balbo_beard", "BaseFemale", "basketball_shorts"]);

        assert_eq!("BaseFemale".cmp("balbo_beard"), Ordering::Less);
    }

    #[test]
    fn locale_cmp_case_tiebreak_lowercase_first() {
        use std::cmp::Ordering;

        assert_eq!(locale_cmp("abc", "ABC"), Ordering::Less);
        assert_eq!(locale_cmp("ABC", "abc"), Ordering::Greater);
        assert_eq!(locale_cmp("abc", "abc"), Ordering::Equal);
    }

    #[test]
    fn sort_rarity_rarest_optional_has_before_hasnot() {
        let mut items = vec![
            owned("urn:base_b", "B", "", false, 0.0),
            owned("urn:onchain_a", "A", "common", true, 0.0),
            owned("urn:onchain_c", "C", "legendary", true, 0.0),
            owned("urn:base_a", "A", "", false, 0.0),
        ];
        sort_items(&mut items, "rarity", "DESC");

        assert_eq!(
            urns(&items),
            vec!["urn:onchain_c", "urn:onchain_a", "urn:base_a", "urn:base_b"]
        );
    }

    #[test]
    fn sort_rarity_asc_reverses() {
        let mut items = vec![
            owned("urn:onchain_a", "A", "common", true, 0.0),
            owned("urn:base_a", "A", "", false, 0.0),
            owned("urn:onchain_c", "C", "legendary", true, 0.0),
        ];
        sort_items(&mut items, "rarity", "ASC");

        assert_eq!(urns(&items), vec!["urn:base_a", "urn:onchain_a", "urn:onchain_c"]);
    }

    #[test]
    fn sort_date_newest_optional() {
        let mut items = vec![
            owned("urn:base", "Base", "", false, 0.0),
            owned("urn:old", "Old", "common", true, 5.0),
            owned("urn:new", "New", "common", true, 9.0),
        ];
        sort_items(&mut items, "date", "DESC");
        assert_eq!(urns(&items), vec!["urn:new", "urn:old", "urn:base"]);
    }

    use crate::cache::ResponseCache;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    use std::time::Duration as StdDuration;

    #[tokio::test]
    async fn explorer_cache_distinct_filters_dont_collide_and_same_key_hits() {
        let cache: ResponseCache<ExplorerKey, Value> =
            ResponseCache::new("explorer_test", StdDuration::from_secs(60), 100);
        let counter = StdArc::new(AtomicUsize::new(0));

        let make_key = |addr: &str, filter: &str| -> ExplorerKey {
            (
                addr.to_string(),
                "rarity".to_string(),
                "DESC".to_string(),
                100,
                1,
                "".to_string(),
                "wearable",
                filter.to_string(),
            )
        };

        let c = counter.clone();
        cache
            .get_or_fetch(make_key("0xabc", "name=hat"), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!({ "elements": ["hat"] }))
            })
            .await
            .unwrap();

        let c = counter.clone();
        cache
            .get_or_fetch(make_key("0xabc", "name=hat"), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!({ "elements": [] }))
            })
            .await
            .unwrap();

        let c = counter.clone();
        cache
            .get_or_fetch(make_key("0xabc", "name=cape"), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!({ "elements": ["cape"] }))
            })
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
