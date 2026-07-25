use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::pagination::get_pagination_params;
use crate::http::response::ApiError;
use crate::ports::bids::{parse_filters, Bid};
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct BidsPage {
    pub results: Vec<Bid>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub page: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pages: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct BidsEnvelope {
    pub ok: bool,
    pub data: BidsPage,
}

pub async fn get_bids(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<BidsEnvelope>, ApiError> {
    let pg = get_pagination_params(&pairs);
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.bids.get_bids(&filters).await?;
    let page = if pg.limit > 0 {
        pg.offset / pg.limit
    } else {
        0
    };
    let pages = if !data.is_empty() && pg.limit > 0 {
        (total + pg.limit - 1) / pg.limit
    } else {
        0
    };
    Ok(Json(BidsEnvelope {
        ok: true,
        data: BidsPage {
            results: data,
            total,
            page,
            pages,
            limit: pg.limit,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcl_schemas::{ChainId, Network};
    use serde_json::json;

    fn full_bid() -> Bid {
        Bid {
            id: "7f7b021a-9207-4868-9543-722fbc674787".to_string(),
            bidder: "0x57d1721f6223eb434e20f5b4a88e494008ea0542".to_string(),
            price: "200000000000000000000".to_string(),
            created_at: 1_782_326_697_937,
            updated_at: 1_782_326_697_937,
            fingerprint: "".to_string(),
            status: "open".to_string(),
            seller: "0x597d94248c5181232e83ee054b74f01548090f33".to_string(),
            network: Network::Matic,
            chain_id: ChainId::MaticMainnet,
            contract_address: "0xf1483f042614105cb943d3dd67157256cd003028".to_string(),
            expires_at: 1_784_865_600_000,
            token_id: Some(
                "1579684375028357800468770415255056484783426431008236668814664663050".to_string(),
            ),
            item_id: Some("15".to_string()),
            trade_id: Some("7f7b021a-9207-4868-9543-722fbc674787".to_string()),
            trade_contract_address: Some("0xa40b1d129b8906888720686f3a01921ddf37716f".to_string()),
            bid_address: None,
            blockchain_id: None,
            block_number: None,
        }
    }

    fn minimal_bid() -> Bid {
        Bid {
            id: "legacy-1".to_string(),
            bidder: "0xabc".to_string(),
            price: "1".to_string(),
            created_at: 1,
            updated_at: 2,
            fingerprint: "0xf".to_string(),
            status: "cancelled".to_string(),
            seller: "0xdef".to_string(),
            network: Network::Ethereum,
            chain_id: ChainId::EthereumMainnet,
            contract_address: "0xc0".to_string(),
            expires_at: 3,
            token_id: None,
            item_id: None,
            trade_id: None,
            trade_contract_address: None,
            bid_address: Some("0xb1d".to_string()),
            blockchain_id: Some("bid-0xc0-61".to_string()),
            block_number: Some("123456".to_string()),
        }
    }

    #[test]
    fn wire_identity_bids_envelope() {
        let new = BidsEnvelope {
            ok: true,
            data: BidsPage {
                results: vec![full_bid(), minimal_bid()],
                total: 842,
                page: 0,
                pages: 9,
                limit: 100,
            },
        };
        let old = json!({
            "ok": true,
            "data": {
                "results": [
                    {
                        "id": "7f7b021a-9207-4868-9543-722fbc674787",
                        "bidder": "0x57d1721f6223eb434e20f5b4a88e494008ea0542",
                        "price": "200000000000000000000",
                        "createdAt": 1_782_326_697_937_i64,
                        "updatedAt": 1_782_326_697_937_i64,
                        "fingerprint": "",
                        "status": "open",
                        "seller": "0x597d94248c5181232e83ee054b74f01548090f33",
                        "network": "MATIC",
                        "chainId": 137,
                        "contractAddress": "0xf1483f042614105cb943d3dd67157256cd003028",
                        "expiresAt": 1_784_865_600_000_i64,
                        "tokenId": "1579684375028357800468770415255056484783426431008236668814664663050",
                        "itemId": "15",
                        "tradeId": "7f7b021a-9207-4868-9543-722fbc674787",
                        "tradeContractAddress": "0xa40b1d129b8906888720686f3a01921ddf37716f",
                    },
                    {
                        "id": "legacy-1",
                        "bidder": "0xabc",
                        "price": "1",
                        "createdAt": 1,
                        "updatedAt": 2,
                        "fingerprint": "0xf",
                        "status": "cancelled",
                        "seller": "0xdef",
                        "network": "ETHEREUM",
                        "chainId": 1,
                        "contractAddress": "0xc0",
                        "expiresAt": 3,
                        "bidAddress": "0xb1d",
                        "blockchainId": "bid-0xc0-61",
                        "blockNumber": "123456",
                    },
                ],
                "total": 842,
                "page": 0,
                "pages": 9,
                "limit": 100,
            }
        });
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }

    #[test]
    fn wire_identity_bids_envelope_empty() {
        let new = BidsEnvelope {
            ok: true,
            data: BidsPage {
                results: vec![],
                total: 0,
                page: 0,
                pages: 0,
                limit: 100,
            },
        };
        let old = json!({
            "ok": true,
            "data": {
                "results": [],
                "total": 0,
                "page": 0,
                "pages": 0,
                "limit": 100,
            }
        });
        assert_eq!(serde_json::to_value(&new).unwrap(), old);
    }
}
