use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::http::errors::ApiError;

#[derive(Debug, Default)]
pub struct ItemQuery {
    pub status: Option<String>,
    pub mapping_status: Option<String>,
    pub synced: Option<bool>,
    pub name: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug)]
pub struct ItemRow {
    pub id: Uuid,
    pub urn_suffix: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub video: Option<String>,
    pub eth_address: String,
    pub collection_id: Option<Uuid>,
    pub blockchain_item_id: Option<String>,
    pub price: Option<String>,
    pub beneficiary: Option<String>,
    pub rarity: Option<String>,
    pub item_type: String,
    pub data: Value,
    pub metrics: Option<Value>,
    pub utility: Option<String>,
    pub mappings: Option<Value>,
    pub is_published: bool,
    pub is_approved: bool,
    pub in_catalyst: bool,
    pub total_supply: i64,
    pub local_content_hash: Option<String>,
    pub content_hash: Option<String>,
    pub catalyst_content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub contents: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct FullItemOut {
    pub id: String,
    pub urn: Option<String>,
    pub name: String,
    pub description: String,
    pub thumbnail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub video: Option<String>,
    pub eth_address: String,
    pub collection_id: Option<String>,
    pub blockchain_item_id: Option<String>,
    pub price: Option<String>,
    pub beneficiary: Option<String>,
    pub rarity: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub data: Value,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub metrics: Value,
    pub utility: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown> | null"))]
    pub mappings: Option<Value>,
    pub contents: BTreeMap<String, String>,
    pub is_published: bool,
    pub is_approved: bool,
    pub in_catalyst: bool,
    pub total_supply: i64,
    pub content_hash: Option<String>,
    pub local_content_hash: Option<String>,
    pub catalyst_content_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ItemRow {
    pub fn to_out(&self) -> FullItemOut {
        FullItemOut {
            id: self.id.to_string(),
            urn: self.urn_suffix.clone(),
            name: self.name.clone(),
            description: self.description.clone().unwrap_or_default(),
            thumbnail: self.thumbnail.clone().unwrap_or_default(),
            video: self.video.clone(),
            eth_address: self.eth_address.clone(),
            collection_id: self.collection_id.map(|c| c.to_string()),
            blockchain_item_id: self.blockchain_item_id.clone(),
            price: self.price.clone(),
            beneficiary: self.beneficiary.clone(),
            rarity: self.rarity.clone(),
            item_type: self.item_type.clone(),
            data: self.data.clone(),
            metrics: self.metrics.clone().unwrap_or(Value::Object(Map::new())),
            utility: self.utility.clone(),
            mappings: self.mappings.clone(),
            contents: self.contents.clone(),
            is_published: self.is_published,
            is_approved: self.is_approved,
            in_catalyst: self.in_catalyst,
            total_supply: self.total_supply,
            content_hash: self.content_hash.clone(),
            local_content_hash: self.local_content_hash.clone(),
            catalyst_content_hash: self.catalyst_content_hash.clone(),
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
        }
    }
}

pub struct ItemsComponent {
    pool: PgPool,
}

impl ItemsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn collection_owner(&self, collection_id: &Uuid) -> Result<Option<String>, ApiError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT lower(eth_address) FROM collections WHERE id = $1")
                .bind(collection_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(addr,)| addr))
    }

    pub async fn collection_by_id(
        &self,
        collection_id: &Uuid,
    ) -> Result<Option<CollectionMetaRow>, ApiError> {
        let row = sqlx::query_as::<_, CollectionMetaRow>(
            "SELECT id, name, eth_address, contract_address, urn_suffix, third_party_id, \
                    is_published, is_approved, created_at, updated_at \
             FROM collections WHERE id = $1",
        )
        .bind(collection_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn set_item_curation_status(
        &self,
        collection_id: &Uuid,
        item_id: &Uuid,
        status: &str,
    ) -> Result<u64, ApiError> {
        let res = sqlx::query(
            r#"
            UPDATE items
               SET curation_status = $3,
                   is_approved = ($3 = 'approved'),
                   updated_at = now()
             WHERE id = $1 AND collection_id = $2
            "#,
        )
        .bind(item_id)
        .bind(collection_id)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn set_items_curation_status(
        &self,
        collection_id: &Uuid,
        item_ids: &[Uuid],
        status: &str,
    ) -> Result<u64, ApiError> {
        let res = sqlx::query(
            r#"
            UPDATE items
               SET curation_status = $3,
                   is_approved = ($3 = 'approved'),
                   updated_at = now()
             WHERE collection_id = $1 AND id = ANY($2)
            "#,
        )
        .bind(collection_id)
        .bind(item_ids)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn items_for_collection(
        &self,
        collection_id: &Uuid,
        q: &ItemQuery,
    ) -> Result<(Vec<ItemRow>, i64), ApiError> {
        let (limit, offset): (Option<i64>, Option<i64>) = match (q.page, q.limit) {
            (Some(page), Some(limit)) if limit > 0 => {
                (Some(limit), Some(limit * (page - 1).max(0)))
            }
            (_, Some(limit)) if limit > 0 => (Some(limit), None),
            _ => (None, None),
        };

        let rows = sqlx::query_as::<_, ItemDbRow>(
            r#"
            SELECT
                i.id, i.urn_suffix, i.name, i.description, i.thumbnail, i.video,
                i.eth_address, i.collection_id, i.blockchain_item_id, i.price,
                i.beneficiary, i.rarity, i.type AS item_type, i.data, i.metrics,
                i.utility, i.mappings, i.is_published, i.is_approved, i.in_catalyst,
                i.total_supply, i.local_content_hash, i.content_hash,
                i.catalyst_content_hash, i.created_at, i.updated_at,
                count(*) OVER() AS total_count
            FROM items i
            WHERE i.collection_id = $1
              AND ($2::text IS NULL OR i.name ILIKE '%' || $2 || '%')
            ORDER BY i.created_at ASC
            LIMIT $3 OFFSET COALESCE($4, 0)
            "#,
        )
        .bind(collection_id)
        .bind(q.name.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total = rows.first().map(|r| r.total_count).unwrap_or(0);

        let mut items = Vec::with_capacity(rows.len());
        for r in rows {
            let content_rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT file, hash FROM item_contents WHERE item_id = $1 ORDER BY file ASC",
            )
            .bind(r.id)
            .fetch_all(&self.pool)
            .await?;
            let contents = content_rows.into_iter().collect::<BTreeMap<_, _>>();
            items.push(r.into_row(contents));
        }
        Ok((items, total))
    }
}

#[derive(sqlx::FromRow)]
struct ItemDbRow {
    id: Uuid,
    urn_suffix: Option<String>,
    name: String,
    description: Option<String>,
    thumbnail: Option<String>,
    video: Option<String>,
    eth_address: String,
    collection_id: Option<Uuid>,
    blockchain_item_id: Option<String>,
    price: Option<String>,
    beneficiary: Option<String>,
    rarity: Option<String>,
    item_type: String,
    data: Value,
    metrics: Option<Value>,
    utility: Option<String>,
    mappings: Option<Value>,
    is_published: bool,
    is_approved: bool,
    in_catalyst: bool,
    total_supply: i64,
    local_content_hash: Option<String>,
    content_hash: Option<String>,
    catalyst_content_hash: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    total_count: i64,
}

impl ItemDbRow {
    fn into_row(self, contents: BTreeMap<String, String>) -> ItemRow {
        ItemRow {
            id: self.id,
            urn_suffix: self.urn_suffix,
            name: self.name,
            description: self.description,
            thumbnail: self.thumbnail,
            video: self.video,
            eth_address: self.eth_address,
            collection_id: self.collection_id,
            blockchain_item_id: self.blockchain_item_id,
            price: self.price,
            beneficiary: self.beneficiary,
            rarity: self.rarity,
            item_type: self.item_type,
            data: self.data,
            metrics: self.metrics,
            utility: self.utility,
            mappings: self.mappings,
            is_published: self.is_published,
            is_approved: self.is_approved,
            in_catalyst: self.in_catalyst,
            total_supply: self.total_supply,
            local_content_hash: self.local_content_hash,
            content_hash: self.content_hash,
            catalyst_content_hash: self.catalyst_content_hash,
            created_at: self.created_at,
            updated_at: self.updated_at,
            contents,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct CollectionMetaRow {
    pub id: Uuid,
    pub name: String,
    pub eth_address: String,
    pub contract_address: Option<String>,
    pub urn_suffix: Option<String>,
    pub third_party_id: Option<String>,
    pub is_published: bool,
    pub is_approved: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct CollectionMetaOut {
    pub id: String,
    pub name: String,
    pub eth_address: String,
    pub contract_address: Option<String>,
    pub urn: Option<String>,
    pub third_party_id: Option<String>,
    pub is_published: bool,
    pub is_approved: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&CollectionMetaRow> for CollectionMetaOut {
    fn from(r: &CollectionMetaRow) -> Self {
        Self {
            id: r.id.to_string(),
            name: r.name.clone(),
            eth_address: r.eth_address.clone(),
            contract_address: r.contract_address.clone(),
            urn: r.urn_suffix.clone(),
            third_party_id: r.third_party_id.clone(),
            is_published: r.is_published,
            is_approved: r.is_approved,
            created_at: r.created_at.timestamp_millis(),
            updated_at: r.updated_at.timestamp_millis(),
        }
    }
}

pub struct NewsletterComponent {
    pool: PgPool,
}

impl NewsletterComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn subscribe(&self, email: &str, source: &str) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO newsletter_subscriptions (email, source, created_at)
             VALUES ($1, $2, now())
             ON CONFLICT (email) DO UPDATE SET source = EXCLUDED.source",
        )
        .bind(email)
        .bind(source)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ts(ms: i64) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(ms).unwrap()
    }

    fn meta_row(with_opts: bool) -> CollectionMetaRow {
        CollectionMetaRow {
            id: Uuid::nil(),
            name: "Hats".into(),
            eth_address: "0xowner".into(),
            contract_address: with_opts.then(|| "0xcontract".into()),
            urn_suffix: with_opts.then(|| "urn:decentraland:matic:collections-v2:0xc0ffee".into()),
            third_party_id: with_opts
                .then(|| "urn:decentraland:matic:collections-thirdparty:tp".into()),
            is_published: true,
            is_approved: false,
            created_at: ts(1_700_000_000_000),
            updated_at: ts(1_700_000_100_000),
        }
    }

    fn old_meta_json(r: &CollectionMetaRow) -> Value {
        json!({
            "id": r.id.to_string(),
            "name": r.name,
            "eth_address": r.eth_address,
            "contract_address": r.contract_address,
            "urn": r.urn_suffix,
            "third_party_id": r.third_party_id,
            "is_published": r.is_published,
            "is_approved": r.is_approved,
            "created_at": r.created_at.timestamp_millis(),
            "updated_at": r.updated_at.timestamp_millis(),
        })
    }

    #[test]
    fn collection_meta_wire_bytes_match_the_old_json_macro() {
        for with_opts in [true, false] {
            let r = meta_row(with_opts);
            assert_eq!(
                serde_json::to_value(CollectionMetaOut::from(&r)).unwrap(),
                old_meta_json(&r),
            );
        }
    }

    fn item_row(with_video: bool) -> ItemRow {
        ItemRow {
            id: Uuid::nil(),
            urn_suffix: Some("urn:decentraland:matic:collections-v2:0xc0ffee:0".into()),
            name: "Cap".into(),
            description: None,
            thumbnail: None,
            video: with_video.then(|| "bafyvideo".into()),
            eth_address: "0xowner".into(),
            collection_id: Some(Uuid::nil()),
            blockchain_item_id: None,
            price: None,
            beneficiary: None,
            rarity: Some("epic".into()),
            item_type: "wearable".into(),
            data: json!({"category": "hat"}),
            metrics: None,
            utility: None,
            mappings: None,
            is_published: true,
            is_approved: false,
            in_catalyst: false,
            total_supply: 0,
            local_content_hash: None,
            content_hash: None,
            catalyst_content_hash: None,
            created_at: ts(1_700_000_000_000),
            updated_at: ts(1_700_000_000_000),
            contents: BTreeMap::from([("male/model.glb".to_string(), "Qm123".to_string())]),
        }
    }

    fn old_full_item(i: &ItemRow) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(i.id.to_string()));
        m.insert(
            "urn".into(),
            i.urn_suffix
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert("name".into(), Value::String(i.name.clone()));
        m.insert(
            "description".into(),
            Value::String(i.description.clone().unwrap_or_default()),
        );
        m.insert(
            "thumbnail".into(),
            Value::String(i.thumbnail.clone().unwrap_or_default()),
        );
        if let Some(v) = &i.video {
            m.insert("video".into(), Value::String(v.clone()));
        }
        m.insert("eth_address".into(), Value::String(i.eth_address.clone()));
        m.insert(
            "collection_id".into(),
            i.collection_id
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::Null),
        );
        m.insert(
            "blockchain_item_id".into(),
            i.blockchain_item_id
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "price".into(),
            i.price.clone().map(Value::String).unwrap_or(Value::Null),
        );
        m.insert(
            "beneficiary".into(),
            i.beneficiary
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "rarity".into(),
            i.rarity.clone().map(Value::String).unwrap_or(Value::Null),
        );
        m.insert("type".into(), Value::String(i.item_type.clone()));
        m.insert("data".into(), i.data.clone());
        m.insert(
            "metrics".into(),
            i.metrics.clone().unwrap_or(Value::Object(Map::new())),
        );
        m.insert(
            "utility".into(),
            i.utility.clone().map(Value::String).unwrap_or(Value::Null),
        );
        m.insert("mappings".into(), i.mappings.clone().unwrap_or(Value::Null));
        let mut contents = Map::new();
        for (k, v) in &i.contents {
            contents.insert(k.clone(), Value::String(v.clone()));
        }
        m.insert("contents".into(), Value::Object(contents));
        m.insert("is_published".into(), Value::Bool(i.is_published));
        m.insert("is_approved".into(), Value::Bool(i.is_approved));
        m.insert("in_catalyst".into(), Value::Bool(i.in_catalyst));
        m.insert("total_supply".into(), Value::Number(i.total_supply.into()));
        m.insert(
            "content_hash".into(),
            i.content_hash
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "local_content_hash".into(),
            i.local_content_hash
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "catalyst_content_hash".into(),
            i.catalyst_content_hash
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "created_at".into(),
            Value::String(i.created_at.to_rfc3339()),
        );
        m.insert(
            "updated_at".into(),
            Value::String(i.updated_at.to_rfc3339()),
        );
        Value::Object(m)
    }

    #[test]
    fn full_item_wire_bytes_match_the_old_json_macro() {
        for with_video in [true, false] {
            let i = item_row(with_video);
            let out = serde_json::to_value(i.to_out()).unwrap();
            assert_eq!(out, old_full_item(&i));
            assert_eq!(out.get("video").is_some(), with_video);
        }
    }
}
