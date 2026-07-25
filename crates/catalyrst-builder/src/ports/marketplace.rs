use serde_json::{json, Value};
use sqlx::PgPool;

use crate::http::errors::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

const MAX_ROWS: i64 = 1000;

const MAX_REVIEW_ROWS: i64 = 1000;

pub struct MarketplaceComponent {
    pool: PgPool,
}

#[derive(Debug, sqlx::FromRow)]
struct DbCollection {
    id: String,
    owner: Option<String>,
    creator: Option<String>,
    name: Option<String>,
    urn: Option<String>,
    items_count: Option<i32>,
    is_completed: Option<bool>,
    is_approved: Option<bool>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    reviewed_at: Option<i64>,
    network: Option<String>,
    chain_id: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct DbItem {
    id: String,
    #[allow(dead_code)]
    creator: Option<String>,
    item_type: Option<String>,
    rarity: Option<String>,
    price: Option<String>,
    beneficiary: Option<String>,
    total_supply: Option<i64>,
    max_supply: Option<i64>,
    urn: Option<String>,
    image: Option<String>,
    collection_id: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
    name: Option<String>,
    category: Option<String>,
    network: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct DbReviewRow {
    id: String,
    name: Option<String>,
    creator: Option<String>,
    items_count: Option<i32>,
    is_completed: Option<bool>,
    is_approved: Option<bool>,
    created_at: Option<i64>,
    reviewed_at: Option<i64>,
    cur_id: Option<String>,
    cur_is_approved: Option<bool>,
    cur_curator: Option<String>,
    cur_timestamp: Option<i64>,
}

impl MarketplaceComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn committee_members(&self) -> Result<Vec<Value>, ApiError> {
        let sql = format!(
            "SELECT DISTINCT split_part(curator_id, '-', 1) AS address \
             FROM {schema}.curation \
             WHERE curator_id IS NOT NULL \
             ORDER BY address",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let rows = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(addr,)| {
                let a = addr.trim().to_ascii_lowercase();
                if a.is_empty() || !a.starts_with("0x") {
                    None
                } else {
                    Some(json!({ "address": a, "name": short_address(&a) }))
                }
            })
            .collect())
    }

    pub async fn collections_under_review(&self) -> Result<Vec<Value>, ApiError> {
        let sql = format!(
            "SELECT c.id, c.name, c.creator, c.items_count, \
                    c.is_completed, c.is_approved, \
                    c.created_at::int8 AS created_at, \
                    c.reviewed_at::int8 AS reviewed_at, \
                    cur.id AS cur_id, \
                    cur.is_approved AS cur_is_approved, \
                    cur.curator_id AS cur_curator, \
                    cur.timestamp::int8 AS cur_timestamp \
             FROM {schema}.collection c \
             LEFT JOIN LATERAL ( \
                 SELECT id, is_approved, curator_id, timestamp \
                 FROM {schema}.curation cu \
                 WHERE cu.collection_id = c.id \
                 ORDER BY cu.timestamp DESC \
                 LIMIT 1 \
             ) cur ON true \
             WHERE c.is_completed AND NOT c.is_approved \
             ORDER BY c.created_at DESC \
             LIMIT $1",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let rows = sqlx::query_as::<_, DbReviewRow>(sqlx::AssertSqlSafe(sql))
            .bind(MAX_REVIEW_ROWS)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(review_row_to_json).collect())
    }

    pub async fn collections_for_address(&self, address: &str) -> Result<Vec<Value>, ApiError> {
        let addr = address.to_ascii_lowercase();
        let sql = format!(
            "SELECT id, owner, creator, name, urn, \
                    items_count, is_completed, is_approved, \
                    created_at::int8 AS created_at, \
                    updated_at::int8 AS updated_at, \
                    reviewed_at::int8 AS reviewed_at, \
                    network, chain_id::int8 AS chain_id \
             FROM {schema}.collection \
             WHERE creator = $1 OR owner = $1 \
                OR $1 = ANY(managers) OR $1 = ANY(minters) \
             ORDER BY created_at DESC \
             LIMIT $2",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let rows = sqlx::query_as::<_, DbCollection>(sqlx::AssertSqlSafe(sql))
            .bind(&addr)
            .bind(MAX_ROWS)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(collection_to_json).collect())
    }

    pub async fn items_for_address(
        &self,
        address: &str,
        only_orphans: bool,
    ) -> Result<Vec<Value>, ApiError> {
        let addr = address.to_ascii_lowercase();
        let sql = format!(
            "SELECT i.id, i.creator, i.item_type, i.rarity, \
                    i.price::text AS price, i.beneficiary, \
                    i.total_supply::int8 AS total_supply, \
                    i.max_supply::int8 AS max_supply, \
                    i.urn, i.image, i.collection_id, \
                    i.created_at::int8 AS created_at, \
                    i.updated_at::int8 AS updated_at, \
                    COALESCE(w.name, e.name) AS name, \
                    COALESCE(w.category, e.category) AS category, \
                    i.network \
             FROM {schema}.item i \
             LEFT JOIN {schema}.metadata m ON m.id = i.metadata_id \
             LEFT JOIN {schema}.wearable w ON w.id = m.wearable_id \
             LEFT JOIN {schema}.emote e ON e.id = m.emote_id \
             WHERE i.creator = $1 AND (NOT $2 OR i.collection_id IS NULL) \
             ORDER BY i.created_at DESC \
             LIMIT $3",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let rows = sqlx::query_as::<_, DbItem>(sqlx::AssertSqlSafe(sql))
            .bind(&addr)
            .bind(only_orphans)
            .bind(MAX_ROWS)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(item_to_json).collect())
    }
}

fn to_ms(secs: Option<i64>) -> Value {
    match secs {
        Some(s) if s > 0 => json!(s.saturating_mul(1000)),
        _ => Value::Null,
    }
}

fn collection_to_json(c: DbCollection) -> Value {
    let is_published = c.is_completed.unwrap_or(false);
    let is_approved = c.is_approved.unwrap_or(false);
    let status = if is_approved {
        "synced"
    } else if is_published {
        "under_review"
    } else {
        "unsynced"
    };
    json!({
        "id": c.id,
        "name": c.name.unwrap_or_default(),
        "type": "standard",
        "is_published": is_published,
        "is_approved": is_approved,
        "reviewed_at": to_ms(c.reviewed_at),
        "created_at": to_ms(c.created_at),
        "updated_at": to_ms(c.updated_at),
        "contract_address": c.id,
        "third_party_id": Value::Null,
        "urn": c.urn,
        "status": status,
        "pending": false,
        "count": c.items_count.unwrap_or(0),
        "thumbs": Value::Array(vec![]),

        "owner": c.owner,
        "creator": c.creator,
        "network": c.network,
        "chain_id": c.chain_id,
    })
}

fn short_address(addr: &str) -> String {
    if addr.len() >= 10 {
        format!("{}…{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn review_row_to_json(r: DbReviewRow) -> Value {
    let collection_id = r.id.clone();
    let is_completed = r.is_completed.unwrap_or(false);
    let is_approved = r.is_approved.unwrap_or(false);
    let status = if is_approved {
        "synced"
    } else if is_completed {
        "under_review"
    } else {
        "unsynced"
    };

    let curator = r
        .cur_curator
        .as_deref()
        .map(|c| c.split('-').next().unwrap_or(c).trim().to_ascii_lowercase());

    let curation = match (r.cur_id.as_ref(), curator.as_ref()) {
        (Some(cid), Some(addr)) => {
            let cstatus = if r.cur_is_approved.unwrap_or(false) {
                "approved"
            } else {
                "rejected"
            };
            json!({
                "id": cid,
                "collection_id": collection_id,
                "assignee": addr,
                "status": cstatus,
                "created_at": to_ms(r.cur_timestamp),
                "updated_at": to_ms(r.cur_timestamp),
            })
        }
        _ => Value::Null,
    };

    json!({
        "id": r.id,
        "name": r.name.unwrap_or_default(),
        "type": "standard",
        "is_programmatic": false,
        "status": status,
        "is_approved": is_approved,
        "has_reviews": r.cur_id.is_some(),
        "item_count": r.items_count.unwrap_or(0),
        "owner": r.creator,
        "created_at": to_ms(r.created_at),
        "reviewed_at": to_ms(r.reviewed_at),
        "curation": curation,
    })
}

fn map_item_type(t: &str) -> &'static str {
    if t.starts_with("smart") {
        "smart_wearable"
    } else if t.starts_with("emote") {
        "emote"
    } else {
        "wearable"
    }
}

fn item_to_json(i: DbItem) -> Value {
    let item_type = i.item_type.as_deref().unwrap_or("wearable_v2");
    let mapped = map_item_type(item_type);

    let name = i.name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        i.urn
            .as_deref()
            .and_then(|u| u.rsplit(':').next())
            .map(|s| s.to_string())
            .unwrap_or_else(|| i.id.clone())
    });
    json!({
        "id": i.id,
        "name": name,
        "type": mapped,

        "status": "synced",
        "createdAt": to_ms(i.created_at),
        "updatedAt": to_ms(i.updated_at),
        "grad": Value::Null,

        "rarity": i.rarity,
        "category": i.category,
        "price": i.price,
        "beneficiary": i.beneficiary,
        "total_supply": i.total_supply,
        "max_supply": i.max_supply,
        "urn": i.urn,
        "image": i.image,
        "collection_id": i.collection_id,
        "is_published": true,
        "network": i.network,
    })
}
