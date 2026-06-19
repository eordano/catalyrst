use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::U256;
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

#[derive(Debug, Clone)]
pub struct TransactionData {
    pub from: String,
    pub params: Vec<String>,
}

pub fn parse_send_transaction_request(body: &[u8]) -> Result<TransactionData, ApiError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| ApiError::MalformedBody(e.to_string()))?;
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
    ApiError::InvalidSchema(format!("Invalid transaction data. Errors: [{error_object}]"))
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

    if let Some(extra) = obj.keys().find(|k| k.as_str() != "from" && k.as_str() != "params") {
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
        match item {
            Value::String(s) => params.push(s.clone()),
            _ => {
                return Err(schema_error(
                    &format!("/params/{i}"),
                    &format!("#/properties/params/items/{i}/type"),
                    "type",
                    json!({ "type": "string" }),
                    "must be string",
                ));
            }
        }
    }

    Ok(TransactionData { from, params })
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

pub struct TransactionComponent {
    pool: PgPool,
    http: reqwest::Client,
    relayer: Option<Relayer>,
    signer: Option<DirectSigner>,
    runtime: Arc<RuntimeConfig>,
}

impl TransactionComponent {
    pub fn new(
        pool: PgPool,
        relayer: Option<Relayer>,
        signer: Option<DirectSigner>,
        runtime: Arc<RuntimeConfig>,
    ) -> Self {
        Self {
            pool,
            http: reqwest::Client::new(),
            relayer,
            signer,
            runtime,
        }
    }

    /// Whether the OZ HTTP relayer is provisioned (wired at startup).
    pub fn has_oz_relayer(&self) -> bool {
        self.relayer.is_some()
    }

    /// Whether the direct JSON-RPC signer is provisioned (wired at startup).
    pub fn has_direct_signer(&self) -> bool {
        self.signer.is_some()
    }

    pub async fn insert(&self, tx_hash: &str, user_address: &str) -> Result<(), ApiError> {
        sqlx::query("INSERT INTO transactions (tx_hash, user_address) VALUES ($1, $2)")
            .bind(tx_hash)
            .bind(user_address.to_lowercase())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_by_user_address(
        &self,
        user_address: &str,
    ) -> Result<Vec<TransactionRow>, ApiError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT id, tx_hash, user_address, created_at FROM transactions WHERE user_address = $1",
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
    ) -> Result<(), ApiError> {
        if cfg.has_rpc() {
            self.check_gas_price(cfg, tx).await?;
            self.check_transaction(cfg, tx).await?;
        }
        check_sale_price(cfg, tx)?;
        self.check_contract_address(contracts, tx).await?;
        self.check_quota(cfg, tx).await?;
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

    async fn check_quota(&self, cfg: &Config, tx: &TransactionData) -> Result<(), ApiError> {
        let from = tx.from.to_lowercase();
        // MAX_TRANSACTIONS_PER_DAY is a per-address daily cap (a relayer-gas
        // spend guard). Upstream transactions-server filters `created_at >= NOW()`,
        // which only matches future-dated rows — so the count is always ~0 and the
        // quota never actually fires. We diverge from that bug and count the
        // calendar day's rows (`>= CURRENT_DATE`), matching the upstream variable's
        // own name (`todayAddressTransactions`) and the MAX_TRANSACTIONS_PER_DAY
        // intent. Matters once a relayer is provisioned and broadcasts go live.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE user_address = $1 AND created_at >= CURRENT_DATE",
        )
        .bind(&from)
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
        let resp: JsonRpcResponse = self
            .rpc_call(rpc_url, body)
            .await
            .map_err(|e| ApiError::InvalidTransaction(format!("Error simulating transaction: {e}")))?;
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

    /// Broadcast a validated meta-transaction. Provider selection honours the
    /// runtime-mutable admin controls ([`crate::admin::RuntimeConfig`]):
    ///   - If the relayer master switch is OFF, short-circuit to 503 (the same
    ///     "validation passed, broadcast unavailable" contract), regardless of
    ///     what is provisioned. This is the admin "relayer off" toggle.
    ///   - Otherwise pick a provisioned provider per the signer-preference:
    ///       * `Auto`   → OZ HTTP relayer first, then direct JSON-RPC signer
    ///                    (the historical startup-only order).
    ///       * `Oz`     → OZ HTTP relayer only.
    ///       * `Direct` → direct JSON-RPC signer only.
    ///   - If the selected provider is not provisioned → 503.
    ///
    /// With the default runtime state (`enabled = true`, `signer = Auto`) this is
    /// byte-for-byte the prior behaviour.
    pub async fn send_meta_transaction(
        &self,
        _cfg: &Config,
        tx: &TransactionData,
    ) -> Result<String, ApiError> {
        if !self.runtime.relayer_enabled() {
            return Err(ApiError::RelayerUnavailable(
                "Broadcasting is currently disabled by the operator (relayer toggle is OFF). Validation passed; the meta-transaction was not broadcast.".into(),
            ));
        }

        let pref = self.runtime.signer_preference();
        let try_oz = matches!(pref, SignerPreference::Auto | SignerPreference::Oz);
        let try_direct = matches!(pref, SignerPreference::Auto | SignerPreference::Direct);

        if try_oz {
            if let Some(relayer) = &self.relayer {
                return relayer.send_meta_transaction(tx).await;
            }
        }
        if try_direct {
            if let Some(signer) = &self.signer {
                return signer.send_meta_transaction(tx).await;
            }
        }
        Err(ApiError::RelayerUnavailable(
            "No relayer is provisioned for the selected signer preference. Validation passed; broadcast is unavailable. Set OZ_RELAYER_URL/OZ_RELAYER_ID/OZ_RELAYER_API_KEY, or META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY (+ RPC_URL), to enable broadcasting.".into(),
        ))
    }
}

fn check_sale_price(cfg: &Config, tx: &TransactionData) -> Result<(), ApiError> {
    let Some(contracts) = DclContracts::for_chain(cfg.collections_chain_id) else {
        return Ok(());
    };
    let contract_address = tx.params[0].to_lowercase();
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

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
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
