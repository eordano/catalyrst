use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use bytes::{Bytes, BytesMut};
use serde_json::{json, Value};

use crate::errors::{AppError, InvalidRequestError};
use crate::extractors::MultipartBody;
use crate::state::AppState;

const MAX_DEPLOY_FILES: usize = 1000;

const MAX_AUTH_CHAIN_LENGTH: i64 = 10;

const MAX_DEPLOY_FILE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEntityRequest {
    pub entity_id: String,

    #[serde(default)]
    pub auth_chain: Option<Value>,

    #[serde(default)]
    pub eth_address: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

pub(crate) fn extract_auth_chain_from_fields(
    fields: &BTreeMap<String, String>,
) -> Result<Option<Value>, AppError> {
    if let Some(chain_str) = fields.get("authChain") {
        let chain: Value = serde_json::from_str(chain_str)
            .map_err(|_| InvalidRequestError::new("Invalid auth chain"))?;
        if !chain.is_array() {
            return Err(InvalidRequestError::new("Invalid auth chain").into());
        }
        if let Some(arr) = chain.as_array() {
            if arr.len() > MAX_AUTH_CHAIN_LENGTH as usize {
                return Err(InvalidRequestError::new(format!(
                    "Auth chain is too long; the maximum allowed is {MAX_AUTH_CHAIN_LENGTH} elements"
                ))
                .into());
            }
        }
        return Ok(Some(chain));
    }

    let mut biggest_index: i64 = -1;
    let re_prefix = "authChain[";
    for key in fields.keys() {
        if key.starts_with(re_prefix) {
            if let Some(rest) = key.strip_prefix(re_prefix) {
                if let Some(idx_str) = rest.split(']').next() {
                    if let Ok(idx) = idx_str.parse::<i64>() {
                        if idx > biggest_index {
                            biggest_index = idx;
                        }
                    }
                }
            }
        }
    }

    if biggest_index == -1 {
        return Ok(None);
    }

    if biggest_index >= MAX_AUTH_CHAIN_LENGTH {
        return Err(InvalidRequestError::new(format!(
            "Auth chain is too long; the maximum allowed is {MAX_AUTH_CHAIN_LENGTH} elements"
        ))
        .into());
    }

    let mut chain = Vec::new();
    for i in 0..=biggest_index {
        let payload_key = format!("authChain[{i}][payload]");
        let signature_key = format!("authChain[{i}][signature]");
        let type_key = format!("authChain[{i}][type]");

        let payload = fields.get(&payload_key).ok_or_else(|| {
            InvalidRequestError::new(format!("Missing auth chain element at index {i}"))
        })?;
        let signature = fields.get(&signature_key).ok_or_else(|| {
            InvalidRequestError::new(format!("Missing auth chain element at index {i}"))
        })?;
        let link_type = fields.get(&type_key).ok_or_else(|| {
            InvalidRequestError::new(format!("Missing auth chain element at index {i}"))
        })?;

        chain.push(json!({
            "type": link_type,
            "payload": payload,
            "signature": signature,
        }));
    }

    Ok(Some(Value::Array(chain)))
}

pub async fn create_entity_multipart(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    MultipartBody(mut multipart): MultipartBody,
) -> Result<impl IntoResponse, AppError> {
    let sync_state = state.synchronization_state.get_state();
    if sync_state == "Bootstrapping" {
        return Err(AppError::ServiceUnavailable(
            "Deployments are not allowed while the Catalyst is bootstrapping".to_string(),
        ));
    }

    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut files: Vec<Bytes> = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| InvalidRequestError::new(format!("Failed to read multipart field: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();

        if field.file_name().is_some() {
            if files.len() >= MAX_DEPLOY_FILES {
                return Err(InvalidRequestError::new(format!(
                    "deployment exceeds maximum of {MAX_DEPLOY_FILES} files"
                ))
                .into());
            }
            let field_name = name.clone();
            let mut buf = BytesMut::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| InvalidRequestError::new(format!("Failed to read file data: {e}")))?
            {
                if buf.len().saturating_add(chunk.len()) > MAX_DEPLOY_FILE_BYTES {
                    return Err(InvalidRequestError::new(format!(
                        "file {field_name} exceeds {MAX_DEPLOY_FILE_BYTES} bytes"
                    ))
                    .into());
                }
                buf.extend_from_slice(&chunk);
            }
            files.push(buf.freeze());
        } else {
            let value = field.text().await.map_err(|e| {
                InvalidRequestError::new(format!("Failed to read field value: {e}"))
            })?;
            fields.insert(name, value);
        }
    }

    if files.len() > MAX_DEPLOY_FILES {
        return Err(InvalidRequestError::new(format!(
            "deployment exceeds maximum of {MAX_DEPLOY_FILES} files"
        ))
        .into());
    }

    let entity_id = fields
        .get("entityId")
        .ok_or_else(|| InvalidRequestError::new("Missing entityId field"))?
        .clone();

    let auth_chain = extract_auth_chain_from_fields(&fields)?;

    let auth_chain = if let Some(chain) = auth_chain {
        if let Some(arr) = chain.as_array() {
            for link in arr {
                if link.get("type").and_then(|v| v.as_str()).is_none() {
                    return Err(InvalidRequestError::new(
                        "invalid auth chain format: each link must have a \"type\" field",
                    )
                    .into());
                }
                if link.get("payload").and_then(|v| v.as_str()).is_none() {
                    return Err(InvalidRequestError::new(
                        "invalid auth chain format: each link must have a \"payload\" field",
                    )
                    .into());
                }
            }
        }
        chain
    } else {
        let eth_address = fields.get("ethAddress");
        let signature = fields.get("signature");
        if let (Some(addr), Some(sig)) = (eth_address, signature) {
            json!([
                { "type": "SIGNER", "payload": addr, "signature": Value::Null },
                { "type": "ECDSA_SIGNED_ENTITY", "payload": entity_id, "signature": sig }
            ])
        } else {
            return Err(InvalidRequestError::new("No auth chain can be derived").into());
        }
    };

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    tracing::info!(
        entity_id = %entity_id,
        user_agent = %user_agent,
        file_count = files.len(),
        "POST /entities - Deploying entity (multipart)"
    );

    match state
        .deployer
        .deploy_entity(files, &entity_id, auth_chain, "LOCAL")
        .await
    {
        Ok(creation_timestamp) => {
            tracing::info!(
                entity_id = %entity_id,
                "POST /entities - Deployment successful"
            );
            Ok((
                StatusCode::OK,
                Json(json!({ "creationTimestamp": creation_timestamp })),
            ))
        }
        Err(errors) => {
            tracing::error!(
                entity_id = %entity_id,
                errors = ?errors,
                "POST /entities - Deployment failed"
            );
            Ok((StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))))
        }
    }
}

pub async fn create_entity(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateEntityRequest>,
) -> Result<impl IntoResponse, AppError> {
    let sync_state = state.synchronization_state.get_state();
    if sync_state == "Bootstrapping" {
        return Err(AppError::ServiceUnavailable(
            "Deployments are not allowed while the Catalyst is bootstrapping".to_string(),
        ));
    }

    let auth_chain = if let Some(chain) = body.auth_chain {
        if !chain.is_array() {
            return Err(InvalidRequestError::new("Invalid auth chain").into());
        }
        let links: Vec<Value> = serde_json::from_value(chain.clone())
            .map_err(|_| AppError::from(InvalidRequestError::new("invalid auth chain format")))?;
        for link in &links {
            if link.get("type").and_then(|v| v.as_str()).is_none() {
                return Err(InvalidRequestError::new(
                    "invalid auth chain format: each link must have a \"type\" field",
                )
                .into());
            }
            if link.get("payload").and_then(|v| v.as_str()).is_none() {
                return Err(InvalidRequestError::new(
                    "invalid auth chain format: each link must have a \"payload\" field",
                )
                .into());
            }
        }
        chain
    } else if let (Some(addr), Some(sig)) = (&body.eth_address, &body.signature) {
        json!([
            { "type": "SIGNER", "payload": addr, "signature": Value::Null },
            { "type": "ECDSA_SIGNED_ENTITY", "payload": body.entity_id, "signature": sig }
        ])
    } else {
        return Err(InvalidRequestError::new("No auth chain can be derived").into());
    };

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    tracing::info!(
        entity_id = %body.entity_id,
        user_agent = %user_agent,
        "POST /entities - Deploying entity"
    );

    let files: Vec<Bytes> = vec![];

    match state
        .deployer
        .deploy_entity(files, &body.entity_id, auth_chain, "LOCAL")
        .await
    {
        Ok(creation_timestamp) => {
            tracing::info!(
                entity_id = %body.entity_id,
                "POST /entities - Deployment successful"
            );
            Ok((
                StatusCode::OK,
                Json(json!({ "creationTimestamp": creation_timestamp })),
            ))
        }
        Err(errors) => {
            tracing::error!(
                entity_id = %body.entity_id,
                errors = ?errors,
                "POST /entities - Deployment failed"
            );
            Ok((StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn extract_auth_chain_from_fields_parses_indexed_keys() {
        let f = fields(&[
            ("authChain[0][type]", "SIGNER"),
            ("authChain[0][payload]", "0xabc"),
            ("authChain[0][signature]", ""),
            ("authChain[1][type]", "ECDSA_SIGNED_ENTITY"),
            ("authChain[1][payload]", "QmEntity"),
            ("authChain[1][signature]", "0xdeadbeef"),
        ]);
        let out = extract_auth_chain_from_fields(&f).expect("parser ok");
        let arr = out.expect("Some(array)");
        let arr = arr.as_array().expect("is array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "SIGNER");
        assert_eq!(arr[0]["payload"], "0xabc");
        assert_eq!(arr[0]["signature"], "");
        assert_eq!(arr[1]["type"], "ECDSA_SIGNED_ENTITY");
        assert_eq!(arr[1]["payload"], "QmEntity");
        assert_eq!(arr[1]["signature"], "0xdeadbeef");
    }

    #[test]
    fn extract_auth_chain_rejects_mixed_keys() {
        let f = fields(&[
            ("authChain[0][type]", "SIGNER"),
            ("authChain[ABC][type]", "SIGNER"),
            ("authChain[ABC][payload]", "0xabc"),
            ("authChain[ABC][signature]", ""),
        ]);
        let err = extract_auth_chain_from_fields(&f)
            .expect_err("missing payload/signature at index 0 must error");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("missing auth chain element"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn extract_auth_chain_caps_at_max_links() {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..=100 {
            pairs.push((format!("authChain[{i}][type]"), "SIGNER".into()));
            pairs.push((format!("authChain[{i}][payload]"), format!("0x{i:040x}")));
            pairs.push((format!("authChain[{i}][signature]"), "".into()));
        }
        let f: BTreeMap<String, String> = pairs.into_iter().collect();
        let err = extract_auth_chain_from_fields(&f)
            .expect_err("index loop must reject more than the max allowed links");
        assert!(
            format!("{err}").contains("Auth chain is too long"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extract_auth_chain_json_caps_at_max_links() {
        let links: Vec<Value> = (0..50)
            .map(|i| json!({ "type": "SIGNER", "payload": format!("0x{i}"), "signature": "" }))
            .collect();
        let chain = serde_json::to_string(&Value::Array(links)).unwrap();
        let f = fields(&[("authChain", chain.as_str())]);
        let err = extract_auth_chain_from_fields(&f)
            .expect_err("json array longer than the max must be rejected");
        assert!(
            format!("{err}").contains("Auth chain is too long"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extract_auth_chain_empty_returns_empty_vec() {
        let f = fields(&[("entityId", "Qm123"), ("someOtherField", "x")]);
        let out = extract_auth_chain_from_fields(&f).expect("parser ok");
        assert!(
            out.is_none(),
            "expected Ok(None) when no authChain keys present"
        );
    }

    #[test]
    fn extract_auth_chain_from_json_string_works() {
        let f = fields(&[(
            "authChain",
            r#"[{"type":"SIGNER","payload":"0x1","signature":""}]"#,
        )]);
        let out = extract_auth_chain_from_fields(&f).expect("parser ok");
        let arr = out.expect("Some(array)");
        let arr = arr.as_array().expect("is array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "SIGNER");
    }
}
