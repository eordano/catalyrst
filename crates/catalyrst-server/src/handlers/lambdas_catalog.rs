use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Request, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::errors::{bad_request, not_found};
use crate::handlers::base_wearables::BASE_AVATARS_COLLECTION_ID;
use crate::handlers::definitions::{
    extract_emote_definition, extract_wearable_definition, SORTED_RARITIES,
};
use crate::query_params::{parse_query_string, qs_get_array, qs_get_string};
use crate::state::AppState;
use catalyrst_commons::cache::TtlMap;

const OUTFITS_CACHE_TTL: Duration = Duration::from_secs(60);
const OUTFITS_CACHE_MAX_ENTRIES: usize = 50_000;

const COLLECTIONS_CACHE_TTL: Duration = Duration::from_secs(300);

fn collections_cache() -> &'static Arc<TtlMap<String, Value>> {
    static C: OnceLock<Arc<TtlMap<String, Value>>> = OnceLock::new();
    C.get_or_init(|| {
        Arc::new(TtlMap::bounded(
            "nfts_collections",
            COLLECTIONS_CACHE_TTL,
            8,
        ))
    })
}

pub(crate) fn outfits_cache() -> &'static Arc<TtlMap<String, Value>> {
    static C: OnceLock<Arc<TtlMap<String, Value>>> = OnceLock::new();
    C.get_or_init(|| {
        Arc::new(TtlMap::bounded(
            "outfits",
            OUTFITS_CACHE_TTL,
            OUTFITS_CACHE_MAX_ENTRIES,
        ))
    })
}

/// A deployment can create or replace an outfits entity, so a client that
/// saves and re-reads within the TTL must not get the stale list (or a
/// cached not-found). Deployment success paths call this alongside their
/// `deployments_cache.clear()`.
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
/// Upstream's global catalog takes no `collectionType`: lamb2's
/// `parseCatalogQuery` reads only collectionId/itemIds/textSearch, so
/// `?collectionType=` alone answers the "you must use one of the filters" 400
/// and, alongside a real filter, is silently dropped (verified against
/// peer.decentraland.org: both the 400 body and the echoed `filters` object are
/// byte-identical to ours without this feature). `collectionType` is upstream's
/// parameter for the per-owner inventory endpoint
/// `/lambdas/explorer/{address}/wearables`, not for this one.
///
/// We accept it here as an extension. The invariant that keeps the divergence
/// honest: with no `collectionType` in the query the response is byte-identical
/// to upstream -- no new key in `filters`, no change to `next`, no change to the
/// 400. Every behaviour below is reachable only when a caller opts in by naming
/// at least one type.
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

/// Parses the opt-in `collectionType` filter, preserving caller order and
/// dropping repeats. An unrecognised value is refused rather than ignored: a
/// silently-dropped typo would widen the result set to everything, which is the
/// opposite of what the caller asked for.
fn parse_collection_types(raw: &[String]) -> Result<Vec<CollectionType>, Response> {
    let mut out: Vec<CollectionType> = Vec::new();
    for value in raw {
        match CollectionType::parse(value) {
            Some(t) if !out.contains(&t) => out.push(t),
            Some(_) => {}
            None => {
                return Err(bad_request(&format!(
                    "Invalid collectionType '{value}'. Valid values are: 'base-wearable', 'on-chain', 'third-party'"
                )))
            }
        }
    }
    Ok(out)
}

/// The catalog's extended filter set, all opt-in. Shares its vocabulary and its
/// error wording with `/lambdas/explorer/{address}/wearables` via
/// `handlers::definitions`, so a caller who learned the per-owner endpoint does
/// not have to learn a second dialect here.
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
    /// True when at least one of these opt-in filters narrows the result set, so
    /// the caller has expressed an intent and the upstream
    /// "you must use one of the filters" 400 no longer applies. `sort` alone
    /// does not count: ordering everything is not a filter.
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
        let mut parts = Vec::new();
        for t in &self.collection_types {
            parts.push(format!("collectionType={}", t.as_str()));
        }
        for c in &self.categories {
            parts.push(format!("category={}", urlencoding::encode(c)));
        }
        if let Some(ref r) = self.rarity {
            parts.push(format!("rarity={}", urlencoding::encode(r)));
        }
        if let Some(ref n) = self.name {
            parts.push(format!("name={}", urlencoding::encode(n)));
        }
        if let Some(ref s) = self.sort {
            parts.push(format!("orderBy={}", urlencoding::encode(s)));
            if let Some(ref d) = self.direction {
                parts.push(format!("direction={d}"));
            }
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
        if let Some(ref r) = self.rarity {
            m.insert("rarity".into(), json!(r));
        }
        if let Some(ref n) = self.name {
            m.insert("name".into(), json!(n));
        }
        if let Some(ref s) = self.sort {
            m.insert("orderBy".into(), json!(s));
            if let Some(ref d) = self.direction {
                m.insert("direction".into(), json!(d));
            }
        }
    }
}

/// Parses the extended filters and enforces the one rule the tiering makes
/// necessary: `orderBy` needs the query to resolve to a single tier.
///
/// Base wearables and on-chain items come from different stores (an in-memory
/// entity list and the squid index) behind a two-phase, urn-keyed cursor that
/// serves every base wearable before the first on-chain item. A global
/// `orderBy=rarity` across both would either have to buffer the whole on-chain
/// set to merge it, or return pages whose order contradicts the parameter.
/// Refusing is the only answer that does not lie, and naming `collectionType`
/// in the message points the caller straight at the fix.
fn parse_extended_filters(p: &CatalogParams) -> Result<ExtendedFilters, Response> {
    let collection_types = parse_collection_types(&p.collection_type)?;

    let categories: Vec<String> = p
        .category
        .iter()
        .map(|c| c.to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();

    let rarity = p
        .rarity
        .as_ref()
        .map(|r| r.to_lowercase())
        .filter(|r| !r.is_empty());
    if let Some(ref r) = rarity {
        crate::handlers::definitions::validate_rarity(r).map_err(|e| bad_request(&e))?;
    }

    let name = p
        .name
        .as_ref()
        .map(|n| n.to_lowercase())
        .filter(|n| !n.is_empty());

    let sort = p
        .order_by
        .as_ref()
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty());
    let direction = match (&sort, &p.direction) {
        (None, _) => None,
        (Some(s), Some(d)) if !d.is_empty() => {
            let d = d.to_uppercase();
            crate::handlers::definitions::validate_sort(s, &d).map_err(|e| bad_request(&e))?;
            Some(d)
        }
        (Some(s), _) => {
            let d = crate::handlers::definitions::default_sort_direction(s).to_string();
            crate::handlers::definitions::validate_sort(s, &d).map_err(|e| bad_request(&e))?;
            Some(d)
        }
    };

    let filters = ExtendedFilters {
        collection_types,
        categories,
        rarity,
        name,
        sort,
        direction,
    };

    if filters.sort.is_some() && filters.collection_types.len() != 1 {
        return Err(bad_request(
            "Sorting the global catalog needs a single collectionType: base wearables and on-chain items are paged from different stores, so 'orderBy' is only well defined within one of them. Add exactly one 'collectionType' (for example 'collectionType=on-chain').",
        ));
    }

    Ok(filters)
}

const MAX_LIMIT: i64 = 500;
const BASE_EMOTES_COLLECTION_ID: &str = "urn:decentraland:off-chain:base-emotes";

fn cursor_to_squid(last_id: &str) -> String {
    last_id.replace(":ethereum:", ":mainnet:")
}

#[derive(Debug, Deserialize, Default)]
pub struct CatalogParams {
    #[serde(default)]
    #[serde(rename = "collectionId")]
    collection_id: Vec<String>,
    #[serde(default)]
    #[serde(rename = "collectionType")]
    collection_type: Vec<String>,
    #[serde(default)]
    category: Vec<String>,
    rarity: Option<String>,
    name: Option<String>,
    #[serde(rename = "orderBy")]
    order_by: Option<String>,
    direction: Option<String>,
    #[serde(default)]
    #[serde(rename = "wearableId")]
    wearable_id: Vec<String>,
    #[serde(default)]
    #[serde(rename = "emoteId")]
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
    /// Empty for an upstream-shaped query, which is what keeps the echoed
    /// `filters` object byte-identical to stock catalyst until a caller opts in.
    extended: ExtendedFilters,
}

impl CatalogFilters {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(ref c) = self.collection_ids {
            m.insert("collectionIds".into(), json!(c));
        }
        if let Some(ref i) = self.item_ids {
            m.insert("itemIds".into(), json!(i));
        }
        if let Some(ref t) = self.text_search {
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

/// `extended` is `Default` for callers that do not offer the extension (the
/// emotes catalog), which keeps their behaviour byte-identical to upstream.
fn parse_catalog_query(
    p: &CatalogParams,
    item_ids_in: &[String],
    id_param_name: &str,
    extended: ExtendedFilters,
) -> Result<CatalogQuery, Response> {
    let collection_ids: Vec<String> = p.collection_id.iter().map(|s| s.to_lowercase()).collect();
    let item_ids: Vec<String> = item_ids_in.iter().map(|s| s.to_lowercase()).collect();
    let text_search = p
        .text_search
        .as_ref()
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty());
    let last_id = p
        .last_id
        .as_ref()
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty());

    // Upstream's rule, widened only by our own opt-in filters: an extended
    // filter that narrows the set is a filter, so it satisfies this the same way
    // collectionId does. With none supplied `extended` is empty and the 400 --
    // message included -- is byte-identical to stock catalyst.
    if collection_ids.is_empty()
        && item_ids.is_empty()
        && text_search.is_none()
        && !extended.narrows()
    {
        return Err(bad_request(&format!(
            "You must use one of the filters: 'textSearch', 'collectionId' or '{id_param_name}'"
        )));
    }
    if let Some(ref t) = text_search {
        if t.chars().count() < 3 {
            return Err(bad_request(
                "The text search must be at least 3 characters long",
            ));
        }
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
            collection_ids: if collection_ids.is_empty() {
                None
            } else {
                Some(collection_ids)
            },
            item_ids: if item_ids.is_empty() {
                None
            } else {
                Some(item_ids)
            },
            text_search,
            extended,
        },
        limit: clamp_limit(&p.limit),
        last_id,
    })
}

fn build_next_query(
    filters: &CatalogFilters,
    limit: i64,
    next_last_id: &str,
    id_param_name: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ref ids) = filters.collection_ids {
        for id in ids {
            parts.push(format!("collectionId={}", urlencoding::encode(id)));
        }
    }
    if let Some(ref ids) = filters.item_ids {
        for id in ids {
            parts.push(format!("{}={}", id_param_name, urlencoding::encode(id)));
        }
    }
    if let Some(ref t) = filters.text_search {
        parts.push(format!("textSearch={}", urlencoding::encode(t)));
    }
    // Empty unless the caller opted in, so `next` stays byte-identical upstream.
    parts.extend(filters.extended.query_parts());
    parts.push(format!("limit={limit}"));
    parts.push(format!("lastId={}", urlencoding::encode(next_last_id)));
    parts.join("&")
}

enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
}

async fn fetch_item_urns(
    pool: &PgPool,
    item_type_prefix: &str,
    filters: &CatalogFilters,
    limit: i64,
    last_id: &Option<String>,
) -> Vec<String> {
    let item_type_clause = if item_type_prefix == "wearable" {
        "(item_type LIKE 'wearable%' OR item_type LIKE 'smart_wearable%')"
    } else {
        "item_type LIKE 'emote%'"
    };
    let mut sql = format!(
        "SELECT urn FROM squid_marketplace.item \
         WHERE urn IS NOT NULL AND {item_type_clause}"
    );
    let mut binds: Vec<Bind> = Vec::new();

    let mut idx = 1;

    if let Some(ref cids) = filters.collection_ids {
        sql.push_str(" AND (");
        let mut clauses = Vec::new();
        for c in cids {
            clauses.push(format!("lower(urn) LIKE ${idx}"));
            binds.push(Bind::Text(format!(
                "{}:%",
                cursor_to_squid(&c.to_lowercase())
            )));
            idx += 1;
        }
        sql.push_str(&clauses.join(" OR "));
        sql.push(')');
    }

    if let Some(ref iids) = filters.item_ids {
        sql.push_str(&format!(" AND lower(urn) = ANY(${idx})"));
        binds.push(Bind::TextArray(
            iids.iter()
                .map(|s| cursor_to_squid(&s.to_lowercase()))
                .collect(),
        ));
        idx += 1;
    }

    if let Some(ref t) = filters.text_search {
        sql.push_str(&format!(" AND search_text ILIKE ${idx}"));
        binds.push(Bind::Text(format!("%{t}%")));
        idx += 1;
    }

    let ext = &filters.extended;

    if !ext.categories.is_empty() {
        let column = if item_type_prefix == "wearable" {
            "search_wearable_category"
        } else {
            "search_emote_category"
        };
        sql.push_str(&format!(" AND lower({column}::text) = ANY(${idx})"));
        binds.push(Bind::TextArray(ext.categories.clone()));
        idx += 1;
    }

    if let Some(ref r) = ext.rarity {
        let column = if item_type_prefix == "wearable" {
            "search_wearable_rarity"
        } else {
            "search_emote_rarity"
        };
        sql.push_str(&format!(" AND lower({column}) = ${idx}"));
        binds.push(Bind::Text(r.clone()));
        idx += 1;
    }

    // `name` narrows the same indexed column `textSearch` uses; the two are
    // separate parameters because the explorer surface names them separately,
    // and a caller may legitimately pass both.
    if let Some(ref n) = ext.name {
        sql.push_str(&format!(" AND search_text ILIKE ${idx}"));
        binds.push(Bind::Text(format!("%{n}%")));
        idx += 1;
    }

    if let Some(ref cursor) = last_id {
        sql.push_str(&format!(" AND lower(urn) > ${idx}"));
        binds.push(Bind::Text(cursor_to_squid(&cursor.to_lowercase())));
        idx += 1;
    }

    // The cursor stays urn-keyed in every case, so an ordered page is the
    // requested sort applied to the urn-ordered window rather than a global
    // re-ordering. `parse_extended_filters` is what makes that honest: it
    // refuses `orderBy` unless the query names a single tier, and the ordering
    // below is a stable tiebreak on urn so pages never overlap or skip.
    let order = match (ext.sort.as_deref(), ext.direction.as_deref()) {
        (Some("rarity"), Some(dir)) => {
            let column = if item_type_prefix == "wearable" {
                "search_wearable_rarity"
            } else {
                "search_emote_rarity"
            };
            let ranks: Vec<String> = SORTED_RARITIES
                .iter()
                .enumerate()
                .map(|(i, r)| format!("WHEN '{r}' THEN {i}"))
                .collect();
            format!(
                "CASE lower({column}) {} ELSE -1 END {dir}, urn ASC",
                ranks.join(" ")
            )
        }
        (Some("date"), Some(dir)) => format!("created_at {dir}, urn ASC"),
        (Some("name"), Some(dir)) => format!("lower(search_text) {dir}, urn ASC"),
        _ => "urn ASC".to_string(),
    };
    sql.push_str(&format!(" ORDER BY {order} LIMIT ${idx}"));
    binds.push(Bind::Int(limit + 1));

    let mut q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
    for b in binds {
        q = match b {
            Bind::Text(s) => q.bind(s),
            Bind::TextArray(a) => q.bind(a),
            Bind::Int(n) => q.bind(n),
        };
    }

    q.fetch_all(pool).await.unwrap_or_default()
}

fn paginate(mut definitions: Vec<Value>, limit: i64) -> (Vec<Value>, Option<String>) {
    definitions.sort_by(|a, b| {
        let ai = a
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let bi = b
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        ai.cmp(&bi)
    });
    let has_more = definitions.len() as i64 > limit;
    if has_more {
        definitions.truncate(limit as usize);
    }
    let next = if has_more {
        definitions
            .last()
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    (definitions, next)
}

fn paginate_merged(
    pre_merge: Vec<Value>,
    mut on_chain: Vec<Value>,
    limit: i64,
) -> (Vec<Value>, Option<String>) {
    on_chain.sort_by(|a, b| {
        let ai = a
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let bi = b
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        ai.cmp(&bi)
    });
    let mut merged = pre_merge;
    merged.extend(on_chain);
    let has_more = merged.len() as i64 > limit;
    if has_more {
        merged.truncate(limit as usize);
    }
    let next = if has_more {
        merged
            .last()
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    (merged, next)
}

fn filter_and_extract_base_wearables(
    base: &[crate::handlers::base_wearables::BaseWearable],
    filters: &CatalogFilters,
    last_id: &Option<String>,
    max_results: usize,
    content_public_url: &str,
) -> Vec<Value> {
    let mut matched: Vec<&crate::handlers::base_wearables::BaseWearable> = base
        .iter()
        .filter(|w| {
            let lc_urn = w.urn.to_lowercase();
            if let Some(lid) = last_id {
                if &lc_urn <= lid {
                    return false;
                }
            }
            if let Some(ref ids) = filters.item_ids {
                if !ids.contains(&lc_urn) {
                    return false;
                }
            }
            if let Some(ref t) = filters.text_search {
                let haystack = w.english_name.as_deref().unwrap_or(&w.name).to_lowercase();
                if !haystack.contains(t) {
                    return false;
                }
            }
            let ext = &filters.extended;
            if !ext.categories.is_empty() && !ext.categories.contains(&w.category.to_lowercase()) {
                return false;
            }
            // Base wearables carry no rarity. A rarity filter is therefore a
            // statement that the caller wants graded items, which this tier has
            // none of -- narrowing it to empty is the honest answer, not
            // ignoring the parameter and returning ungraded ones.
            if ext.rarity.is_some() {
                return false;
            }
            if let Some(ref n) = ext.name {
                let haystack = w.english_name.as_deref().unwrap_or(&w.name).to_lowercase();
                if !haystack.contains(n) {
                    return false;
                }
            }
            true
        })
        .collect();
    matched.sort_by_key(|a| a.urn.to_lowercase());
    matched.truncate(max_results);
    matched
        .into_iter()
        .filter_map(|w| extract_wearable_definition(&w.entity, content_public_url))
        .collect()
}

async fn catalog_wearables_with_base(state: &AppState, query: CatalogQuery) -> Response {
    let content_public_url = &state.content_public_url;
    let filters = &query.filters;
    let limit = query.limit;

    let only_base_collection = matches!(
        &filters.collection_ids,
        Some(c) if c.len() == 1 && c[0] == BASE_AVATARS_COLLECTION_ID
    );
    let base_collection_allowed = match &filters.collection_ids {
        None => true,
        Some(c) => c.iter().any(|id| id == BASE_AVATARS_COLLECTION_ID),
    } && filters.extended.allows(CollectionType::BaseWearable);

    // `third-party` is a real upstream tier that this endpoint has no source
    // for: the catalog reads base wearables and the squid index, neither of
    // which holds third-party items. Naming it alone therefore selects nothing
    // here, which is why it is accepted as a value but contributes no branch.
    let on_chain_allowed = filters.extended.allows(CollectionType::OnChain);

    let mut off_chain: Vec<Value> = Vec::new();
    let mut on_chain_cursor = query.last_id.clone();
    let cursor_in_base_range = query
        .last_id
        .as_ref()
        .map(|l| l.starts_with(BASE_AVATARS_COLLECTION_ID))
        .unwrap_or(true);
    if base_collection_allowed && cursor_in_base_range {
        let base = crate::handlers::base_wearables::fetch_base_wearables(state).await;

        off_chain = filter_and_extract_base_wearables(
            &base,
            filters,
            &query.last_id,
            (limit + 1) as usize,
            content_public_url,
        );
        on_chain_cursor = None;
    }

    let remaining = limit - off_chain.len() as i64;
    let mut on_chain_defs: Vec<Value> = Vec::new();
    if !only_base_collection && on_chain_allowed && remaining >= 0 {
        if let Some(pool) = state.squid_pool.as_ref() {
            let urns =
                fetch_item_urns(pool, "wearable", filters, remaining + 1, &on_chain_cursor).await;
            if !urns.is_empty() {
                let pointers: Vec<String> = urns
                    .iter()
                    .map(|u| u.replacen(":mainnet:", ":ethereum:", 1).to_lowercase())
                    .collect();
                let entities = state
                    .database
                    .active_entities_by_pointers(&pointers)
                    .await
                    .unwrap_or_default();
                on_chain_defs = entities
                    .iter()
                    .filter_map(|e| extract_wearable_definition(e, content_public_url))
                    .collect();
            }
        }
    }

    let (items, next_last_id) = paginate_merged(off_chain, on_chain_defs, limit);

    let next =
        next_last_id.map(|nl| format!("?{}", build_next_query(filters, limit, &nl, "wearableId")));

    let mut pagination = Map::new();
    pagination.insert("limit".into(), json!(limit));
    if let Some(n) = next {
        pagination.insert("next".into(), json!(n));
    }

    let body = json!({
        "wearables": items,
        "filters": filters.to_json(),
        "pagination": Value::Object(pagination),
    });
    Json(body).into_response()
}

async fn catalog(
    state: &AppState,
    query: CatalogQuery,
    item_type_prefix: &str,
    id_param_name: &str,
    items_key: &str,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Response {
    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => {
            let body = json!({
                items_key: [],
                "filters": query.filters.to_json(),
                "pagination": { "limit": query.limit },
            });
            return Json(body).into_response();
        }
    };

    let urns = fetch_item_urns(
        pool,
        item_type_prefix,
        &query.filters,
        query.limit,
        &query.last_id,
    )
    .await;

    let pointers: Vec<String> = urns.iter().map(|u| u.to_lowercase()).collect();
    let entities = if pointers.is_empty() {
        Vec::new()
    } else {
        state
            .database
            .active_entities_by_pointers(&pointers)
            .await
            .unwrap_or_default()
    };

    let content_public_url = &state.content_public_url;
    let definitions: Vec<Value> = entities
        .iter()
        .filter_map(|e| extract(e, content_public_url))
        .collect();

    let (items, next_last_id) = paginate(definitions, query.limit);

    let next = next_last_id.map(|nl| {
        format!(
            "?{}",
            build_next_query(&query.filters, query.limit, &nl, id_param_name)
        )
    });

    let mut pagination = Map::new();
    pagination.insert("limit".into(), json!(query.limit));
    if let Some(n) = next {
        pagination.insert("next".into(), json!(n));
    }

    let body = json!({
        items_key: items,
        "filters": query.filters.to_json(),
        "pagination": Value::Object(pagination),
    });
    Json(body).into_response()
}

pub async fn collections_wearables_catalog(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let params = catalog_params_from_query(request.uri().query().unwrap_or(""));
    let extended = match parse_extended_filters(&params) {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    let query = match parse_catalog_query(&params, &params.wearable_id, "wearableId", extended) {
        Ok(q) => q,
        Err(resp) => return resp,
    };
    catalog_wearables_with_base(&state, query).await
}

pub async fn collections_emotes_catalog(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response {
    let params = catalog_params_from_query(request.uri().query().unwrap_or(""));
    // The emotes catalog is upstream-shaped: it has no off-chain tier to select
    // between, so it does not offer the extension and stays byte-identical.
    let query = match parse_catalog_query(
        &params,
        &params.emote_id,
        "emoteId",
        ExtendedFilters::default(),
    ) {
        Ok(q) => q,
        Err(resp) => return resp,
    };
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

pub async fn nfts_collections(State(state): State<Arc<AppState>>) -> Response {
    let network = state.eth_network.clone();
    let pool = state.squid_pool.clone();
    let cached = collections_cache()
        .get_or_fetch(network.clone(), move || async move {
            use crate::handlers::external_graph;

            let mut collections: Vec<Value> = vec![
                json!({ "id": BASE_AVATARS_COLLECTION_ID, "name": "Base Wearables" }),
                json!({ "id": BASE_EMOTES_COLLECTION_ID, "name": "Base Emotes" }),
            ];

            let local: Vec<(String, String)> = if let Some(pool) = pool.as_ref() {
                let (eth, poly) = tokio::join!(
                    external_graph::collections_from_squid(
                        pool,
                        "ETHEREUM",
                        Some((":mainnet:", ":ethereum:")),
                    ),
                    external_graph::collections_from_squid(pool, "POLYGON", None),
                );
                eth.unwrap_or_default()
                    .into_iter()
                    .chain(poly.unwrap_or_default())
                    .collect()
            } else {
                Vec::new()
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
                        l1.unwrap_or_default()
                            .into_iter()
                            .chain(l2.unwrap_or_default())
                            .collect()
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "collection catalog upstream unavailable");
                        Vec::new()
                    }
                }
            };
            for (urn, name) in items {
                collections.push(json!({ "id": urn, "name": name }));
            }

            Ok::<Value, ()>(json!({ "collections": collections }))
        })
        .await;

    match cached {
        Ok(v) => Json(v).into_response(),

        Err(_) => Json(json!({ "collections": [
            json!({ "id": BASE_AVATARS_COLLECTION_ID, "name": "Base Wearables" }),
            json!({ "id": BASE_EMOTES_COLLECTION_ID, "name": "Base Emotes" }),
        ] }))
        .into_response(),
    }
}

pub async fn outfits(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let address = id.to_lowercase();
    let state_arc = state.clone();
    let address_for_fetch = address.clone();
    let cached = outfits_cache()
        .get_or_fetch(address.clone(), move || async move {
            let pointer = format!("{address_for_fetch}:outfits");
            let entity = match state_arc.database.find_entity_by_pointer(&pointer).await {
                Ok(Some(e)) => e,
                Ok(None) | Err(_) => return Ok::<Value, ()>(Value::Null),
            };

            let owned_names: Vec<String> = match state_arc.squid_pool.as_ref() {
                Some(pool) => {
                    super::profile_processing::fetch_owned_ens_names(pool, &address_for_fetch).await
                }
                None => Vec::new(),
            };

            let mut entity = entity;
            let has_names = !owned_names.is_empty();
            if let Some(metadata) = entity.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                if !has_names {
                    if let Some(outfits) =
                        metadata.get_mut("outfits").and_then(|o| o.as_array_mut())
                    {
                        outfits.retain(|o| {
                            o.get("slot")
                                .and_then(|s| s.as_i64())
                                .map(|s| s <= 4)
                                .unwrap_or(true)
                        });
                    }
                }
                metadata.insert("namesForExtraSlots".into(), json!(owned_names));
            }
            Ok::<Value, ()>(entity)
        })
        .await;

    match cached {
        Ok(v) if v.is_null() => not_found("Outfits not found"),
        Ok(v) => Json(v).into_response(),
        Err(_) => not_found("Outfits not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert_eq!(
            cursor_to_squid("urn:decentraland:ethereum:collections-v1:0xabc:0"),
            "urn:decentraland:mainnet:collections-v1:0xabc:0"
        );

        assert_eq!(
            cursor_to_squid("urn:decentraland:matic:collections-v2:0xabc:0"),
            "urn:decentraland:matic:collections-v2:0xabc:0"
        );

        assert_eq!(
            cursor_to_squid("urn:decentraland:off-chain:base-avatars:eyes_00"),
            "urn:decentraland:off-chain:base-avatars:eyes_00"
        );

        assert_eq!(
            cursor_to_squid(":ethereum::ethereum:"),
            ":mainnet::mainnet:"
        );
    }

    #[test]
    fn clamp_limit_defaults_and_caps() {
        assert_eq!(clamp_limit(&None), MAX_LIMIT);
        assert_eq!(clamp_limit(&Some("0".into())), MAX_LIMIT);
        assert_eq!(clamp_limit(&Some("9999".into())), MAX_LIMIT);
        assert_eq!(clamp_limit(&Some("abc".into())), MAX_LIMIT);
        assert_eq!(clamp_limit(&Some("10".into())), 10);
    }

    #[test]
    fn parse_requires_a_filter() {
        let p = CatalogParams::default();
        assert!(parse_catalog_query(&p, &[], "wearableId", ExtendedFilters::default()).is_err());
    }

    fn wearables_query(qs: &str) -> Result<CatalogQuery, Response> {
        let p = catalog_params_from_query(qs);
        let extended = parse_extended_filters(&p)?;
        parse_catalog_query(&p, &p.wearable_id, "wearableId", extended)
    }

    // The invariant the whole divergence rests on: an upstream-shaped query must
    // be indistinguishable from upstream. If this fails, the extension has
    // leaked into the default surface.
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
        // orderBy narrows nothing, so it must not unlock the unfiltered catalog.
        // It fails on the single-tier rule first, which is still a 400.
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
            crate::handlers::definitions::validate_rarity("ultra").unwrap_err(),
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
        let base = crate::handlers::base_wearables::BaseWearable {
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
        assert!(parse_catalog_query(&p, &[], "wearableId", ExtendedFilters::default()).is_err());
    }

    #[test]
    fn parse_accepts_collection_id_and_lowercases() {
        let p = CatalogParams {
            collection_id: vec!["URN:Decentraland".into()],
            ..Default::default()
        };
        let q = parse_catalog_query(&p, &[], "wearableId", ExtendedFilters::default()).unwrap();
        assert_eq!(
            q.filters.collection_ids.unwrap(),
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
        let defs = vec![json!({ "id": "urn:a" })];
        let (items, next) = paginate(defs, 2);
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
        let q = build_next_query(&f, 50, "urn:c1:5", "wearableId");
        assert_eq!(
            q,
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

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    use std::time::Duration as StdDuration;

    #[tokio::test]
    async fn outfits_cache_second_call_is_a_hit() {
        let cache: TtlMap<String, Value> =
            TtlMap::bounded("outfits_test", StdDuration::from_secs(60), 100);
        let counter = StdArc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let v1 = cache
            .get_or_fetch("0xabc".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!({ "id": "outfit-entity", "metadata": { "outfits": [] } }))
            })
            .await
            .unwrap();
        let c = counter.clone();
        let v2 = cache
            .get_or_fetch("0xabc".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!(null))
            })
            .await
            .unwrap();
        assert_eq!(v1, v2);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn outfits_cache_caches_not_found_sentinel() {
        let cache: TtlMap<String, Value> =
            TtlMap::bounded("outfits_test_nf", StdDuration::from_secs(60), 100);
        let counter = StdArc::new(AtomicUsize::new(0));

        let c = counter.clone();
        cache
            .get_or_fetch("0xnone".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(Value::Null)
            })
            .await
            .unwrap();
        let c = counter.clone();
        let v = cache
            .get_or_fetch("0xnone".to_string(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!({ "x": 1 }))
            })
            .await
            .unwrap();
        assert!(v.is_null(), "404 sentinel must HIT");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
