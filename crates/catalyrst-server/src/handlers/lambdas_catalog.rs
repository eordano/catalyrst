use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Request, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::errors::{bad_request, not_found};
use crate::handlers::base_wearables::{self, BaseWearable, BASE_AVATARS_COLLECTION_ID};
use crate::handlers::definitions::{
    default_sort_direction, extract_emote_definition, extract_wearable_definition, validate_rarity,
    validate_sort, SORTED_RARITIES,
};
use crate::query_params::{parse_query_string, qs_get_array, qs_get_string};
use crate::state::AppState;
use catalyrst_commons::cache::TtlMap;

type Cache = Arc<TtlMap<String, Value>>;

fn static_cache(
    cell: &'static OnceLock<Cache>,
    name: &'static str,
    ttl_secs: u64,
    max_entries: usize,
) -> &'static Cache {
    cell.get_or_init(|| {
        Arc::new(TtlMap::bounded(
            name,
            Duration::from_secs(ttl_secs),
            max_entries,
        ))
    })
}

fn collections_cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    static_cache(&C, "nfts_collections", 300, 8)
}

pub(crate) fn outfits_cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    static_cache(&C, "outfits", 60, 50_000)
}

/// A deployment can create or replace an outfits entity, so a client that saves and re-reads within
/// the TTL must not get the stale list (or a cached not-found). Deployment success paths call this
/// alongside their `deployments_cache.clear()`.
pub(crate) fn invalidate_outfits_cache() {
    outfits_cache().clear();
}

fn catalog_params_from_query(qs: &str) -> CatalogParams {
    let p = parse_query_string(qs);
    CatalogParams {
        collection_id: qs_get_array(&p, "collectionId"),
        collection_type: qs_get_array(&p, "collectionType"),
        category: qs_get_array(&p, "category"),
        rarity: qs_get_string(&p, "rarity"),
        name: qs_get_string(&p, "name"),
        order_by: qs_get_string(&p, "orderBy"),
        direction: qs_get_string(&p, "direction"),
        wearable_id: qs_get_array(&p, "wearableId"),
        emote_id: qs_get_array(&p, "emoteId"),
        text_search: qs_get_string(&p, "textSearch"),
        last_id: qs_get_string(&p, "lastId"),
        limit: qs_get_string(&p, "limit"),
    }
}

/// DELIBERATE DIVERGENCE from stock catalyst / lamb2, added 2026-08-26.
///
/// Upstream's global catalog takes no `collectionType`: lamb2's `parseCatalogQuery` reads only
/// collectionId/itemIds/textSearch, so `?collectionType=` alone answers the "you must use one of the
/// filters" 400 and, alongside a real filter, is silently dropped. `collectionType` is upstream's
/// parameter for `/lambdas/explorer/{address}/wearables`, not for this one.
///
/// The invariant that keeps the divergence honest: with no `collectionType` in the query the response
/// is byte-identical to upstream -- no new key in `filters`, no change to `next`, no change to the
/// 400. Every behaviour below is reachable only when a caller opts in by naming at least one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionType {
    BaseWearable,
    OnChain,
    ThirdParty,
}

impl CollectionType {
    fn parse(raw: &str) -> Option<Self> {
        match raw.to_lowercase().as_str() {
            "base-wearable" => Some(Self::BaseWearable),
            "on-chain" => Some(Self::OnChain),
            "third-party" => Some(Self::ThirdParty),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::BaseWearable => "base-wearable",
            Self::OnChain => "on-chain",
            Self::ThirdParty => "third-party",
        }
    }
}

/// Preserves caller order and drops repeats. An unrecognised value is refused rather than ignored: a
/// silently-dropped typo would widen the result set to everything.
fn parse_collection_types(raw: &[String]) -> Result<Vec<CollectionType>, Response> {
    let mut out = Vec::new();
    for value in raw {
        let t = CollectionType::parse(value).ok_or_else(|| {
            bad_request(&format!(
                "Invalid collectionType '{value}'. Valid values are: 'base-wearable', 'on-chain', 'third-party'"
            ))
        })?;
        if !out.contains(&t) {
            out.push(t);
        }
    }
    Ok(out)
}

fn qp(key: &str, value: &str) -> String {
    format!("{key}={}", urlencoding::encode(value))
}

fn lower_nonempty(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty())
}

/// All opt-in. Shares its vocabulary and error wording with
/// `/lambdas/explorer/{address}/wearables` via `handlers::definitions`, so a caller who learned the
/// per-owner endpoint does not have to learn a second dialect here.
#[derive(Debug, Default, Clone)]
struct ExtendedFilters {
    collection_types: Vec<CollectionType>,
    categories: Vec<String>,
    rarity: Option<String>,
    name: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
}

impl ExtendedFilters {
    /// True once the caller has expressed an intent, so the upstream "you must use one of the
    /// filters" 400 no longer applies. `sort` alone does not count: ordering everything is not a
    /// filter.
    fn narrows(&self) -> bool {
        !self.collection_types.is_empty()
            || !self.categories.is_empty()
            || self.rarity.is_some()
            || self.name.is_some()
    }

    fn allows(&self, t: CollectionType) -> bool {
        self.collection_types.is_empty() || self.collection_types.contains(&t)
    }

    fn query_parts(&self) -> Vec<String> {
        let mut parts: Vec<String> = self
            .collection_types
            .iter()
            .map(|t| format!("collectionType={}", t.as_str()))
            .collect();
        parts.extend(self.categories.iter().map(|c| qp("category", c)));
        parts.extend(self.rarity.iter().map(|r| qp("rarity", r)));
        parts.extend(self.name.iter().map(|n| qp("name", n)));
        if let Some(s) = &self.sort {
            parts.push(qp("orderBy", s));
            parts.extend(self.direction.iter().map(|d| format!("direction={d}")));
        }
        parts
    }

    fn write_json(&self, m: &mut Map<String, Value>) {
        if !self.collection_types.is_empty() {
            let v: Vec<&str> = self.collection_types.iter().map(|t| t.as_str()).collect();
            m.insert("collectionTypes".into(), json!(v));
        }
        if !self.categories.is_empty() {
            m.insert("categories".into(), json!(self.categories));
        }
        if let Some(r) = &self.rarity {
            m.insert("rarity".into(), json!(r));
        }
        if let Some(n) = &self.name {
            m.insert("name".into(), json!(n));
        }
        if let Some(s) = &self.sort {
            m.insert("orderBy".into(), json!(s));
            if let Some(d) = &self.direction {
                m.insert("direction".into(), json!(d));
            }
        }
    }
}

/// Enforces the one rule the tiering makes necessary: `orderBy` needs the query to resolve to a
/// single tier. Base wearables and on-chain items come from different stores (an in-memory entity
/// list and the squid index) behind a two-phase, urn-keyed cursor that serves every base wearable
/// before the first on-chain item, so a global `orderBy=rarity` across both would either buffer the
/// whole on-chain set to merge it or return pages whose order contradicts the parameter.
fn parse_extended_filters(p: &CatalogParams) -> Result<ExtendedFilters, Response> {
    let collection_types = parse_collection_types(&p.collection_type)?;

    let categories: Vec<String> = p
        .category
        .iter()
        .map(|c| c.to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();

    let rarity = lower_nonempty(&p.rarity);
    if let Some(r) = &rarity {
        validate_rarity(r).map_err(|e| bad_request(&e))?;
    }

    let sort = lower_nonempty(&p.order_by);
    let direction = match &sort {
        None => None,
        Some(s) => {
            let d = match p.direction.as_deref() {
                Some(d) if !d.is_empty() => d.to_uppercase(),
                _ => default_sort_direction(s).to_string(),
            };
            validate_sort(s, &d).map_err(|e| bad_request(&e))?;
            Some(d)
        }
    };

    if sort.is_some() && collection_types.len() != 1 {
        return Err(bad_request(
            "Sorting the global catalog needs a single collectionType: base wearables and on-chain items are paged from different stores, so 'orderBy' is only well defined within one of them. Add exactly one 'collectionType' (for example 'collectionType=on-chain').",
        ));
    }

    Ok(ExtendedFilters {
        collection_types,
        categories,
        rarity,
        name: lower_nonempty(&p.name),
        sort,
        direction,
    })
}

const MAX_LIMIT: i64 = 500;
const BASE_EMOTES_COLLECTION_ID: &str = "urn:decentraland:off-chain:base-emotes";

fn cursor_to_squid(last_id: &str) -> String {
    last_id.replace(":ethereum:", ":mainnet:")
}

#[derive(Debug, Deserialize, Default)]
pub struct CatalogParams {
    #[serde(default, rename = "collectionId")]
    collection_id: Vec<String>,
    #[serde(default, rename = "collectionType")]
    collection_type: Vec<String>,
    #[serde(default)]
    category: Vec<String>,
    rarity: Option<String>,
    name: Option<String>,
    #[serde(rename = "orderBy")]
    order_by: Option<String>,
    direction: Option<String>,
    #[serde(default, rename = "wearableId")]
    wearable_id: Vec<String>,
    #[serde(default, rename = "emoteId")]
    emote_id: Vec<String>,
    #[serde(rename = "textSearch")]
    text_search: Option<String>,
    #[serde(rename = "lastId")]
    last_id: Option<String>,
    limit: Option<String>,
}

struct CatalogFilters {
    collection_ids: Option<Vec<String>>,
    item_ids: Option<Vec<String>>,
    text_search: Option<String>,
    /// Empty for an upstream-shaped query, which keeps the echoed `filters` object byte-identical to
    /// stock catalyst until a caller opts in.
    extended: ExtendedFilters,
}

impl CatalogFilters {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(c) = &self.collection_ids {
            m.insert("collectionIds".into(), json!(c));
        }
        if let Some(i) = &self.item_ids {
            m.insert("itemIds".into(), json!(i));
        }
        if let Some(t) = &self.text_search {
            m.insert("textSearch".into(), json!(t));
        }
        self.extended.write_json(&mut m);
        Value::Object(m)
    }
}

struct CatalogQuery {
    filters: CatalogFilters,
    limit: i64,
    last_id: Option<String>,
}

fn clamp_limit(raw: &Option<String>) -> i64 {
    catalyrst_types::limit_or_max(raw.as_ref().and_then(|s| s.parse().ok()), MAX_LIMIT)
}

fn non_empty(v: Vec<String>) -> Option<Vec<String>> {
    (!v.is_empty()).then_some(v)
}

/// `extended` is `Default` for callers that do not offer the extension (the emotes catalog), which
/// keeps their behaviour byte-identical to upstream.
fn parse_catalog_query(
    p: &CatalogParams,
    item_ids_in: &[String],
    id_param_name: &str,
    extended: ExtendedFilters,
) -> Result<CatalogQuery, Response> {
    let collection_ids: Vec<String> = p.collection_id.iter().map(|s| s.to_lowercase()).collect();
    let item_ids: Vec<String> = item_ids_in.iter().map(|s| s.to_lowercase()).collect();
    let text_search = lower_nonempty(&p.text_search);

    if collection_ids.is_empty()
        && item_ids.is_empty()
        && text_search.is_none()
        && !extended.narrows()
    {
        return Err(bad_request(&format!(
            "You must use one of the filters: 'textSearch', 'collectionId' or '{id_param_name}'"
        )));
    }
    if text_search.as_ref().is_some_and(|t| t.chars().count() < 3) {
        return Err(bad_request(
            "The text search must be at least 3 characters long",
        ));
    }
    let items_label = if id_param_name == "wearableId" {
        "wearables"
    } else {
        "emotes"
    };
    if item_ids.len() as i64 > MAX_LIMIT {
        return Err(bad_request(&format!(
            "You can't ask for more than {MAX_LIMIT} {items_label}"
        )));
    }
    if collection_ids.len() as i64 > MAX_LIMIT {
        return Err(bad_request(&format!(
            "You can't filter for more than {MAX_LIMIT} collection ids"
        )));
    }

    Ok(CatalogQuery {
        filters: CatalogFilters {
            collection_ids: non_empty(collection_ids),
            item_ids: non_empty(item_ids),
            text_search,
            extended,
        },
        limit: clamp_limit(&p.limit),
        last_id: lower_nonempty(&p.last_id),
    })
}

fn build_next_query(
    filters: &CatalogFilters,
    limit: i64,
    next_last_id: &str,
    id_param_name: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(
        filters
            .collection_ids
            .iter()
            .flatten()
            .map(|id| qp("collectionId", id)),
    );
    parts.extend(
        filters
            .item_ids
            .iter()
            .flatten()
            .map(|id| qp(id_param_name, id)),
    );
    parts.extend(filters.text_search.iter().map(|t| qp("textSearch", t)));
    parts.extend(filters.extended.query_parts());
    parts.push(format!("limit={limit}"));
    parts.push(qp("lastId", next_last_id));
    parts.join("&")
}

enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

struct SqlBuilder {
    sql: String,
    binds: Vec<Bind>,
}

impl SqlBuilder {
    /// `clause` receives the 1-based placeholder index that `bind` will occupy.
    fn add(&mut self, clause: impl FnOnce(usize) -> String, bind: Bind) {
        self.sql.push_str(&clause(self.binds.len() + 1));
        self.binds.push(bind);
    }
}

async fn fetch_item_urns(
    pool: &PgPool,
    item_type_prefix: &str,
    filters: &CatalogFilters,
    limit: i64,
    last_id: &Option<String>,
) -> Vec<String> {
    let wearable = item_type_prefix == "wearable";
    let item_type_clause = if wearable {
        "(item_type LIKE 'wearable%' OR item_type LIKE 'smart_wearable%')"
    } else {
        "item_type LIKE 'emote%'"
    };
    let rarity_column = if wearable {
        "search_wearable_rarity"
    } else {
        "search_emote_rarity"
    };
    let mut q = SqlBuilder {
        sql: format!(
            "SELECT urn FROM squid_marketplace.item \
             WHERE urn IS NOT NULL AND {item_type_clause}"
        ),
        binds: Vec::new(),
    };

    if let Some(cids) = &filters.collection_ids {
        let clauses: Vec<String> = cids
            .iter()
            .map(|c| {
                q.binds.push(Bind::Text(format!(
                    "{}:%",
                    cursor_to_squid(&c.to_lowercase())
                )));
                format!("lower(urn) LIKE ${}", q.binds.len())
            })
            .collect();
        q.sql.push_str(&format!(" AND ({})", clauses.join(" OR ")));
    }

    if let Some(iids) = &filters.item_ids {
        q.add(
            |idx| format!(" AND lower(urn) = ANY(${idx})"),
            Bind::TextArray(
                iids.iter()
                    .map(|s| cursor_to_squid(&s.to_lowercase()))
                    .collect(),
            ),
        );
    }

    if let Some(t) = &filters.text_search {
        q.add(
            |idx| format!(" AND search_text ILIKE ${idx}"),
            Bind::Text(format!("%{t}%")),
        );
    }

    let ext = &filters.extended;

    if !ext.categories.is_empty() {
        let column = if wearable {
            "search_wearable_category"
        } else {
            "search_emote_category"
        };
        q.add(
            |idx| format!(" AND lower({column}::text) = ANY(${idx})"),
            Bind::TextArray(ext.categories.clone()),
        );
    }

    if let Some(r) = &ext.rarity {
        q.add(
            |idx| format!(" AND lower({rarity_column}) = ${idx}"),
            Bind::Text(r.clone()),
        );
    }

    if let Some(n) = &ext.name {
        q.add(
            |idx| format!(" AND search_text ILIKE ${idx}"),
            Bind::Text(format!("%{n}%")),
        );
    }

    if let Some(cursor) = last_id {
        q.add(
            |idx| format!(" AND lower(urn) > ${idx}"),
            Bind::Text(cursor_to_squid(&cursor.to_lowercase())),
        );
    }

    let order = match (ext.sort.as_deref(), ext.direction.as_deref()) {
        (Some("rarity"), Some(dir)) => {
            let ranks: Vec<String> = SORTED_RARITIES
                .iter()
                .enumerate()
                .map(|(i, r)| format!("WHEN '{r}' THEN {i}"))
                .collect();
            format!(
                "CASE lower({rarity_column}) {} ELSE -1 END {dir}, urn ASC",
                ranks.join(" ")
            )
        }
        (Some("date"), Some(dir)) => format!("created_at {dir}, urn ASC"),
        (Some("name"), Some(dir)) => format!("lower(search_text) {dir}, urn ASC"),
        _ => "urn ASC".to_string(),
    };
    q.add(
        |idx| format!(" ORDER BY {order} LIMIT ${idx}"),
        Bind::Int(limit + 1),
    );

    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(q.sql));
    for b in q.binds {
        query = match b {
            Bind::Text(s) => query.bind(s),
            Bind::TextArray(a) => query.bind(a),
            Bind::Int(n) => query.bind(n),
        };
    }

    query.fetch_all(pool).await.unwrap_or_default()
}

fn id_key(v: &Value) -> String {
    v["id"].as_str().unwrap_or("").to_lowercase()
}

fn paginate(definitions: Vec<Value>, limit: i64) -> (Vec<Value>, Option<String>) {
    paginate_merged(Vec::new(), definitions, limit)
}

/// `pre_merge` is served first and never reordered; `on_chain` is sorted by id behind it.
fn paginate_merged(
    mut merged: Vec<Value>,
    mut on_chain: Vec<Value>,
    limit: i64,
) -> (Vec<Value>, Option<String>) {
    on_chain.sort_by_key(id_key);
    merged.extend(on_chain);
    let has_more = merged.len() as i64 > limit;
    if has_more {
        merged.truncate(limit as usize);
    }
    let next = if has_more {
        merged
            .last()
            .and_then(|d| d["id"].as_str())
            .map(String::from)
    } else {
        None
    };
    (merged, next)
}

fn filter_and_extract_base_wearables(
    base: &[BaseWearable],
    filters: &CatalogFilters,
    last_id: &Option<String>,
    max_results: usize,
    content_public_url: &str,
) -> Vec<Value> {
    let ext = &filters.extended;
    let mut matched: Vec<&BaseWearable> = base
        .iter()
        .filter(|w| {
            let lc_urn = w.urn.to_lowercase();
            let haystack = w.english_name.as_deref().unwrap_or(&w.name).to_lowercase();
            last_id.as_ref().is_none_or(|lid| lc_urn > *lid)
                && filters
                    .item_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&lc_urn))
                && filters
                    .text_search
                    .as_ref()
                    .is_none_or(|t| haystack.contains(t.as_str()))
                && (ext.categories.is_empty()
                    || ext.categories.contains(&w.category.to_lowercase()))
                && ext.rarity.is_none()
                && ext
                    .name
                    .as_ref()
                    .is_none_or(|n| haystack.contains(n.as_str()))
        })
        .collect();
    matched.sort_by_key(|a| a.urn.to_lowercase());
    matched.truncate(max_results);
    matched
        .into_iter()
        .filter_map(|w| extract_wearable_definition(&w.entity, content_public_url))
        .collect()
}

async fn definitions_for(
    state: &AppState,
    pointers: Vec<String>,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Vec<Value> {
    if pointers.is_empty() {
        return Vec::new();
    }
    let entities = state
        .database
        .active_entities_by_pointers(&pointers)
        .await
        .unwrap_or_default();
    entities
        .iter()
        .filter_map(|e| extract(e, &state.content_public_url))
        .collect()
}

fn catalog_response(
    items_key: &str,
    items: Vec<Value>,
    filters: &CatalogFilters,
    limit: i64,
    next_last_id: Option<String>,
    id_param_name: &str,
) -> Response {
    let mut pagination = Map::new();
    pagination.insert("limit".into(), json!(limit));
    if let Some(nl) = next_last_id {
        let next = format!("?{}", build_next_query(filters, limit, &nl, id_param_name));
        pagination.insert("next".into(), json!(next));
    }
    Json(json!({
        items_key: items,
        "filters": filters.to_json(),
        "pagination": Value::Object(pagination),
    }))
    .into_response()
}

async fn catalog_wearables_with_base(state: &AppState, query: CatalogQuery) -> Response {
    let filters = &query.filters;
    let limit = query.limit;

    let only_base_collection = matches!(
        &filters.collection_ids,
        Some(c) if c.len() == 1 && c[0] == BASE_AVATARS_COLLECTION_ID
    );
    let base_collection_allowed = filters
        .collection_ids
        .as_ref()
        .is_none_or(|c| c.iter().any(|id| id == BASE_AVATARS_COLLECTION_ID))
        && filters.extended.allows(CollectionType::BaseWearable);
    let cursor_in_base_range = query
        .last_id
        .as_ref()
        .is_none_or(|l| l.starts_with(BASE_AVATARS_COLLECTION_ID));

    let (off_chain, on_chain_cursor) = if base_collection_allowed && cursor_in_base_range {
        let base = base_wearables::fetch_base_wearables(state).await;
        let off_chain = filter_and_extract_base_wearables(
            &base,
            filters,
            &query.last_id,
            (limit + 1) as usize,
            &state.content_public_url,
        );
        (off_chain, None)
    } else {
        (Vec::new(), query.last_id.clone())
    };

    let remaining = limit - off_chain.len() as i64;
    let mut on_chain_defs = Vec::new();
    if !only_base_collection && filters.extended.allows(CollectionType::OnChain) && remaining >= 0 {
        if let Some(pool) = state.squid_pool.as_ref() {
            let urns =
                fetch_item_urns(pool, "wearable", filters, remaining + 1, &on_chain_cursor).await;
            let pointers = urns
                .iter()
                .map(|u| u.replacen(":mainnet:", ":ethereum:", 1).to_lowercase())
                .collect();
            on_chain_defs = definitions_for(state, pointers, extract_wearable_definition).await;
        }
    }

    let (items, next_last_id) = paginate_merged(off_chain, on_chain_defs, limit);
    catalog_response(
        "wearables",
        items,
        filters,
        limit,
        next_last_id,
        "wearableId",
    )
}

async fn catalog(
    state: &AppState,
    query: CatalogQuery,
    item_type_prefix: &str,
    id_param_name: &str,
    items_key: &str,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Response {
    let Some(pool) = state.squid_pool.as_ref() else {
        return catalog_response(
            items_key,
            Vec::new(),
            &query.filters,
            query.limit,
            None,
            id_param_name,
        );
    };
    let urns = fetch_item_urns(
        pool,
        item_type_prefix,
        &query.filters,
        query.limit,
        &query.last_id,
    )
    .await;
    let pointers = urns.iter().map(|u| u.to_lowercase()).collect();
    let definitions = definitions_for(state, pointers, extract).await;
    let (items, next_last_id) = paginate(definitions, query.limit);
    catalog_response(
        items_key,
        items,
        &query.filters,
        query.limit,
        next_last_id,
        id_param_name,
    )
}

fn wearables_query(qs: &str) -> Result<CatalogQuery, Response> {
    let p = catalog_params_from_query(qs);
    let extended = parse_extended_filters(&p)?;
    parse_catalog_query(&p, &p.wearable_id, "wearableId", extended)
}

pub async fn collections_wearables_catalog(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    match wearables_query(request.uri().query().unwrap_or("")) {
        Ok(query) => catalog_wearables_with_base(&state, query).await,
        Err(resp) => resp,
    }
}

pub async fn collections_emotes_catalog(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let p = catalog_params_from_query(request.uri().query().unwrap_or(""));
    match parse_catalog_query(&p, &p.emote_id, "emoteId", ExtendedFilters::default()) {
        Ok(query) => {
            catalog(
                &state,
                query,
                "emote",
                "emoteId",
                "emotes",
                extract_emote_definition,
            )
            .await
        }
        Err(resp) => resp,
    }
}

fn base_collections() -> Vec<Value> {
    vec![
        json!({ "id": BASE_AVATARS_COLLECTION_ID, "name": "Base Wearables" }),
        json!({ "id": BASE_EMOTES_COLLECTION_ID, "name": "Base Emotes" }),
    ]
}

fn chain_ok<T, E>(a: Result<Vec<T>, E>, b: Result<Vec<T>, E>) -> Vec<T> {
    a.unwrap_or_default()
        .into_iter()
        .chain(b.unwrap_or_default())
        .collect()
}

pub async fn nfts_collections(State(state): State<Arc<AppState>>) -> Response {
    let network = state.eth_network.clone();
    let pool = state.squid_pool.clone();
    let cached = collections_cache()
        .get_or_fetch(network.clone(), move || async move {
            use crate::handlers::external_graph;

            let mut collections = base_collections();

            let local = match pool.as_ref() {
                Some(pool) => {
                    let (eth, poly) = tokio::join!(
                        external_graph::collections_from_squid(
                            pool,
                            "ETHEREUM",
                            Some((":mainnet:", ":ethereum:")),
                        ),
                        external_graph::collections_from_squid(pool, "POLYGON", None),
                    );
                    chain_ok(eth, poly)
                }
                None => Vec::new(),
            };

            let items = if !local.is_empty() {
                local
            } else {
                match external_graph::subgraph_urls(&network) {
                    Ok(urls) => {
                        let (l1, l2) = tokio::join!(
                            external_graph::collections(&urls.eth_collections),
                            external_graph::collections(&urls.matic_collections),
                        );
                        chain_ok(l1, l2)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "collection catalog upstream unavailable");
                        Vec::new()
                    }
                }
            };
            collections.extend(
                items
                    .into_iter()
                    .map(|(urn, name)| json!({ "id": urn, "name": name })),
            );

            Ok::<Value, ()>(json!({ "collections": collections }))
        })
        .await;

    match cached {
        Ok(v) => Json(v),
        Err(_) => Json(json!({ "collections": base_collections() })),
    }
    .into_response()
}

pub async fn outfits(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let address = id.to_lowercase();
    let key = address.clone();
    let cached = outfits_cache()
        .get_or_fetch(key, move || async move {
            let pointer = format!("{address}:outfits");
            let mut entity = match state.database.find_entity_by_pointer(&pointer).await {
                Ok(Some(e)) => e,
                Ok(None) | Err(_) => return Ok::<Value, ()>(Value::Null),
            };

            let owned_names = match state.squid_pool.as_ref() {
                Some(pool) => {
                    super::profile_processing::fetch_owned_ens_names(pool, &address).await
                }
                None => Vec::new(),
            };

            if let Some(metadata) = entity.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                if owned_names.is_empty() {
                    if let Some(outfits) =
                        metadata.get_mut("outfits").and_then(|o| o.as_array_mut())
                    {
                        outfits.retain(|o| {
                            o.get("slot")
                                .and_then(|s| s.as_i64())
                                .is_none_or(|s| s <= 4)
                        });
                    }
                }
                metadata.insert("namesForExtraSlots".into(), json!(owned_names));
            }
            Ok(entity)
        })
        .await;

    match cached {
        Ok(v) if !v.is_null() => Json(v).into_response(),
        _ => not_found("Outfits not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sample_wearable_entity() -> Value {
        json!({
            "id": "QmEntity",
            "type": "wearable",
            "pointers": ["urn:decentraland:matic:collections-v2:0xabc:0"],
            "timestamp": 1644605585899i64,
            "content": [
                { "file": "male/x.glb", "hash": "QmGlb" },
                { "file": "image.png", "hash": "QmImg" },
                { "file": "thumbnail.png", "hash": "QmThumb" }
            ],
            "metadata": {
                "id": "urn:decentraland:matic:collections-v2:0xabc:0",
                "name": "Thing",
                "rarity": "legendary",
                "data": {
                    "category": "upper_body",
                    "representations": [
                        { "bodyShapes": ["BaseMale"], "mainFile": "male/x.glb", "contents": ["male/x.glb"] }
                    ]
                },
                "image": "image.png",
                "thumbnail": "thumbnail.png"
            }
        })
    }

    #[test]
    fn extract_wearable_rewrites_contents_and_images() {
        let e = sample_wearable_entity();
        let def = extract_wearable_definition(&e, "https://c.example/content/").unwrap();
        assert_eq!(def["image"], "https://c.example/content/contents/QmImg");
        assert_eq!(
            def["thumbnail"],
            "https://c.example/content/contents/QmThumb"
        );
        let contents = &def["data"]["representations"][0]["contents"];
        assert_eq!(contents[0]["key"], "male/x.glb");
        assert_eq!(
            contents[0]["url"],
            "https://c.example/content/contents/QmGlb"
        );
    }

    #[test]
    fn content_url_appends_slash_when_missing() {
        use crate::handlers::definitions::content_url;
        let e = sample_wearable_entity();
        let url = content_url(&e, "image.png", "https://c.example/content").unwrap();
        assert_eq!(url, "https://c.example/content/contents/QmImg");
    }

    #[test]
    fn cursor_ethereum_to_mainnet_roundtrip() {
        for (input, want) in [
            (
                "urn:decentraland:ethereum:collections-v1:0xabc:0",
                "urn:decentraland:mainnet:collections-v1:0xabc:0",
            ),
            (
                "urn:decentraland:matic:collections-v2:0xabc:0",
                "urn:decentraland:matic:collections-v2:0xabc:0",
            ),
            (
                "urn:decentraland:off-chain:base-avatars:eyes_00",
                "urn:decentraland:off-chain:base-avatars:eyes_00",
            ),
            (":ethereum::ethereum:", ":mainnet::mainnet:"),
        ] {
            assert_eq!(cursor_to_squid(input), want);
        }
    }

    #[test]
    fn clamp_limit_defaults_and_caps() {
        assert_eq!(clamp_limit(&None), MAX_LIMIT);
        for raw in ["0", "9999", "abc"] {
            assert_eq!(clamp_limit(&Some(raw.into())), MAX_LIMIT);
        }
        assert_eq!(clamp_limit(&Some("10".into())), 10);
    }

    fn parse_plain(p: &CatalogParams) -> Result<CatalogQuery, Response> {
        parse_catalog_query(p, &[], "wearableId", ExtendedFilters::default())
    }

    #[test]
    fn parse_requires_a_filter() {
        assert!(parse_plain(&CatalogParams::default()).is_err());
    }

    #[test]
    fn an_upstream_shaped_query_is_untouched_by_the_extension() {
        let q = wearables_query("collectionId=urn:c1").unwrap();
        assert_eq!(
            q.filters.to_json(),
            json!({ "collectionIds": ["urn:c1"] }),
            "filters echo gained a key without the caller opting in"
        );
        assert_eq!(
            build_next_query(&q.filters, 50, "urn:c1:5", "wearableId"),
            "collectionId=urn%3Ac1&limit=50&lastId=urn%3Ac1%3A5",
            "next query gained a parameter without the caller opting in"
        );
    }

    #[test]
    fn collection_type_alone_satisfies_the_filter_requirement() {
        let q = wearables_query("collectionType=on-chain").unwrap();
        assert_eq!(
            q.filters.extended.collection_types,
            vec![CollectionType::OnChain]
        );
        assert!(!q.filters.extended.allows(CollectionType::BaseWearable));
        assert!(q.filters.extended.allows(CollectionType::OnChain));
    }

    #[test]
    fn each_extended_filter_alone_satisfies_the_filter_requirement() {
        for qs in ["category=eyewear", "rarity=mythic", "name=aviator"] {
            assert!(wearables_query(qs).is_ok(), "{qs} should be a filter");
        }
    }

    #[test]
    fn ordering_alone_is_not_a_filter() {
        assert!(wearables_query("orderBy=rarity").is_err());
    }

    #[test]
    fn an_unknown_collection_type_is_refused_not_ignored() {
        assert!(wearables_query("collectionType=on-chian").is_err());
    }

    #[test]
    fn an_unknown_rarity_is_refused_with_the_explorer_wording() {
        assert!(wearables_query("rarity=ultra").is_err());
        assert_eq!(
            validate_rarity("ultra").unwrap_err(),
            "Invalid rarity requested: 'ultra'."
        );
    }

    #[test]
    fn ordering_needs_exactly_one_tier() {
        assert!(wearables_query("orderBy=rarity&collectionType=on-chain").is_ok());
        assert!(wearables_query("category=hat&orderBy=rarity").is_err());
        assert!(
            wearables_query("orderBy=rarity&collectionType=on-chain&collectionType=base-wearable")
                .is_err(),
            "two tiers cannot be globally ordered behind a tiered cursor"
        );
    }

    #[test]
    fn direction_defaults_per_sort_field_and_validates() {
        let q = wearables_query("collectionType=on-chain&orderBy=name").unwrap();
        assert_eq!(q.filters.extended.direction.as_deref(), Some("ASC"));
        let q = wearables_query("collectionType=on-chain&orderBy=rarity").unwrap();
        assert_eq!(q.filters.extended.direction.as_deref(), Some("DESC"));
        assert!(
            wearables_query("collectionType=on-chain&orderBy=rarity&direction=sideways").is_err()
        );
    }

    #[test]
    fn extended_filters_survive_pagination() {
        let q = wearables_query("collectionType=on-chain&category=hat&rarity=mythic&orderBy=date")
            .unwrap();
        let next = build_next_query(&q.filters, 50, "urn:c1:5", "wearableId");
        for expected in [
            "collectionType=on-chain",
            "category=hat",
            "rarity=mythic",
            "orderBy=date",
            "direction=DESC",
        ] {
            assert!(next.contains(expected), "next dropped {expected}: {next}");
        }
    }

    #[test]
    fn extended_filters_are_echoed_only_when_used() {
        let q = wearables_query("collectionType=base-wearable&category=eyewear").unwrap();
        let echoed = q.filters.to_json();
        assert_eq!(echoed["collectionTypes"], json!(["base-wearable"]));
        assert_eq!(echoed["categories"], json!(["eyewear"]));
        assert!(echoed.get("rarity").is_none());
        assert!(echoed.get("orderBy").is_none());
    }

    #[test]
    fn a_rarity_filter_excludes_the_ungraded_base_tier() {
        let q = wearables_query("rarity=mythic").unwrap();
        let base = BaseWearable {
            urn: "urn:decentraland:off-chain:base-avatars:aviatorstyle".into(),
            name: "aviatorstyle".into(),
            english_name: None,
            category: "eyewear".into(),
            entity: json!({}),
        };
        let got = filter_and_extract_base_wearables(&[base], &q.filters, &None, 10, "http://x");
        assert!(
            got.is_empty(),
            "base wearables carry no rarity, so a rarity filter must exclude them"
        );
    }

    #[test]
    fn parse_rejects_short_text_search() {
        let p = CatalogParams {
            text_search: Some("ab".into()),
            ..Default::default()
        };
        assert!(parse_plain(&p).is_err());
    }

    #[test]
    fn parse_accepts_collection_id_and_lowercases() {
        let p = CatalogParams {
            collection_id: vec!["URN:Decentraland".into()],
            ..Default::default()
        };
        assert_eq!(
            parse_plain(&p).unwrap().filters.collection_ids.unwrap(),
            vec!["urn:decentraland".to_string()]
        );
    }

    #[test]
    fn paginate_sorts_slices_and_reports_next() {
        let defs = vec![
            json!({ "id": "urn:c" }),
            json!({ "id": "urn:a" }),
            json!({ "id": "urn:b" }),
        ];
        let (items, next) = paginate(defs, 2);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "urn:a");
        assert_eq!(items[1]["id"], "urn:b");
        assert_eq!(next.as_deref(), Some("urn:b"));
    }

    #[test]
    fn paginate_no_overflow_has_no_next() {
        let (items, next) = paginate(vec![json!({ "id": "urn:a" })], 2);
        assert_eq!(items.len(), 1);
        assert!(next.is_none());
    }

    #[test]
    fn build_next_query_orders_params() {
        let f = CatalogFilters {
            collection_ids: Some(vec!["urn:c1".into()]),
            item_ids: None,
            text_search: Some("hat".into()),
            extended: ExtendedFilters::default(),
        };
        assert_eq!(
            build_next_query(&f, 50, "urn:c1:5", "wearableId"),
            "collectionId=urn%3Ac1&textSearch=hat&limit=50&lastId=urn%3Ac1%3A5"
        );
    }

    #[test]
    fn extract_emote_legacy_builds_adr74() {
        let e = json!({
            "content": [{ "file": "e.glb", "hash": "QmE" }, { "file": "image.png", "hash": "QmI" }, { "file": "thumbnail.png", "hash": "QmT" }],
            "metadata": {
                "id": "urn:emote",
                "name": "Wave",
                "image": "image.png",
                "thumbnail": "thumbnail.png",
                "emoteDataV0": { "loop": true },
                "data": { "tags": ["fun"], "representations": [{ "contents": ["e.glb"] }] }
            }
        });
        let def = extract_emote_definition(&e, "https://c/content/").unwrap();
        assert!(def.get("data").is_none());
        assert_eq!(def["emoteDataADR74"]["loop"], true);
        assert_eq!(def["emoteDataADR74"]["category"], "dance");
        assert_eq!(
            def["emoteDataADR74"]["representations"][0]["contents"][0]["url"],
            "https://c/content/contents/QmE"
        );
        assert_eq!(def["image"], "https://c/content/contents/QmI");
    }

    #[test]
    fn extract_emote_adr74_passthrough() {
        let e = json!({
            "content": [{ "file": "e.glb", "hash": "QmE" }],
            "metadata": {
                "id": "urn:emote",
                "name": "Wave",
                "emoteDataADR74": { "category": "fun", "loop": false, "representations": [{ "contents": ["e.glb"] }] }
            }
        });
        let def = extract_emote_definition(&e, "https://c/content/").unwrap();
        assert_eq!(def["emoteDataADR74"]["category"], "fun");
        assert_eq!(
            def["emoteDataADR74"]["representations"][0]["contents"][0]["url"],
            "https://c/content/contents/QmE"
        );
    }

    /// Fetches through `cache`, counting how often the fetch closure actually runs.
    async fn counted_fetch(
        cache: &TtlMap<String, Value>,
        key: &str,
        counter: &Arc<AtomicUsize>,
        value: Value,
    ) -> Value {
        let c = counter.clone();
        cache
            .get_or_fetch(key.to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(value)
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn outfits_cache_second_call_is_a_hit() {
        let cache: TtlMap<String, Value> =
            TtlMap::bounded("outfits_test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let v1 = counted_fetch(
            &cache,
            "0xabc",
            &counter,
            json!({ "id": "outfit-entity", "metadata": { "outfits": [] } }),
        )
        .await;
        let v2 = counted_fetch(&cache, "0xabc", &counter, json!(null)).await;
        assert_eq!(v1, v2);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn outfits_cache_caches_not_found_sentinel() {
        let cache: TtlMap<String, Value> =
            TtlMap::bounded("outfits_test_nf", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        counted_fetch(&cache, "0xnone", &counter, Value::Null).await;
        let v = counted_fetch(&cache, "0xnone", &counter, json!({ "x": 1 })).await;
        assert!(v.is_null(), "404 sentinel must HIT");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
