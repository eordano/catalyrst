use alloy::primitives::keccak256;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{
    drafts::{CollectionDraft, ItemDraft},
    items::ItemsComponent,
    linked_publication::LinkedPublicationCheque,
    publication::SNAPSHOT_QUERY,
};
use crate::http::errors::ApiError;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct LinkedPublicationPreparation {
    pub id: String,
    pub revision: String,
    pub name: String,
    pub creator: String,
    pub urn: String,
    pub third_party_id: String,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct LinkedPublicationState {
    pub preparation: LinkedPublicationPreparation,
    pub salt: String,
    pub status: String,
    pub cheque: Option<LinkedPublicationCheque>,
    pub forum_url: Option<String>,
}

pub(super) const STATE_QUERY: &str = "SELECT jsonb_build_object('preparation', p.preparation, \
    'salt', p.salt, 'status', p.status, 'cheque', p.cheque, 'forum_url', p.forum_url) FROM linked_collection_publications p \
    JOIN collections c ON c.id=p.collection_id WHERE c.id=$1 AND lower(c.eth_address)=$2";

pub(super) fn decode(value: Value) -> Result<LinkedPublicationState, ApiError> {
    serde_json::from_value(value)
        .map_err(|e| ApiError::internal(format!("Invalid saved linked publication: {e}")))
}

fn prepare(snapshot: &Value, signer: &str) -> Result<LinkedPublicationPreparation, ApiError> {
    let mut collection = snapshot["collection"].clone();
    collection["urn"] = collection["urn_suffix"].clone();
    let draft: CollectionDraft = serde_json::from_value(collection.clone())
        .map_err(|_| ApiError::bad_request("Invalid collection draft"))?;
    draft.validate(draft.id, signer)?;
    let urn = draft
        .urn
        .ok_or_else(|| ApiError::bad_request("Choose a linked collection"))?;
    let provider = urn.rsplit_once(':').unwrap().0;
    if collection["third_party_id"].as_str() != Some(provider) {
        return Err(ApiError::bad_request(
            "Collection provider does not match its URN",
        ));
    }
    let rows = snapshot["items"]
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 50)
        .ok_or_else(|| ApiError::bad_request("Publish a collection containing 1 to 50 items"))?;
    let mut item_ids = Vec::with_capacity(rows.len());
    for row in rows {
        let item: ItemDraft = serde_json::from_value(row.clone())
            .map_err(|_| ApiError::bad_request("Invalid collection item"))?;
        item.validate(item.id, signer)?;
        if item.collection_id != Some(draft.id)
            || row["missing_content"] == true
            || item.contents.is_empty()
            || item
                .thumbnail
                .as_ref()
                .is_none_or(|name| !item.contents.contains_key(name))
            || item
                .metrics
                .as_ref()
                .and_then(Value::as_object)
                .is_none_or(|metrics| metrics.is_empty())
        {
            return Err(ApiError::bad_request(format!(
                "{}: inspect the model and upload its files and thumbnail before publishing",
                item.name
            )));
        }
        item_ids.push(item.id.to_string());
    }
    Ok(LinkedPublicationPreparation {
        id: draft.id.to_string(),
        revision: catalyrst_hashing::hash_bytes_v1(snapshot.to_string().as_bytes()),
        name: draft.name,
        creator: signer.into(),
        urn: urn.clone(),
        third_party_id: provider.into(),
        item_ids,
    })
}

impl ItemsComponent {
    pub async fn linked_publication_state(
        &self,
        signer: &str,
        id: Uuid,
    ) -> Result<Option<LinkedPublicationState>, ApiError> {
        sqlx::query_scalar::<_, Value>(STATE_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&self.pool)
            .await?
            .map(decode)
            .transpose()
    }

    pub async fn prepare_linked_publication(
        &self,
        signer: &str,
        id: Uuid,
    ) -> Result<LinkedPublicationPreparation, ApiError> {
        if let Some(state) = self.linked_publication_state(signer, id).await? {
            return Ok(state.preparation);
        }
        let snapshot: Value = sqlx::query_scalar(SNAPSHOT_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ApiError::not_found("Collection not found"))?;
        prepare(&snapshot, signer)
    }

    pub async fn begin_linked_publication(
        &self,
        signer: &str,
        id: Uuid,
        revision: &str,
    ) -> Result<LinkedPublicationState, ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_collection(&mut tx, signer, id).await?;
        if let Some(prior) = sqlx::query_scalar::<_, Value>(STATE_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&mut *tx)
            .await?
        {
            let prior = decode(prior)?;
            if prior.preparation.revision != revision {
                return Err(ApiError::http(
                    409,
                    "Another collection revision is already being published",
                ));
            }
            return Ok(prior);
        }
        sqlx::query("SELECT id FROM items WHERE collection_id=$1 ORDER BY id FOR UPDATE")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        let snapshot: Value = sqlx::query_scalar(SNAPSHOT_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_one(&mut *tx)
            .await?;
        if snapshot["collection"]["publication_pending"] == true {
            return Err(ApiError::http(
                409,
                "Collection already has a pending publication",
            ));
        }
        let preparation = prepare(&snapshot, signer)?;
        if preparation.revision != revision {
            return Err(ApiError::http(
                409,
                "Collection changed. Review its items again before publishing.",
            ));
        }
        let salt = format!(
            "{:#x}",
            keccak256(format!("{}{}", Uuid::new_v4(), Uuid::new_v4()))
        );
        sqlx::query("INSERT INTO linked_collection_publications(collection_id,preparation,snapshot,salt,status) VALUES ($1,$2,$3,$4,'prepared')")
            .bind(id).bind(serde_json::to_value(&preparation).map_err(|e| ApiError::internal(e.to_string()))?).bind(snapshot).bind(&salt).execute(&mut *tx).await?;
        sqlx::query("UPDATE collections SET publication_pending=true WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(LinkedPublicationState {
            preparation,
            salt,
            status: "prepared".into(),
            cheque: None,
            forum_url: None,
        })
    }

    pub async fn claim_linked_publication(
        &self,
        signer: &str,
        id: Uuid,
        revision: &str,
    ) -> Result<LinkedPublicationState, ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_collection(&mut tx, signer, id).await?;
        let prior = sqlx::query_scalar::<_, Value>(STATE_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::http(409, "Prepare this publication before signing"))?;
        let mut state = decode(prior)?;
        if state.preparation.revision != revision {
            return Err(ApiError::http(409, "Publication revision changed"));
        }
        if state.status == "prepared" {
            sqlx::query("UPDATE linked_collection_publications SET status='signing',updated_at=now() WHERE collection_id=$1")
                .bind(id).execute(&mut *tx).await?;
            state.status = "signing".into();
        }
        tx.commit().await?;
        Ok(state)
    }

    pub async fn authorize_linked_publication(
        &self,
        signer: &str,
        id: Uuid,
        cheque: &LinkedPublicationCheque,
    ) -> Result<LinkedPublicationState, ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_collection(&mut tx, signer, id).await?;
        let prior = sqlx::query_scalar::<_, Value>(STATE_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                ApiError::http(409, "Prepare this publication before authorizing slots")
            })?;
        let mut state = decode(prior)?;
        cheque.verify(
            signer,
            &state.preparation.third_party_id,
            state.preparation.item_ids.len() as u32,
            &state.salt,
        )?;
        if let Some(existing) = &state.cheque {
            if existing != cheque {
                return Err(ApiError::http(
                    409,
                    "This publication already has a saved authorization",
                ));
            }
            return Ok(state);
        }
        if state.status != "signing" {
            return Err(ApiError::http(
                409,
                "Claim the publication before signing its authorization",
            ));
        }
        sqlx::query("UPDATE linked_collection_publications SET cheque=$2,status='authorized',updated_at=now() WHERE collection_id=$1")
            .bind(id).bind(serde_json::to_value(cheque).map_err(|e| ApiError::internal(e.to_string()))?).execute(&mut *tx).await?;
        state.cheque = Some(cheque.clone());
        state.status = "authorized".into();
        tx.commit().await?;
        Ok(state)
    }

    pub async fn cancel_linked_publication(&self, signer: &str, id: Uuid) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_collection(&mut tx, signer, id).await?;
        let prior = sqlx::query_scalar::<_, Value>(STATE_QUERY)
            .bind(id)
            .bind(signer)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(prior) = prior {
            if decode(prior)?.status != "prepared" {
                return Err(ApiError::http(409, "A slot authorization may exist. Resume this publication before making changes."));
            }
            sqlx::query("DELETE FROM linked_collection_publications WHERE collection_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE collections SET publication_pending=false WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

pub(super) async fn lock_collection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    signer: &str,
    id: Uuid,
) -> Result<(), ApiError> {
    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM collections WHERE id=$1 AND lower(eth_address)=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(signer)
    .fetch_optional(&mut **tx)
    .await?;
    owner.ok_or_else(|| ApiError::not_found("Collection not found"))?;
    Ok(())
}
