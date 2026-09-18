use std::{collections::BTreeMap, time::Duration};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::{
    items::ItemsComponent,
    linked_publication_store::{decode, lock_collection, LinkedPublicationState, STATE_QUERY},
};
use crate::{auth_chain::require_signer, http::errors::ApiError};

const FOUNDATION: &str = "https://builder-api.decentraland.org";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
pub struct FoundationReviewSignatures {
    pub collection: BTreeMap<String, String>,
    pub items: BTreeMap<String, String>,
    pub curations: BTreeMap<String, String>,
}

pub struct FoundationLinkedReview {
    client: reqwest::Client,
    base: String,
}

impl FoundationLinkedReview {
    pub fn new() -> Result<Self, ApiError> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|e| ApiError::internal(e.to_string()))?,
            base: FOUNDATION.into(),
        })
    }

    async fn fetch(
        &self,
        owner: &str,
        path: &str,
        signed: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, ApiError> {
        let headers = signed_headers(owner, path, signed).await?;
        let mut response = self.client.get(format!("{}{path}", self.base)).headers(headers).send().await
            .map_err(|_| ApiError::service_unavailable("Foundation review verification is unavailable. Retry to check the same submission."))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(ApiError::http(
                502,
                "Foundation could not verify this submission. Check provider access and retry.",
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE as u64)
        {
            return Err(ApiError::http(
                502,
                "Foundation review response is too large",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ApiError::http(502, "Foundation review response was interrupted"))?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Err(ApiError::http(
                    502,
                    "Foundation review response is too large",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::http(502, "Invalid Foundation review response"))?;
        if body["ok"] == false || !body.as_object().is_some_and(|obj| obj.contains_key("data")) {
            return Err(ApiError::http(502, "Invalid Foundation review response"));
        }
        Ok(Some(body["data"].clone()))
    }

    pub async fn verify(
        &self,
        store: &ItemsComponent,
        owner: &str,
        id: Uuid,
        signatures: &FoundationReviewSignatures,
    ) -> Result<LinkedPublicationState, ApiError> {
        let state = store
            .linked_publication_state(owner, id)
            .await?
            .ok_or_else(|| ApiError::not_found("Linked publication not found"))?;
        if state.status == "submitted" {
            return Ok(state);
        }
        if state.status != "authorized" {
            return Err(ApiError::http(
                409,
                "Save the slot authorization before checking submission",
            ));
        }
        let snapshot: Value = sqlx::query_scalar(
            "SELECT snapshot FROM linked_collection_publications WHERE collection_id=$1",
        )
        .bind(id)
        .fetch_one(&store.pool)
        .await?;
        let path = format!("/v1/collections/{id}");
        let items_path = format!("{path}/items");
        let curations_path = format!("{path}/itemCurations");
        let (collection, items, curations) = tokio::try_join!(
            self.fetch(owner, &path, &signatures.collection),
            self.fetch(owner, &items_path, &signatures.items),
            self.fetch(owner, &curations_path, &signatures.curations),
        )?;
        let (Some(collection), Some(items), Some(curations)) = (collection, items, curations)
        else {
            return Ok(state);
        };
        let Some(review) = verify_review(&snapshot, &collection, &items, &curations)? else {
            return Ok(state);
        };
        store
            .finish_linked_publication(owner, id, &state.preparation.revision, review)
            .await
    }
}

async fn signed_headers(
    owner: &str,
    path: &str,
    signed: &BTreeMap<String, String>,
) -> Result<HeaderMap, ApiError> {
    if signed.len() > crate::auth_chain::MAX_AUTH_CHAIN_LINKS + 2 {
        return Err(ApiError::bad_request(
            "Too many Foundation signature headers",
        ));
    }
    let mut headers = HeaderMap::new();
    for (name, value) in signed {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ApiError::bad_request("Invalid Foundation signature header"))?;
        let allowed = matches!(
            name.as_str(),
            "x-identity-timestamp" | "x-identity-metadata"
        ) || name
            .as_str()
            .strip_prefix("x-identity-auth-chain-")
            .is_some_and(|index| !index.is_empty() && index.bytes().all(|c| c.is_ascii_digit()));
        if !allowed || value.len() > 8192 {
            return Err(ApiError::bad_request("Invalid Foundation signature header"));
        }
        let value = HeaderValue::from_str(value)
            .map_err(|_| ApiError::bad_request("Invalid Foundation signature header"))?;
        if headers.insert(name, value).is_some() {
            return Err(ApiError::bad_request(
                "Duplicate Foundation signature header",
            ));
        }
    }
    let signer = require_signer(&headers, "get", path)
        .await
        .map_err(|error| {
            let (status, message) = error.http_status_and_message();
            ApiError::http(status, message)
        })?;
    if !signer.as_str().eq_ignore_ascii_case(owner) {
        return Err(ApiError::forbidden(
            "Foundation requests must be signed by the collection owner",
        ));
    }
    Ok(headers)
}

struct ReviewedItem {
    id: Uuid,
    urn: String,
    content_hash: String,
    status: String,
    approved: bool,
}
struct VerifiedReview {
    items: Vec<ReviewedItem>,
    forum_url: Option<String>,
}

fn verify_review(
    snapshot: &Value,
    collection: &Value,
    items: &Value,
    curations: &Value,
) -> Result<Option<VerifiedReview>, ApiError> {
    let mismatch = || {
        ApiError::http(
            409,
            "Foundation's submission does not match the reviewed collection and content",
        )
    };
    let local = &snapshot["collection"];
    let owner = local["eth_address"].as_str().ok_or_else(mismatch)?;
    let urn = local["urn_suffix"].as_str().ok_or_else(mismatch)?;
    if collection["id"] != local["id"]
        || collection["name"] != local["name"]
        || collection["urn"] != urn
        || !collection["contract_address"].is_null()
        || !collection["eth_address"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(owner))
    {
        return Err(mismatch());
    }
    let expected = snapshot["items"].as_array().ok_or_else(mismatch)?;
    let items = items.as_array().ok_or_else(mismatch)?;
    let curations = curations.as_array().ok_or_else(mismatch)?;
    if expected.is_empty() || items.len() != expected.len() {
        return Err(mismatch());
    }
    let mut reviewed = Vec::with_capacity(expected.len());
    for saved in expected {
        let id = saved["id"].as_str().ok_or_else(mismatch)?;
        let matching: Vec<_> = items.iter().filter(|item| item["id"] == id).collect();
        if matching.len() != 1 {
            return Err(mismatch());
        }
        let item = matching[0];
        let item_urn = format!("{urn}:{id}");
        if item["urn"] != item_urn
            || item["collection_id"] != local["id"]
            || !item["eth_address"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(owner))
            || ["name", "type", "thumbnail", "data", "metrics", "contents"]
                .iter()
                .any(|key| item[*key] != saved[*key])
            || item["description"].as_str().unwrap_or("")
                != saved["description"].as_str().unwrap_or("")
        {
            return Err(mismatch());
        }
        if curations.is_empty() {
            continue;
        }
        let matching: Vec<_> = curations
            .iter()
            .filter(|curation| curation["item_id"] == id)
            .collect();
        if matching.len() != 1 {
            return Err(mismatch());
        }
        let curation = matching[0];
        let hash = item["local_content_hash"]
            .as_str()
            .filter(|h| !h.is_empty())
            .ok_or_else(mismatch)?;
        let status = curation["status"]
            .as_str()
            .filter(|s| matches!(*s, "pending" | "approved" | "rejected"))
            .ok_or_else(mismatch)?;
        if curation["content_hash"] != hash || item["is_published"] != true {
            return Err(mismatch());
        }
        reviewed.push(ReviewedItem {
            id: id.parse().map_err(|_| mismatch())?,
            urn: item_urn,
            content_hash: hash.into(),
            status: status.into(),
            approved: item["is_approved"] == true && status == "approved",
        });
    }
    if curations.is_empty() {
        if collection["is_published"] == true
            || items.iter().any(|item| item["is_published"] == true)
        {
            return Err(mismatch());
        }
        return Ok(None);
    }
    if curations.len() != expected.len() || collection["is_published"] != true {
        return Err(mismatch());
    }
    let forum_url = match collection["forum_link"]
        .as_str()
        .filter(|url| !url.is_empty())
    {
        None => None,
        Some(raw) => {
            let url = reqwest::Url::parse(raw).map_err(|_| mismatch())?;
            if url.scheme() != "https"
                || url.host_str() != Some("forum.decentraland.org")
                || !url.path().starts_with("/t/")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port_or_known_default() != Some(443)
            {
                return Err(mismatch());
            }
            Some(url.to_string())
        }
    };
    Ok(Some(VerifiedReview {
        items: reviewed,
        forum_url,
    }))
}

impl ItemsComponent {
    async fn finish_linked_publication(
        &self,
        owner: &str,
        id: Uuid,
        revision: &str,
        review: VerifiedReview,
    ) -> Result<LinkedPublicationState, ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_collection(&mut tx, owner, id).await?;
        let value: Value = sqlx::query_scalar(STATE_QUERY)
            .bind(id)
            .bind(owner)
            .fetch_one(&mut *tx)
            .await?;
        let mut state = decode(value)?;
        if state.preparation.revision != revision {
            return Err(ApiError::http(409, "Linked publication revision changed"));
        }
        if state.status == "submitted" {
            return Ok(state);
        }
        if state.status != "authorized" || state.cheque.is_none() {
            return Err(ApiError::http(409, "Linked publication is not authorized"));
        }
        let approved = review.items.iter().all(|item| item.approved);
        for item in review.items {
            let changed = sqlx::query("UPDATE items SET is_published=true,urn_suffix=$2,blockchain_item_id=$3,local_content_hash=$4,curation_status=$5,is_approved=$6,price='0',beneficiary='0x0000000000000000000000000000000000000000',rarity=NULL,total_supply=0,updated_at=now() \
                WHERE id=$1 AND collection_id=$7 AND lower(eth_address)=$8 AND NOT is_published")
                .bind(item.id).bind(item.urn).bind(item.id.to_string()).bind(item.content_hash).bind(item.status).bind(item.approved).bind(id).bind(owner)
                .execute(&mut *tx).await?.rows_affected();
            if changed != 1 {
                return Err(ApiError::http(
                    409,
                    "Linked items changed during publication",
                ));
            }
        }
        sqlx::query("UPDATE collections SET is_published=true,is_approved=$2,publication_pending=false,updated_at=now() WHERE id=$1").bind(id).bind(approved).execute(&mut *tx).await?;
        sqlx::query("UPDATE linked_collection_publications SET status='submitted',forum_url=$2,updated_at=now() WHERE collection_id=$1")
            .bind(id).bind(&review.forum_url).execute(&mut *tx).await?;
        state.status = "submitted".into();
        state.forum_url = review.forum_url;
        tx.commit().await?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests;
