use serde::Serialize;
use sqlx::PgPool;

use crate::http::errors::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

// All the `*Out` structs in this module replace `json!({...})` payloads. The
// `wire_bytes_match_the_old_json_macro` tests below assert each one carries the
// same wire shape as the payload it replaced, compared as parsed JSON so object
// key order (which flips with serde_json's preserve_order feature) is not part
// of the contract.

/// Collection/item sync status served on curation and on-chain rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "builder/", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum CollectionStatus {
    Synced,
    UnderReview,
    Unsynced,
}

/// The only collection type this port serves (third-party collections are not
/// mirrored locally).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "builder/", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum CollectionType {
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "builder/", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum CurationStatus {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "builder/", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    SmartWearable,
    Emote,
    Wearable,
}

/// Row of `GET /v1/{address}/collections` (consumed by the sites creator-hub
/// collections list and metrics).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct BuilderCollectionOut {
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub chain_id: Option<i64>,
    pub contract_address: String,
    pub count: i32,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_at: Option<i64>,
    pub creator: Option<String>,
    pub id: String,
    pub is_approved: bool,
    pub is_published: bool,
    pub name: String,
    pub network: Option<String>,
    pub owner: Option<String>,
    pub pending: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub reviewed_at: Option<i64>,
    pub status: CollectionStatus,
    pub third_party_id: Option<String>,
    pub thumbs: Vec<String>,
    #[serde(rename = "type")]
    pub kind: CollectionType,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub updated_at: Option<i64>,
    pub urn: Option<String>,
}

/// Member of the curation committee (`GET /v1/collections/curation`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct CommitteeMemberOut {
    pub address: String,
    pub name: String,
}

/// Latest curation attached to a collection under review.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct ReviewCurationOut {
    pub assignee: String,
    pub collection_id: String,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_at: Option<i64>,
    pub id: String,
    pub status: CurationStatus,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub updated_at: Option<i64>,
}

/// Row of the `collections` list in `GET /v1/collections/curation`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct ReviewRowOut {
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_at: Option<i64>,
    pub curation: Option<ReviewCurationOut>,
    pub has_reviews: bool,
    pub id: String,
    pub is_approved: bool,
    pub is_programmatic: bool,
    pub item_count: i32,
    pub name: String,
    pub owner: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub reviewed_at: Option<i64>,
    pub status: CollectionStatus,
    #[serde(rename = "type")]
    pub kind: CollectionType,
}

/// Row of `GET /v1/{address}/items`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct OrphanItemOut {
    pub beneficiary: Option<String>,
    pub category: Option<String>,
    pub collection_id: Option<String>,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_at: Option<i64>,
    pub grad: Option<String>,
    pub id: String,
    pub image: Option<String>,
    pub is_published: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub max_supply: Option<i64>,
    pub name: String,
    pub network: Option<String>,
    pub price: Option<String>,
    pub rarity: Option<String>,
    pub status: CollectionStatus,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub total_supply: Option<i64>,
    #[serde(rename = "type")]
    pub kind: ItemKind,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub updated_at: Option<i64>,
    pub urn: Option<String>,
}

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

    pub async fn committee_members(&self) -> Result<Vec<CommitteeMemberOut>, ApiError> {
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
                    let name = short_address(&a);
                    Some(CommitteeMemberOut { address: a, name })
                }
            })
            .collect())
    }

    pub async fn collections_under_review(&self) -> Result<Vec<ReviewRowOut>, ApiError> {
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
        Ok(rows.into_iter().map(review_row_out).collect())
    }

    pub async fn collections_for_address(
        &self,
        address: &str,
    ) -> Result<Vec<BuilderCollectionOut>, ApiError> {
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
        Ok(rows.into_iter().map(collection_out).collect())
    }

    pub async fn items_for_address(
        &self,
        address: &str,
        only_orphans: bool,
    ) -> Result<Vec<OrphanItemOut>, ApiError> {
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
        Ok(rows.into_iter().map(item_out).collect())
    }
}

fn to_ms(secs: Option<i64>) -> Option<i64> {
    match secs {
        Some(s) if s > 0 => Some(s.saturating_mul(1000)),
        _ => None,
    }
}

fn status_of(is_published: bool, is_approved: bool) -> CollectionStatus {
    if is_approved {
        CollectionStatus::Synced
    } else if is_published {
        CollectionStatus::UnderReview
    } else {
        CollectionStatus::Unsynced
    }
}

fn collection_out(c: DbCollection) -> BuilderCollectionOut {
    let is_published = c.is_completed.unwrap_or(false);
    let is_approved = c.is_approved.unwrap_or(false);
    BuilderCollectionOut {
        chain_id: c.chain_id,
        contract_address: c.id.clone(),
        count: c.items_count.unwrap_or(0),
        created_at: to_ms(c.created_at),
        creator: c.creator,
        id: c.id,
        is_approved,
        is_published,
        name: c.name.unwrap_or_default(),
        network: c.network,
        owner: c.owner,
        pending: false,
        reviewed_at: to_ms(c.reviewed_at),
        status: status_of(is_published, is_approved),
        third_party_id: None,
        thumbs: Vec::new(),
        kind: CollectionType::Standard,
        updated_at: to_ms(c.updated_at),
        urn: c.urn,
    }
}

fn short_address(addr: &str) -> String {
    if addr.len() >= 10 {
        format!("{}\u{2026}{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn review_row_out(r: DbReviewRow) -> ReviewRowOut {
    let collection_id = r.id.clone();
    let is_completed = r.is_completed.unwrap_or(false);
    let is_approved = r.is_approved.unwrap_or(false);

    let curator = r
        .cur_curator
        .as_deref()
        .map(|c| c.split('-').next().unwrap_or(c).trim().to_ascii_lowercase());

    let curation = match (r.cur_id.as_ref(), curator) {
        (Some(cid), Some(addr)) => Some(ReviewCurationOut {
            assignee: addr,
            collection_id,
            created_at: to_ms(r.cur_timestamp),
            id: cid.clone(),
            status: if r.cur_is_approved.unwrap_or(false) {
                CurationStatus::Approved
            } else {
                CurationStatus::Rejected
            },
            updated_at: to_ms(r.cur_timestamp),
        }),
        _ => None,
    };

    ReviewRowOut {
        created_at: to_ms(r.created_at),
        curation,
        has_reviews: r.cur_id.is_some(),
        id: r.id,
        is_approved,
        is_programmatic: false,
        item_count: r.items_count.unwrap_or(0),
        name: r.name.unwrap_or_default(),
        owner: r.creator,
        reviewed_at: to_ms(r.reviewed_at),
        status: status_of(is_completed, is_approved),
        kind: CollectionType::Standard,
    }
}

fn map_item_type(t: &str) -> ItemKind {
    if t.starts_with("smart") {
        ItemKind::SmartWearable
    } else if t.starts_with("emote") {
        ItemKind::Emote
    } else {
        ItemKind::Wearable
    }
}

fn item_out(i: DbItem) -> OrphanItemOut {
    let item_type = i.item_type.as_deref().unwrap_or("wearable_v2");
    let mapped = map_item_type(item_type);

    let name = i.name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        i.urn
            .as_deref()
            .and_then(|u| u.rsplit(':').next())
            .map(|s| s.to_string())
            .unwrap_or_else(|| i.id.clone())
    });
    OrphanItemOut {
        beneficiary: i.beneficiary,
        category: i.category,
        collection_id: i.collection_id,
        created_at: to_ms(i.created_at),
        grad: None,
        id: i.id,
        image: i.image,
        is_published: true,
        max_supply: i.max_supply,
        name,
        network: i.network,
        price: i.price,
        rarity: i.rarity,
        status: CollectionStatus::Synced,
        total_supply: i.total_supply,
        kind: mapped,
        updated_at: to_ms(i.updated_at),
        urn: i.urn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn db_collection() -> DbCollection {
        DbCollection {
            id: "0xc0ffee".into(),
            owner: Some("0xowner".into()),
            creator: Some("0xcreator".into()),
            name: Some("Hats".into()),
            urn: Some("urn:decentraland:matic:collections-v2:0xc0ffee".into()),
            items_count: Some(3),
            is_completed: Some(true),
            is_approved: Some(false),
            created_at: Some(1_700_000_000),
            updated_at: Some(1_700_000_100),
            reviewed_at: None,
            network: Some("matic".into()),
            chain_id: Some(137),
        }
    }

    fn db_item() -> DbItem {
        DbItem {
            id: "item-1".into(),
            creator: Some("0xcreator".into()),
            item_type: Some("smart_wearable_v1".into()),
            rarity: Some("epic".into()),
            price: Some("1000000000000000000".into()),
            beneficiary: Some("0xbenef".into()),
            total_supply: Some(7),
            max_supply: Some(100),
            urn: Some("urn:decentraland:matic:collections-v2:0xc0ffee:0".into()),
            image: Some("https://img".into()),
            collection_id: Some("0xc0ffee".into()),
            created_at: Some(1_700_000_000),
            updated_at: None,
            name: Some("Cap".into()),
            category: Some("hat".into()),
            network: Some("matic".into()),
        }
    }

    fn db_review_row(with_curation: bool) -> DbReviewRow {
        DbReviewRow {
            id: "0xc0ffee".into(),
            name: Some("Hats".into()),
            creator: Some("0xcreator".into()),
            items_count: Some(3),
            is_completed: Some(true),
            is_approved: Some(false),
            created_at: Some(1_700_000_000),
            reviewed_at: None,
            cur_id: with_curation.then(|| "cur-1".into()),
            cur_is_approved: with_curation.then_some(false),
            cur_curator: with_curation.then(|| "0xCurator-0".into()),
            cur_timestamp: with_curation.then_some(1_700_000_200),
        }
    }

    fn old_to_ms(secs: Option<i64>) -> Value {
        match secs {
            Some(s) if s > 0 => json!(s.saturating_mul(1000)),
            _ => Value::Null,
        }
    }

    /// Each `*Out` struct must carry the same wire shape the retired
    /// `json!({...})` payload produced. Compared as parsed JSON -- object key
    /// order is not part of the contract and flips with serde_json's
    /// preserve_order feature under workspace-wide unification.
    #[test]
    fn collection_wire_bytes_match_the_old_json_macro() {
        let c = db_collection();
        let old = json!({
            "id": c.id,
            "name": c.name.clone().unwrap_or_default(),
            "type": "standard",
            "is_published": true,
            "is_approved": false,
            "reviewed_at": old_to_ms(c.reviewed_at),
            "created_at": old_to_ms(c.created_at),
            "updated_at": old_to_ms(c.updated_at),
            "contract_address": c.id,
            "third_party_id": Value::Null,
            "urn": c.urn,
            "status": "under_review",
            "pending": false,
            "count": c.items_count.unwrap_or(0),
            "thumbs": Value::Array(vec![]),
            "owner": c.owner,
            "creator": c.creator,
            "network": c.network,
            "chain_id": c.chain_id,
        });
        assert_eq!(
            serde_json::to_value(collection_out(db_collection())).unwrap(),
            old
        );
    }

    #[test]
    fn item_wire_bytes_match_the_old_json_macro() {
        let i = db_item();
        let old = json!({
            "id": i.id,
            "name": i.name,
            "type": "smart_wearable",
            "status": "synced",
            "createdAt": old_to_ms(i.created_at),
            "updatedAt": old_to_ms(i.updated_at),
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
        });
        assert_eq!(serde_json::to_value(item_out(db_item())).unwrap(), old);
    }

    #[test]
    fn review_row_wire_bytes_match_the_old_json_macro() {
        let r = db_review_row(true);
        let old = json!({
            "id": r.id,
            "name": r.name.clone().unwrap_or_default(),
            "type": "standard",
            "is_programmatic": false,
            "status": "under_review",
            "is_approved": false,
            "has_reviews": true,
            "item_count": r.items_count.unwrap_or(0),
            "owner": r.creator,
            "created_at": old_to_ms(r.created_at),
            "reviewed_at": old_to_ms(r.reviewed_at),
            "curation": {
                "id": "cur-1",
                "collection_id": r.id,
                "assignee": "0xcurator",
                "status": "rejected",
                "created_at": old_to_ms(r.cur_timestamp),
                "updated_at": old_to_ms(r.cur_timestamp),
            },
        });
        assert_eq!(
            serde_json::to_value(review_row_out(db_review_row(true))).unwrap(),
            old
        );

        let no_curation = serde_json::to_value(review_row_out(db_review_row(false))).unwrap();
        assert_eq!(no_curation["curation"], Value::Null);
        assert_eq!(no_curation["has_reviews"], json!(false));
    }

    #[test]
    fn committee_member_wire_bytes_match_the_old_json_macro() {
        let member = CommitteeMemberOut {
            address: "0xabcdef0123456789abcdef0123456789abcdef01".into(),
            name: short_address("0xabcdef0123456789abcdef0123456789abcdef01"),
        };
        let old = json!({
            "address": "0xabcdef0123456789abcdef0123456789abcdef01",
            "name": "0xabcd\u{2026}ef01",
        });
        assert_eq!(serde_json::to_value(&member).unwrap(), old);
    }
}
