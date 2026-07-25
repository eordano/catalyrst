use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub const DEFAULT_TRADES_SYNC_UPSTREAM_URL: &str =
    "https://marketplace-api.decentraland.org/v1/trades";

pub const DEFAULT_TRADES_SYNC_INTERVAL_SECS: u64 = 900;

#[derive(Debug, Clone)]
pub struct UpstreamTradeHead {
    pub id: String,
    pub network: String,
    pub chain_id: i32,
    pub signature: String,
    pub hashed_signature: String,
    pub checks: serde_json::Value,
    pub signer: String,
    pub trade_type: String,
    pub expires_at: DateTime<Utc>,
    pub effective_since: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub contract: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetValue {
    Erc20 { amount: String },
    Erc721 { token_id: String },
    Item { item_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamTradeAsset {
    pub direction: &'static str,
    pub asset_type: i16,
    pub contract_address: String,
    pub beneficiary: Option<String>,
    pub extra: String,
    pub value: AssetValue,
}

fn str_field(v: &serde_json::Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string field {key:?}"))
}

fn ts_field(v: &serde_json::Value, key: &str) -> Result<DateTime<Utc>, String> {
    let raw = str_field(v, key)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("bad timestamp {key:?}={raw:?}: {e}"))
}

pub fn parse_list_row(v: &serde_json::Value) -> Result<UpstreamTradeHead, String> {
    let trade_type = str_field(v, "type")?;
    match trade_type.as_str() {
        "bid" | "public_nft_order" | "public_item_order" => {}
        other => return Err(format!("unknown trade type {other:?}")),
    }
    Ok(UpstreamTradeHead {
        id: str_field(v, "id")?,
        network: str_field(v, "network")?,
        chain_id: v
            .get("chain_id")
            .and_then(|x| x.as_i64())
            .ok_or("missing chain_id")? as i32,
        signature: str_field(v, "signature")?.to_lowercase(),
        hashed_signature: str_field(v, "hashed_signature")?.to_lowercase(),
        checks: v.get("checks").cloned().ok_or("missing checks")?,
        signer: str_field(v, "signer")?.to_lowercase(),
        trade_type,
        expires_at: ts_field(v, "expires_at")?,
        effective_since: ts_field(v, "effective_since")?,
        created_at: ts_field(v, "created_at")?,
        contract: str_field(v, "contract")?,
    })
}

fn normalize_extra(raw: Option<&str>) -> String {
    match raw {
        None | Some("") | Some("0x") => String::new(),
        Some(s) => s.to_string(),
    }
}

fn parse_asset(
    v: &serde_json::Value,
    direction: &'static str,
) -> Result<UpstreamTradeAsset, String> {
    let asset_type = v
        .get("assetType")
        .and_then(|x| x.as_i64())
        .ok_or("asset missing assetType")?;
    let value = match asset_type {
        1 | 2 => AssetValue::Erc20 {
            amount: str_field(v, "amount")?,
        },
        3 => AssetValue::Erc721 {
            token_id: str_field(v, "tokenId")?,
        },
        4 => AssetValue::Item {
            item_id: str_field(v, "itemId")?,
        },
        other => return Err(format!("unsupported assetType {other}")),
    };
    let beneficiary = v
        .get("beneficiary")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase());
    Ok(UpstreamTradeAsset {
        direction,
        asset_type: asset_type as i16,
        contract_address: str_field(v, "contractAddress")?.to_lowercase(),
        beneficiary,
        extra: normalize_extra(v.get("extra").and_then(|x| x.as_str())),
        value,
    })
}

pub fn parse_detail_assets(detail: &serde_json::Value) -> Result<Vec<UpstreamTradeAsset>, String> {
    let mut out = Vec::new();
    for (key, direction) in [("sent", "sent"), ("received", "received")] {
        let arr = detail
            .get(key)
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("detail missing {key:?} array"))?;
        for a in arr {
            out.push(parse_asset(a, direction)?);
        }
    }
    if out.is_empty() {
        return Err("trade detail carries no assets".into());
    }
    Ok(out)
}

async fn insert_trade(
    pool: &PgPool,
    head: &UpstreamTradeHead,
    assets: &[UpstreamTradeAsset],
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO marketplace.trades \
             (id, network, chain_id, signature, hashed_signature, checks, signer, type, \
              expires_at, effective_since, contract, created_at) \
         VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8::marketplace.trade_type, $9, $10, $11, $12) \
         ON CONFLICT (hashed_signature) DO NOTHING \
         RETURNING id::text",
    )
    .bind(&head.id)
    .bind(&head.network)
    .bind(head.chain_id)
    .bind(&head.signature)
    .bind(&head.hashed_signature)
    .bind(&head.checks)
    .bind(&head.signer)
    .bind(&head.trade_type)
    .bind(head.expires_at)
    .bind(head.effective_since)
    .bind(&head.contract)
    .bind(head.created_at)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((trade_id,)) = inserted else {
        tx.rollback().await?;
        return Ok(false);
    };

    for asset in assets {
        let (asset_id,): (String,) = sqlx::query_as(
            "INSERT INTO marketplace.trade_assets \
                 (trade_id, direction, asset_type, contract_address, beneficiary, extra) \
             VALUES ($1::uuid, $2::marketplace.asset_direction_type, $3, $4, $5, $6) \
             RETURNING id::text",
        )
        .bind(&trade_id)
        .bind(asset.direction)
        .bind(asset.asset_type)
        .bind(&asset.contract_address)
        .bind(asset.beneficiary.as_deref())
        .bind(&asset.extra)
        .fetch_one(&mut *tx)
        .await?;

        match &asset.value {
            AssetValue::Erc20 { amount } => {
                sqlx::query(
                    "INSERT INTO marketplace.trade_assets_erc20 (asset_id, amount) \
                     VALUES ($1::uuid, $2::numeric)",
                )
                .bind(&asset_id)
                .bind(amount)
                .execute(&mut *tx)
                .await?;
            }
            AssetValue::Erc721 { token_id } => {
                sqlx::query(
                    "INSERT INTO marketplace.trade_assets_erc721 (asset_id, token_id) \
                     VALUES ($1::uuid, $2)",
                )
                .bind(&asset_id)
                .bind(token_id)
                .execute(&mut *tx)
                .await?;
            }
            AssetValue::Item { item_id } => {
                sqlx::query(
                    "INSERT INTO marketplace.trade_assets_item (asset_id, item_id) \
                     VALUES ($1::uuid, $2)",
                )
                .bind(&asset_id)
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(true)
}

struct SweepStats {
    fetched: usize,
    new: usize,
    skipped: usize,
    failed: usize,
}

async fn run_sweep(
    http: &reqwest::Client,
    pool: &PgPool,
    list_url: &str,
) -> Result<SweepStats, String> {
    let resp = http
        .get(list_url)
        .send()
        .await
        .map_err(|e| format!("upstream list request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("upstream list returned {}", resp.status().as_u16()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("upstream list parse failed: {e}"))?;
    let rows = body
        .get("data")
        .and_then(|d| d.get("data"))
        .and_then(|d| d.as_array())
        .ok_or("upstream list missing data.data array")?;

    let known: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT hashed_signature FROM marketplace.trades")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("local hashed_signature scan failed: {e}"))?
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();

    let mut stats = SweepStats {
        fetched: rows.len(),
        new: 0,
        skipped: 0,
        failed: 0,
    };

    let detail_base = list_url.trim_end_matches('/');
    for row in rows {
        let head = match parse_list_row(row) {
            Ok(h) => h,
            Err(e) => {
                stats.failed += 1;
                tracing::warn!(error = %e, "trades sync: unparseable upstream list row");
                continue;
            }
        };
        if known.contains(&head.hashed_signature) {
            stats.skipped += 1;
            continue;
        }

        let detail_url = format!("{detail_base}/{}", head.id);
        let assets = match fetch_detail_assets(http, &detail_url).await {
            Ok(a) => a,
            Err(e) => {
                stats.failed += 1;
                tracing::warn!(trade_id = %head.id, error = %e, "trades sync: detail fetch failed");
                continue;
            }
        };

        match insert_trade(pool, &head, &assets).await {
            Ok(true) => stats.new += 1,
            Ok(false) => stats.skipped += 1,
            Err(e) => {
                stats.failed += 1;
                tracing::warn!(trade_id = %head.id, error = %e, "trades sync: insert failed");
            }
        }
    }
    Ok(stats)
}

async fn fetch_detail_assets(
    http: &reqwest::Client,
    url: &str,
) -> Result<Vec<UpstreamTradeAsset>, String> {
    let resp = http
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("status {}", resp.status().as_u16()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("parse failed: {e}"))?;
    let detail = body.get("data").ok_or("detail missing data")?;
    parse_detail_assets(detail)
}

pub fn spawn_trades_upstream_sync(pool: PgPool, upstream_url: String, interval_secs: u64) {
    tokio::spawn(async move {
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "trades sync: could not build http client; sync off");
                return;
            }
        };
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(60)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match run_sweep(&http, &pool, &upstream_url).await {
                Ok(s) => tracing::info!(
                    fetched = s.fetched,
                    new = s.new,
                    skipped = s.skipped,
                    failed = s.failed,
                    "trades sync sweep complete"
                ),
                Err(e) => tracing::warn!(error = %e, "trades sync sweep skipped"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_list_row() -> serde_json::Value {
        serde_json::json!({
            "id": "6aa4130b-42b1-4ee5-84c1-6bac9e6b63f5",
            "network": "MATIC",
            "chain_id": 137,
            "signature": "0x7FD2afc813e91b1b5ddabe66cd81b132a881ff9a5d73440c100afe0a338d2d6b3598857ffa3add6acc1389d4b9d067ea14b138d9ad7488865b966920ee7e41181b",
            "hashed_signature": "0xF573a15c62efabc7ad5328d649fc89fd0bb10c66f7d524e336d4b1f5c10ad673",
            "checks": {
                "salt": "0x988c1638e5",
                "uses": 1,
                "effective": 1733691659485i64,
                "expiration": 1736204400000i64,
                "allowedRoot": "0x",
                "externalChecks": [],
                "signerSignatureIndex": 0,
                "contractSignatureIndex": 0
            },
            "signer": "0x747C6f502272129bf1ba872a1903045b837ee86c",
            "type": "public_nft_order",
            "expires_at": "2025-01-06T23:00:00.000Z",
            "effective_since": "2024-12-08T21:00:59.485Z",
            "created_at": "2024-12-08T21:01:07.473Z",
            "contract": "0x540fb08eDb56AaE562864B390542C97F562825BA"
        })
    }

    fn fixture_detail() -> serde_json::Value {
        serde_json::json!({
            "id": "568da193-be7a-474f-a2c5-e51fcd2e2d53",
            "signer": "0x747c6f502272129bf1ba872a1903045b837ee86c",
            "type": "public_nft_order",
            "sent": [
                {
                    "contractAddress": "0x659704BDC5152b0348a7b526d5120CAB98ee6f7F",
                    "extra": "",
                    "assetType": 3,
                    "tokenId": "450"
                }
            ],
            "received": [
                {
                    "contractAddress": "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
                    "extra": "0x",
                    "assetType": 1,
                    "amount": "150000000000000000000",
                    "beneficiary": "0x747C6F502272129bf1ba872a1903045b837ee86c"
                }
            ],
            "contract": "0x540fb08eDb56AaE562864B390542C97F562825BA"
        })
    }

    #[test]
    fn list_row_maps_to_trades_columns() {
        let head = parse_list_row(&fixture_list_row()).unwrap();
        assert_eq!(head.id, "6aa4130b-42b1-4ee5-84c1-6bac9e6b63f5");
        assert_eq!(head.network, "MATIC");
        assert_eq!(head.chain_id, 137);
        assert!(head.signature.starts_with("0x7fd2afc813e91b1b"));
        assert_eq!(
            head.hashed_signature,
            "0xf573a15c62efabc7ad5328d649fc89fd0bb10c66f7d524e336d4b1f5c10ad673"
        );
        assert_eq!(head.signer, "0x747c6f502272129bf1ba872a1903045b837ee86c");
        assert_eq!(head.trade_type, "public_nft_order");
        assert_eq!(head.checks["salt"], "0x988c1638e5");
        assert_eq!(head.checks["uses"], 1);
        assert_eq!(head.created_at.timestamp_millis(), 1_733_691_667_473);
        assert_eq!(head.expires_at.timestamp_millis(), 1_736_204_400_000);
        assert_eq!(head.effective_since.timestamp_millis(), 1_733_691_659_485);
        assert_eq!(head.contract, "0x540fb08eDb56AaE562864B390542C97F562825BA");
    }

    #[test]
    fn detail_assets_map_to_trade_asset_rows() {
        let assets = parse_detail_assets(&fixture_detail()).unwrap();
        assert_eq!(assets.len(), 2);

        let sent = &assets[0];
        assert_eq!(sent.direction, "sent");
        assert_eq!(sent.asset_type, 3);
        assert_eq!(
            sent.contract_address, "0x659704bdc5152b0348a7b526d5120cab98ee6f7f",
            "asset contract addresses are stored lowercase"
        );
        assert_eq!(sent.beneficiary, None, "sent assets carry no beneficiary");
        assert_eq!(sent.extra, "");
        assert_eq!(
            sent.value,
            AssetValue::Erc721 {
                token_id: "450".into()
            }
        );

        let received = &assets[1];
        assert_eq!(received.direction, "received");
        assert_eq!(received.asset_type, 1);
        assert_eq!(
            received.beneficiary.as_deref(),
            Some("0x747c6f502272129bf1ba872a1903045b837ee86c"),
            "received beneficiary is stored lowercase"
        );
        assert_eq!(received.extra, "", "a bare '0x' extra normalizes to ''");
        assert_eq!(
            received.value,
            AssetValue::Erc20 {
                amount: "150000000000000000000".into()
            }
        );
    }

    #[test]
    fn item_order_assets_route_to_the_item_sub_table() {
        let detail = serde_json::json!({
            "sent": [
                {"contractAddress": "0xAAA0000000000000000000000000000000000aaa",
                 "assetType": 4, "itemId": "7", "extra": ""}
            ],
            "received": [
                {"contractAddress": "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
                 "assetType": 1, "amount": "1000", "extra": "",
                 "beneficiary": "0xbbb0000000000000000000000000000000000bbb"}
            ]
        });
        let assets = parse_detail_assets(&detail).unwrap();
        assert_eq!(
            assets[0].value,
            AssetValue::Item {
                item_id: "7".into()
            }
        );
        assert_eq!(assets[0].asset_type, 4);
    }

    #[test]
    fn unknown_asset_type_is_refused() {
        let detail = serde_json::json!({
            "sent": [{"contractAddress": "0xaaa", "assetType": 9, "extra": ""}],
            "received": []
        });
        assert!(parse_detail_assets(&detail).is_err());
    }

    #[test]
    fn asset_missing_its_typed_value_is_refused() {
        let detail = serde_json::json!({
            "sent": [{"contractAddress": "0xaaa", "assetType": 3, "extra": ""}],
            "received": []
        });
        assert!(parse_detail_assets(&detail).is_err());
        let detail = serde_json::json!({
            "sent": [],
            "received": [{"contractAddress": "0xaaa", "assetType": 1, "extra": ""}]
        });
        assert!(parse_detail_assets(&detail).is_err());
    }

    #[test]
    fn tradeless_detail_is_refused() {
        assert!(parse_detail_assets(&serde_json::json!({"sent": [], "received": []})).is_err());
        assert!(parse_detail_assets(&serde_json::json!({})).is_err());
    }

    #[test]
    fn bid_rows_are_synced_and_unknown_types_refused() {
        let mut row = fixture_list_row();
        row["type"] = serde_json::json!("bid");
        assert_eq!(parse_list_row(&row).unwrap().trade_type, "bid");
        row["type"] = serde_json::json!("mystery_order");
        assert!(parse_list_row(&row).is_err());
    }

    #[test]
    fn extra_normalization_matches_snapshot_convention() {
        assert_eq!(normalize_extra(None), "");
        assert_eq!(normalize_extra(Some("")), "");
        assert_eq!(normalize_extra(Some("0x")), "");
        assert_eq!(normalize_extra(Some("0xdead")), "0xdead");
    }
}
