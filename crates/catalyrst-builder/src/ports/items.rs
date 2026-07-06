use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
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

impl ItemRow {
    pub fn to_full_item(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(self.id.to_string()));
        m.insert(
            "urn".into(),
            self.urn_suffix
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert("name".into(), Value::String(self.name.clone()));
        m.insert("description".into(), opt_str(self.description.clone(), ""));
        m.insert("thumbnail".into(), opt_str(self.thumbnail.clone(), ""));
        if let Some(v) = &self.video {
            m.insert("video".into(), Value::String(v.clone()));
        }
        m.insert(
            "eth_address".into(),
            Value::String(self.eth_address.clone()),
        );
        m.insert(
            "collection_id".into(),
            self.collection_id
                .map(|c| Value::String(c.to_string()))
                .unwrap_or(Value::Null),
        );
        m.insert(
            "blockchain_item_id".into(),
            opt_null(self.blockchain_item_id.clone()),
        );
        m.insert("price".into(), opt_null(self.price.clone()));
        m.insert("beneficiary".into(), opt_null(self.beneficiary.clone()));
        m.insert("rarity".into(), opt_null(self.rarity.clone()));
        m.insert("type".into(), Value::String(self.item_type.clone()));
        m.insert("data".into(), self.data.clone());
        m.insert(
            "metrics".into(),
            self.metrics.clone().unwrap_or(Value::Object(Map::new())),
        );
        m.insert("utility".into(), opt_null(self.utility.clone()));
        m.insert(
            "mappings".into(),
            self.mappings.clone().unwrap_or(Value::Null),
        );
        let mut contents = Map::new();
        for (k, v) in &self.contents {
            contents.insert(k.clone(), Value::String(v.clone()));
        }
        m.insert("contents".into(), Value::Object(contents));
        m.insert("is_published".into(), Value::Bool(self.is_published));
        m.insert("is_approved".into(), Value::Bool(self.is_approved));
        m.insert("in_catalyst".into(), Value::Bool(self.in_catalyst));
        m.insert(
            "total_supply".into(),
            Value::Number(self.total_supply.into()),
        );
        m.insert("content_hash".into(), opt_null(self.content_hash.clone()));
        m.insert(
            "local_content_hash".into(),
            opt_null(self.local_content_hash.clone()),
        );
        m.insert(
            "catalyst_content_hash".into(),
            opt_null(self.catalyst_content_hash.clone()),
        );
        m.insert(
            "created_at".into(),
            Value::String(self.created_at.to_rfc3339()),
        );
        m.insert(
            "updated_at".into(),
            Value::String(self.updated_at.to_rfc3339()),
        );
        Value::Object(m)
    }
}

fn opt_null(v: Option<String>) -> Value {
    v.map(Value::String).unwrap_or(Value::Null)
}

fn opt_str(v: Option<String>, default: &str) -> Value {
    Value::String(v.unwrap_or_else(|| default.to_string()))
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

    pub async fn item_in_collection(
        &self,
        collection_id: &Uuid,
        item_id: &Uuid,
    ) -> Result<bool, ApiError> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM items WHERE id = $1 AND collection_id = $2")
                .bind(item_id)
                .bind(collection_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.is_some())
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

impl CollectionMetaRow {
    pub fn to_meta_json(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "name": self.name,
            "eth_address": self.eth_address,
            "contract_address": self.contract_address,
            "urn": self.urn_suffix,
            "third_party_id": self.third_party_id,
            "is_published": self.is_published,
            "is_approved": self.is_approved,
            "created_at": self.created_at.timestamp_millis(),
            "updated_at": self.updated_at.timestamp_millis(),
        })
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
