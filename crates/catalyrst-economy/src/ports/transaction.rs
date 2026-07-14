use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::chrono::NaiveDateTime;
use sqlx::PgPool;

use crate::admin::{RuntimeConfig, SignerPreference};
use crate::config::Config;
use crate::http::errors::ApiError;
use crate::ports::abi::{self, SaleKind};
use crate::ports::contracts::ContractsComponent;
use crate::ports::contracts_addrs::DclContracts;
use crate::ports::relayer::Relayer;
use crate::ports::signer::DirectSigner;
use crate::ports::upstream::UpstreamForwarder;

#[derive(Debug, Clone)]
pub struct TransactionData {
    pub from: String,
    pub params: Vec<String>,
}

pub fn parse_send_transaction_request(body: &[u8]) -> Result<TransactionData, ApiError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|e| ApiError::MalformedBody(e.to_string()))?;
    let transaction_data = match value.get("transactionData") {
        Some(v) if !v.is_null() => v,
        _ => {
            return Err(ApiError::MissingTransactionData(
                "Missing transaction data. Please add it to the body of the request as `transactionData`".into(),
            ));
        }
    };
    validate_transaction_data(transaction_data)
}

const ADDRESS_PATTERN: &str = "^0x[a-fA-F0-9]{40}$";
const CALLDATA_PATTERN: &str = "^0x[a-fA-F0-9]+$";
const CALLDATA_MAX_LENGTH: usize = 200_002;

fn schema_error(
    instance_path: &str,
    schema_path: &str,
    keyword: &str,
    params: Value,
    message: &str,
) -> ApiError {
    let s = |v: &str| serde_json::to_string(v).unwrap_or_else(|_| "\"\"".to_string());
    let error_object = format!(
        "{{\"instancePath\":{},\"schemaPath\":{},\"keyword\":{},\"params\":{},\"message\":{}}}",
        s(instance_path),
        s(schema_path),
        s(keyword),
        serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_string()),
        s(message)
    );
    ApiError::InvalidSchema(format!(
        "Invalid transaction data. Errors: [{error_object}]"
    ))
}

fn validate_transaction_data(data: &Value) -> Result<TransactionData, ApiError> {
    let obj = match data {
        Value::Object(map) => map,
        _ => {
            return Err(schema_error(
                "",
                "#/type",
                "type",
                json!({ "type": "object" }),
                "must be object",
            ));
        }
    };

    for required in ["from", "params"] {
        if !obj.contains_key(required) {
            return Err(schema_error(
                "",
                "#/required",
                "required",
                json!({ "missingProperty": required }),
                &format!("must have required property '{required}'"),
            ));
        }
    }

    if let Some(extra) = obj
        .keys()
        .find(|k| k.as_str() != "from" && k.as_str() != "params")
    {
        return Err(schema_error(
            "",
            "#/additionalProperties",
            "additionalProperties",
            json!({ "additionalProperty": extra }),
            "must NOT have additional properties",
        ));
    }

    let from = match obj.get("from") {
        Some(Value::String(s)) => s.clone(),
        _ => {
            return Err(schema_error(
                "/from",
                "#/properties/from/type",
                "type",
                json!({ "type": "string" }),
                "must be string",
            ));
        }
    };

    if !is_hex_address(&from) {
        return Err(schema_error(
            "/from",
            "#/properties/from/pattern",
            "pattern",
            json!({ "pattern": ADDRESS_PATTERN }),
            &format!("must match pattern \"{ADDRESS_PATTERN}\""),
        ));
    }

    let arr = match obj.get("params") {
        Some(Value::Array(a)) => a,
        _ => {
            return Err(schema_error(
                "/params",
                "#/properties/params/type",
                "type",
                json!({ "type": "array" }),
                "must be array",
            ));
        }
    };

    if arr.len() < 2 {
        return Err(schema_error(
            "/params",
            "#/properties/params/minItems",
            "minItems",
            json!({ "limit": 2 }),
            "must NOT have fewer than 2 items",
        ));
    }
    if arr.len() > 2 {
        return Err(schema_error(
            "/params",
            "#/properties/params/maxItems",
            "maxItems",
            json!({ "limit": 2 }),
            "must NOT have more than 2 items",
        ));
    }

    let mut params = Vec::with_capacity(2);
    for (i, item) in arr.iter().enumerate() {
        let s = match item {
            Value::String(s) => s,
            _ => {
                return Err(schema_error(
                    &format!("/params/{i}"),
                    &format!("#/properties/params/items/{i}/type"),
                    "type",
                    json!({ "type": "string" }),
                    "must be string",
                ));
            }
        };

        if i == 1 && s.chars().count() > CALLDATA_MAX_LENGTH {
            return Err(schema_error(
                "/params/1",
                "#/properties/params/items/1/maxLength",
                "maxLength",
                json!({ "limit": CALLDATA_MAX_LENGTH }),
                &format!("must NOT have more than {CALLDATA_MAX_LENGTH} characters"),
            ));
        }

        let (matches, pattern) = if i == 0 {
            (is_hex_address(s), ADDRESS_PATTERN)
        } else {
            (is_hex_data(s), CALLDATA_PATTERN)
        };
        if !matches {
            return Err(schema_error(
                &format!("/params/{i}"),
                &format!("#/properties/params/items/{i}/pattern"),
                "pattern",
                json!({ "pattern": pattern }),
                &format!("must match pattern \"{pattern}\""),
            ));
        }

        params.push(s.clone());
    }

    Ok(TransactionData { from, params })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaTxSender(String);

impl MetaTxSender {
    pub fn from_meta_tx_calldata(from: &str, calldata: &str) -> Result<Self, ApiError> {
        let data = hex_to_bytes(calldata).ok_or_else(|| {
            ApiError::InvalidTransaction(
                "Invalid transaction data. The calldata is not a hex byte string.".into(),
            )
        })?;
        let signed_user = abi::decode_meta_tx_user_address(&data).ok_or_else(|| {
            ApiError::InvalidTransaction(
                "Invalid transaction data. The executeMetaTransaction calldata could not be \
                 decoded, so the userAddress it was signed for cannot be established."
                    .into(),
            )
        })?;
        let claimed = from.trim().parse::<Address>().map_err(|_| {
            ApiError::InvalidTransaction(format!("Invalid from address. Received: {from}"))
        })?;
        if signed_user != claimed {
            return Err(ApiError::InvalidTransaction(format!(
                "The from address does not match the userAddress signed into the \
                 executeMetaTransaction calldata. from: {claimed} - userAddress: {signed_user}"
            )));
        }
        Ok(Self(format!("{signed_user:#x}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationDisposition {
    Release,
    Keep,
}

pub fn reservation_disposition(err: &ApiError) -> ReservationDisposition {
    if is_post_broadcast(err) {
        return ReservationDisposition::Keep;
    }
    if is_pre_broadcast(err) {
        ReservationDisposition::Release
    } else {
        ReservationDisposition::Keep
    }
}

fn is_post_broadcast(err: &ApiError) -> bool {
    matches!(
        err,
        ApiError::RelayReverted(_) | ApiError::RelayerTimeout(_)
    )
}

fn is_pre_broadcast(err: &ApiError) -> bool {
    matches!(
        err,
        ApiError::InvalidSchema(_)
            | ApiError::InvalidSalePrice(_)
            | ApiError::InvalidContractAddress(_)
            | ApiError::InvalidTransaction(_)
            | ApiError::HighCongestion(_)
            | ApiError::RelayerFailed(_)
            | ApiError::RelayerUnavailable(_)
    )
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TransactionRow {
    pub id: i32,
    pub tx_hash: String,
    pub user_address: String,
    #[serde(serialize_with = "serialize_created_at")]
    pub created_at: NaiveDateTime,
}

fn serialize_created_at<S>(dt: &NaiveDateTime, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// The broadcast path one POST /transactions resolves to. Under the Auto
/// preference, locally-provisioned signers win over the upstream forward,
/// which is the fallback for nodes without real relayer creds. An explicit
/// Oz/Direct preference is local-or-fail: it never rides the upstream, so the
/// admin can force local broadcasting while TRANSACTIONS_UPSTREAM_URL is set.
/// The operator toggle beats everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastRoute {
    Disabled,
    Oz,
    Direct,
    Upstream,
    None,
}

pub fn select_broadcast_route(
    enabled: bool,
    pref: SignerPreference,
    has_oz: bool,
    has_direct: bool,
    has_upstream: bool,
) -> BroadcastRoute {
    if !enabled {
        return BroadcastRoute::Disabled;
    }
    match pref {
        // An explicit preference whose provider is unprovisioned yields the
        // no-provider 503 rather than silently falling through to upstream.
        SignerPreference::Oz if has_oz => BroadcastRoute::Oz,
        SignerPreference::Direct if has_direct => BroadcastRoute::Direct,
        SignerPreference::Oz | SignerPreference::Direct => BroadcastRoute::None,
        SignerPreference::Auto if has_oz => BroadcastRoute::Oz,
        SignerPreference::Auto if has_direct => BroadcastRoute::Direct,
        SignerPreference::Auto if has_upstream => BroadcastRoute::Upstream,
        SignerPreference::Auto => BroadcastRoute::None,
    }
}

pub struct TransactionComponent {
    pool: PgPool,
    http: reqwest::Client,
    relayer: Option<Relayer>,
    signer: Option<DirectSigner>,
    upstream: Option<UpstreamForwarder>,
    runtime: Arc<RuntimeConfig>,
}

impl TransactionComponent {
    pub fn new(
        pool: PgPool,
        relayer: Option<Relayer>,
        signer: Option<DirectSigner>,
        upstream: Option<UpstreamForwarder>,
        runtime: Arc<RuntimeConfig>,
    ) -> Self {
        Self {
            pool,
            http: reqwest::Client::new(),
            relayer,
            signer,
            upstream,
            runtime,
        }
    }

    pub fn has_oz_relayer(&self) -> bool {
        self.relayer.is_some()
    }

    pub fn has_direct_signer(&self) -> bool {
        self.signer.is_some()
    }

    pub fn has_upstream_forwarder(&self) -> bool {
        self.upstream.is_some()
    }

    pub fn broadcast_route(&self) -> BroadcastRoute {
        select_broadcast_route(
            self.runtime.relayer_enabled(),
            self.runtime.signer_preference(),
            self.relayer.is_some(),
            self.signer.is_some(),
            self.upstream.is_some(),
        )
    }

    /// Some only when the current selection resolves to the upstream forward;
    /// the handler branches on this before calling `send_meta_transaction`.
    pub fn upstream_route(&self) -> Option<&UpstreamForwarder> {
        match self.broadcast_route() {
            BroadcastRoute::Upstream => self.upstream.as_ref(),
            _ => None,
        }
    }

    pub fn direct_signer(&self) -> Option<&DirectSigner> {
        self.signer.as_ref()
    }

    pub async fn reserve_quota(
        &self,
        max_transactions_per_day: i64,
        sender: &MetaTxSender,
        session_id: &str,
    ) -> Result<(), ApiError> {
        let user_address = sender.as_str().to_string();

        let mut db_tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&user_address)
            .execute(&mut *db_tx)
            .await?;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions \
             WHERE user_address = $1 AND created_at >= NOW() - INTERVAL '1 day'",
        )
        .bind(&user_address)
        .fetch_one(&mut *db_tx)
        .await?;

        if count >= max_transactions_per_day {
            db_tx.rollback().await?;
            return Err(ApiError::QuotaReached(format!(
                "Max amount of transactions reached for address. Quota: {count}"
            )));
        }

        sqlx::query("INSERT INTO transactions (user_address, session_id) VALUES ($1, $2)")
            .bind(&user_address)
            .bind(session_id)
            .execute(&mut *db_tx)
            .await?;

        db_tx.commit().await?;
        Ok(())
    }

    pub async fn confirm_reservation(
        &self,
        session_id: &str,
        tx_hash: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE transactions SET tx_hash = $1, session_id = NULL WHERE session_id = $2",
        )
        .bind(tx_hash)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release_reservation(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM transactions WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_by_user_address(
        &self,
        user_address: &str,
    ) -> Result<Vec<TransactionRow>, ApiError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT id, tx_hash, user_address, created_at FROM transactions \
             WHERE user_address = $1 AND tx_hash IS NOT NULL",
        )
        .bind(user_address.to_lowercase())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn check_data(
        &self,
        cfg: &Config,
        contracts: &ContractsComponent,
        tx: &TransactionData,
    ) -> Result<MetaTxSender, ApiError> {
        self.check_function_selector(tx)?;
        let sender = MetaTxSender::from_meta_tx_calldata(&tx.from, &tx.params[1])?;
        self.check_quota(cfg, &sender).await?;
        if cfg.has_rpc() {
            self.check_gas_price(cfg, tx).await?;
            self.check_transaction(cfg, tx).await?;
        }
        check_sale_price(cfg, tx)?;
        self.check_contract_address(contracts, tx).await?;
        Ok(sender)
    }

    fn check_function_selector(&self, tx: &TransactionData) -> Result<(), ApiError> {
        let data = hex_to_bytes(&tx.params[1]).unwrap_or_default();
        if !abi::is_execute_meta_tx(&data) {
            let sel: String = tx.params[1]
                .chars()
                .take(10)
                .collect::<String>()
                .to_lowercase();
            return Err(ApiError::InvalidTransaction(format!(
                "Invalid function selector. Only executeMetaTransaction (0x0c53c51c or 0xd8ed1acc) is allowed. Received: {sel}"
            )));
        }
        Ok(())
    }

    async fn check_contract_address(
        &self,
        contracts: &ContractsComponent,
        tx: &TransactionData,
    ) -> Result<(), ApiError> {
        let contract_address = &tx.params[0];
        if !contracts.is_valid_address(contract_address).await? {
            return Err(ApiError::InvalidContractAddress(format!(
                "Invalid contract address. Contract address: {contract_address}"
            )));
        }
        Ok(())
    }

    async fn check_quota(&self, cfg: &Config, sender: &MetaTxSender) -> Result<(), ApiError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions \
             WHERE user_address = $1 AND created_at >= NOW() - INTERVAL '1 day'",
        )
        .bind(sender.as_str())
        .fetch_one(&self.pool)
        .await?;

        if count >= cfg.max_transactions_per_day {
            return Err(ApiError::QuotaReached(format!(
                "Max amount of transactions reached for address. Quota: {count}"
            )));
        }
        Ok(())
    }

    async fn check_transaction(&self, cfg: &Config, tx: &TransactionData) -> Result<(), ApiError> {
        let rpc_url = cfg.rpc_url.as_deref().expect("has_rpc gated");
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_estimateGas",
            "params": [{
                "from": tx.from.to_lowercase(),
                "to": tx.params[0].to_lowercase(),
                "data": tx.params[1],
            }]
        });
        let resp: JsonRpcResponse = self.rpc_call(rpc_url, body).await.map_err(|e| {
            ApiError::InvalidTransaction(format!("Error simulating transaction: {e}"))
        })?;
        if let Some(err) = resp.error {
            return Err(ApiError::InvalidTransaction(format!(
                "Error simulating transaction: {}",
                err.message
            )));
        }
        Ok(())
    }

    async fn check_gas_price(&self, cfg: &Config, _tx: &TransactionData) -> Result<(), ApiError> {
        let Some(max_allowed) = cfg.max_gas_price_allowed_in_wei else {
            return Ok(());
        };
        let rpc_url = cfg.rpc_url.as_deref().expect("has_rpc gated");
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_gasPrice",
            "params": []
        });
        let resp: JsonRpcResponse = self
            .rpc_call(rpc_url, body)
            .await
            .map_err(|e| ApiError::Internal(format!("Could not get current gas price: {e}")))?;
        let hex = resp
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::Internal("Could not get current gas price".into()))?;
        let current = u128::from_str_radix(hex.trim_start_matches("0x"), 16)
            .map_err(|e| ApiError::Internal(format!("bad gas price: {e}")))?;
        if current > max_allowed {
            return Err(ApiError::HighCongestion(format!(
                "Current network gas price {current} exceeds max gas price allowed {max_allowed}"
            )));
        }
        Ok(())
    }

    async fn rpc_call(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<JsonRpcResponse, String> {
        let resp = self
            .http
            .post(url)
            .timeout(Duration::from_secs(15))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json::<JsonRpcResponse>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn send_meta_transaction(
        &self,
        _cfg: &Config,
        tx: &TransactionData,
    ) -> Result<String, ApiError> {
        match self.broadcast_route() {
            BroadcastRoute::Disabled => Err(ApiError::RelayerUnavailable(
                "Broadcasting is currently disabled by the operator (relayer toggle is OFF). Validation passed; the meta-transaction was not broadcast.".into(),
            )),
            BroadcastRoute::Oz => {
                self.relayer
                    .as_ref()
                    .expect("route Oz implies a provisioned relayer")
                    .send_meta_transaction(tx)
                    .await
            }
            BroadcastRoute::Direct => {
                self.signer
                    .as_ref()
                    .expect("route Direct implies a provisioned signer")
                    .send_meta_transaction(tx)
                    .await
            }
            // The handler intercepts the upstream route before broadcast, so
            // this arm is only reachable through a caller that skipped
            // `upstream_route()`; it fails safe rather than double-relaying.
            BroadcastRoute::Upstream => Err(ApiError::RelayerUnavailable(
                "Upstream forwarding is handled at the HTTP layer; no local broadcast provider is provisioned.".into(),
            )),
            BroadcastRoute::None => Err(ApiError::RelayerUnavailable(
                "No relayer is provisioned for the selected signer preference. Validation passed; broadcast is unavailable. Set OZ_RELAYER_URL/OZ_RELAYER_ID/OZ_RELAYER_API_KEY, META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY (+ RPC_URL), or TRANSACTIONS_UPSTREAM_URL, to enable broadcasting.".into(),
            )),
        }
    }
}

fn check_sale_price(cfg: &Config, tx: &TransactionData) -> Result<(), ApiError> {
    let Some(contracts) = DclContracts::for_chain(cfg.collections_chain_id) else {
        return Ok(());
    };
    let Ok(contract_address) = tx.params[0].trim().parse::<alloy::primitives::Address>() else {
        return Ok(());
    };
    let kind = if contract_address == contracts.collection_store {
        SaleKind::CollectionStore
    } else if contract_address == contracts.marketplace_v2 {
        SaleKind::MarketplaceV2
    } else if contract_address == contracts.bid_v2 {
        SaleKind::BidV2
    } else {
        return Ok(());
    };

    let full_data = match hex_to_bytes(&tx.params[1]) {
        Some(b) => b,
        None => return Ok(()),
    };

    let Some(sale_price) = abi::get_sale_price(&full_data, kind) else {
        return Ok(());
    };

    let min = U256::from_str_radix(&cfg.min_sale_value_in_wei, 10)
        .map_err(|e| ApiError::Internal(format!("bad MIN_SALE_VALUE_IN_WEI: {e}")))?;
    if sale_price < min {
        return Err(ApiError::InvalidSalePrice(format!(
            "The transaction data contains a sale price that's lower than the allowed minimum. Sale price: {sale_price} - Minimum price: {min}"
        )));
    }
    Ok(())
}

fn is_hex_address(s: &str) -> bool {
    match s.strip_prefix("0x") {
        Some(rest) => rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

fn is_hex_data(s: &str) -> bool {
    match s.strip_prefix("0x") {
        Some(rest) => !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    catalyrst_types::decode_hex_0x(s).ok()
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: &str = "0xe539E0AED3C1971560517D58277f8dd9aC296281";
    const CONTRACT: &str = "0x7ad72b9f944ea9793cf4055d88f81138cc2c63a0";
    const CALLDATA: &str = "0x0c53c51cffffffffffffffff";

    fn tx(from: &str, params: Value) -> Value {
        json!({ "from": from, "params": params })
    }

    fn schema_msg(res: Result<TransactionData, ApiError>) -> String {
        match res {
            Err(ApiError::InvalidSchema(m)) => m,
            other => panic!("expected ApiError::InvalidSchema, got {other:?}"),
        }
    }

    #[test]
    fn accepts_well_formed_transaction_data() {
        let data = tx(FROM, json!([CONTRACT, CALLDATA]));
        let parsed = validate_transaction_data(&data).expect("valid payload should parse");
        assert_eq!(parsed.from, FROM);
        assert_eq!(
            parsed.params,
            vec![CONTRACT.to_string(), CALLDATA.to_string()]
        );
    }

    #[test]
    fn rejects_from_that_does_not_match_address_pattern() {
        let data = tx("0x1234", json!([CONTRACT, CALLDATA]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(msg.contains(r#""instancePath":"/from""#), "{msg}");
        assert!(
            msg.contains(r##""schemaPath":"#/properties/from/pattern""##),
            "{msg}"
        );
        assert!(msg.contains(r#""keyword":"pattern""#), "{msg}");
        assert!(msg.contains(r#""pattern":"^0x[a-fA-F0-9]{40}$""#), "{msg}");
        assert!(
            msg.contains(r#"must match pattern \"^0x[a-fA-F0-9]{40}$\""#),
            "{msg}"
        );
    }

    #[test]
    fn rejects_from_missing_0x_prefix() {
        let data = tx(
            "e539E0AED3C1971560517D58277f8dd9aC296281",
            json!([CONTRACT, CALLDATA]),
        );
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(
            msg.contains(r##""schemaPath":"#/properties/from/pattern""##),
            "{msg}"
        );
    }

    #[test]
    fn rejects_from_with_non_hex_char() {
        let data = tx(
            "0xg539E0AED3C1971560517D58277f8dd9aC296281",
            json!([CONTRACT, CALLDATA]),
        );
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(
            msg.contains(r##""schemaPath":"#/properties/from/pattern""##),
            "{msg}"
        );
    }

    #[test]
    fn rejects_contract_param_that_does_not_match_address_pattern() {
        let data = tx(FROM, json!([CALLDATA, CALLDATA]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(msg.contains(r#""instancePath":"/params/0""#), "{msg}");
        assert!(
            msg.contains(r##""schemaPath":"#/properties/params/items/0/pattern""##),
            "{msg}"
        );
        assert!(msg.contains(r#""pattern":"^0x[a-fA-F0-9]{40}$""#), "{msg}");
    }

    #[test]
    fn rejects_calldata_param_that_does_not_match_hex_pattern() {
        let data = tx(FROM, json!([CONTRACT, "0xnothex"]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(msg.contains(r#""instancePath":"/params/1""#), "{msg}");
        assert!(
            msg.contains(r##""schemaPath":"#/properties/params/items/1/pattern""##),
            "{msg}"
        );
        assert!(msg.contains(r#""pattern":"^0x[a-fA-F0-9]+$""#), "{msg}");
        assert!(
            msg.contains(r#"must match pattern \"^0x[a-fA-F0-9]+$\""#),
            "{msg}"
        );
    }

    #[test]
    fn rejects_empty_calldata_param() {
        let data = tx(FROM, json!([CONTRACT, "0x"]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(
            msg.contains(r##""schemaPath":"#/properties/params/items/1/pattern""##),
            "{msg}"
        );
    }

    #[test]
    fn rejects_oversized_calldata_before_pattern() {
        let oversized = format!("0x{}", "a".repeat(200_001));
        assert_eq!(oversized.chars().count(), 200_003);
        let data = tx(FROM, json!([CONTRACT, oversized]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(msg.contains(r#""instancePath":"/params/1""#), "{msg}");
        assert!(
            msg.contains(r##""schemaPath":"#/properties/params/items/1/maxLength""##),
            "{msg}"
        );
        assert!(msg.contains(r#""keyword":"maxLength""#), "{msg}");
        assert!(msg.contains(r#""limit":200002"#), "{msg}");
        assert!(
            msg.contains("must NOT have more than 200002 characters"),
            "{msg}"
        );
    }

    #[test]
    fn accepts_calldata_at_max_length_boundary() {
        let at_limit = format!("0x{}", "a".repeat(200_000));
        assert_eq!(at_limit.chars().count(), CALLDATA_MAX_LENGTH);
        let data = tx(FROM, json!([CONTRACT, at_limit.clone()]));
        let parsed = validate_transaction_data(&data).expect("payload at cap should parse");
        assert_eq!(parsed.params[1], at_limit);
    }

    #[test]
    fn oversized_non_hex_calldata_reports_maxlength_not_pattern() {
        let oversized = format!("0x{}", "z".repeat(200_001));
        let data = tx(FROM, json!([CONTRACT, oversized]));
        let msg = schema_msg(validate_transaction_data(&data));
        assert!(
            msg.contains(r##""schemaPath":"#/properties/params/items/1/maxLength""##),
            "{msg}"
        );
    }

    fn meta_tx_calldata(user_address: &str) -> String {
        let call = abi::executeMetaTransactionCall {
            userAddress: user_address.parse().expect("address"),
            functionSignature: alloy::primitives::Bytes::from_static(&[0xaa]),
            sigR: alloy::primitives::FixedBytes::<32>::repeat_byte(0x11),
            sigS: alloy::primitives::FixedBytes::<32>::repeat_byte(0x22),
            sigV: 27,
        };
        format!(
            "0x{}",
            alloy::hex::encode(alloy::sol_types::SolCall::abi_encode(&call))
        )
    }

    #[test]
    fn sender_is_the_address_signed_into_the_calldata() {
        let sender = MetaTxSender::from_meta_tx_calldata(FROM, &meta_tx_calldata(FROM))
            .expect("matching from and userAddress");
        assert_eq!(sender.as_str(), FROM.to_lowercase());
    }

    #[test]
    fn a_from_that_disagrees_with_the_signed_user_address_is_rejected() {
        let victim = "0x1111111111111111111111111111111111111111";
        let attacker = "0x2222222222222222222222222222222222222222";
        let err = MetaTxSender::from_meta_tx_calldata(victim, &meta_tx_calldata(attacker))
            .expect_err("a forged from must not yield a sender");
        let msg = match err {
            ApiError::InvalidTransaction(m) => m,
            other => panic!("expected ApiError::InvalidTransaction, got {other:?}"),
        };
        assert!(msg.contains(victim), "{msg}");
        assert!(msg.contains(attacker), "{msg}");
    }

    #[test]
    fn calldata_with_an_unrecoverable_user_address_yields_no_sender() {
        assert!(matches!(
            MetaTxSender::from_meta_tx_calldata(FROM, CALLDATA),
            Err(ApiError::InvalidTransaction(_))
        ));
    }

    #[test]
    fn pre_broadcast_errors_release_the_slot() {
        for err in [
            ApiError::InvalidTransaction("bad".into()),
            ApiError::InvalidSchema("bad".into()),
            ApiError::InvalidContractAddress("bad".into()),
            ApiError::InvalidSalePrice("bad".into()),
            ApiError::HighCongestion("busy".into()),
            ApiError::RelayerFailed("network".into()),
            ApiError::RelayerUnavailable("off".into()),
        ] {
            assert_eq!(
                reservation_disposition(&err),
                ReservationDisposition::Release,
                "{err:?} is a pre-broadcast failure and must refund the slot"
            );
        }
    }

    #[test]
    fn post_broadcast_errors_keep_the_slot() {
        for err in [
            ApiError::RelayReverted("reverted".into()),
            ApiError::RelayerTimeout("too slow".into()),
        ] {
            assert_eq!(
                reservation_disposition(&err),
                ReservationDisposition::Keep,
                "{err:?} is post-broadcast/indeterminate and must keep the slot consumed"
            );
        }
    }

    #[test]
    fn upstream_is_the_auto_fallback_after_local_providers() {
        use crate::admin::SignerPreference::*;

        // Toggle OFF beats every provider, including the upstream forward.
        assert_eq!(
            select_broadcast_route(false, Auto, true, true, true),
            BroadcastRoute::Disabled
        );

        // Locally-provisioned signers win over the upstream forward.
        assert_eq!(
            select_broadcast_route(true, Auto, true, false, true),
            BroadcastRoute::Oz
        );
        assert_eq!(
            select_broadcast_route(true, Auto, false, true, true),
            BroadcastRoute::Direct
        );
        assert_eq!(
            select_broadcast_route(true, Oz, true, true, true),
            BroadcastRoute::Oz
        );
        assert_eq!(
            select_broadcast_route(true, Direct, true, true, true),
            BroadcastRoute::Direct
        );

        // Auto with no local provider: the upstream carries.
        assert_eq!(
            select_broadcast_route(true, Auto, false, false, true),
            BroadcastRoute::Upstream
        );

        // Nothing provisioned at all.
        assert_eq!(
            select_broadcast_route(true, Auto, false, false, false),
            BroadcastRoute::None
        );
    }

    #[test]
    fn explicit_preference_without_its_provider_fails_instead_of_forwarding() {
        use crate::admin::SignerPreference::*;

        // Local-or-fail: an explicit preference never rides the upstream,
        // even when the other local provider or the forwarder is available.
        assert_eq!(
            select_broadcast_route(true, Oz, false, true, true),
            BroadcastRoute::None
        );
        assert_eq!(
            select_broadcast_route(true, Direct, true, false, true),
            BroadcastRoute::None
        );
        assert_eq!(
            select_broadcast_route(true, Oz, false, false, false),
            BroadcastRoute::None
        );
        assert_eq!(
            select_broadcast_route(true, Direct, false, false, false),
            BroadcastRoute::None
        );
    }

    #[test]
    fn unclassified_errors_default_to_keep() {
        assert_eq!(
            reservation_disposition(&ApiError::Internal("boom".into())),
            ReservationDisposition::Keep
        );
        assert_eq!(
            reservation_disposition(&ApiError::Conflict("dup".into())),
            ReservationDisposition::Keep
        );
    }
}
