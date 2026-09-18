use super::*;
use catalyrst_contract_gate::pg::ScratchDb;

fn asset(kind: i32, value: &str) -> TradeAssetInput {
    TradeAssetInput {
        asset_type: kind,
        contract_address: "0xABC".into(),
        beneficiary: Some("0xDEF".into()),
        extra: None,
        token_id: Some(value.into()),
        amount: Some(value.into()),
        item_id: Some(value.into()),
    }
}

fn trade(
    signature: &str,
    sent: Vec<TradeAssetInput>,
    received: Vec<TradeAssetInput>,
) -> TradeCreation {
    let mut trade: TradeCreation = serde_json::from_value(serde_json::json!({
        "signer": "owner", "signature": signature, "type": "bid",
        "network": "ethereum", "chainId": 1,
        "checks": {"uses": 1, "expiration": 0, "effective": 0, "salt": "0x",
                   "contractSignatureIndex": 0, "signerSignatureIndex": 0, "allowedRoot": "0x"},
        "sent": [], "received": []
    }))
    .unwrap();
    trade.sent = sent;
    trade.received = received;
    trade
}

async fn insert(
    pool: &PgPool,
    signature: &str,
    sent: Vec<TradeAssetInput>,
    received: Vec<TradeAssetInput>,
) -> Result<String, TradeCreationError> {
    insert_trade(
        pool,
        &trade(signature, sent, received),
        "ethereum",
        "",
        None,
    )
    .await
}

#[tokio::test]
async fn asset_batches_keep_subtypes_duplicates_precision_and_atomicity() {
    let Some(db) = ScratchDb::create("CATALYRST_TEST_PG", "trade_assets_batch").await else {
        return;
    };
    sqlx::query("CREATE SCHEMA marketplace")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/0002_trades_schema.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE marketplace.trades ADD COLUMN IF NOT EXISTS trade_digest text")
        .execute(&db.pool)
        .await
        .unwrap();
    let big = "1234567890123456789012345678901234567890";
    let sent: Vec<_> = (0..10).map(|i| asset(i % 4 + 1, big)).collect();
    let received = vec![
        asset(ASSET_TYPE_ERC721, "42"),
        asset(ASSET_TYPE_COLLECTION_ITEM, "7"),
    ];
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    let id = insert(&db.pool, "good", sent, received).await.unwrap();
    assert_eq!(capture.count(), 1);
    type AssetRow = (
        String,
        i16,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<AssetRow> = sqlx::query_as(
        "SELECT a.direction::text,a.asset_type,a.contract_address,a.beneficiary,a.extra,n.token_id,f.amount::text,i.item_id FROM marketplace.trade_assets a
         LEFT JOIN marketplace.trade_assets_erc721 n ON n.asset_id=a.id
         LEFT JOIN marketplace.trade_assets_erc20 f ON f.asset_id=a.id
         LEFT JOIN marketplace.trade_assets_item i ON i.asset_id=a.id WHERE a.trade_id=$1::uuid")
        .bind(&id).fetch_all(&db.pool).await.unwrap();
    assert_eq!(rows.len(), 12);
    for (direction, kind, contract, beneficiary, extra, token, amount, item) in rows {
        assert_eq!(contract, "0xabc");
        assert_eq!(beneficiary.as_deref(), Some("0xdef"));
        assert_eq!(extra, "0x");
        let expected = if direction == "sent" {
            big
        } else if kind == 3 {
            "42"
        } else {
            "7"
        };
        assert_eq!(token.as_deref(), (kind == 3).then_some(expected));
        assert_eq!(
            amount.as_deref(),
            (kind == 1 || kind == 2).then_some(expected)
        );
        assert_eq!(item.as_deref(), (kind == 4).then_some(expected));
    }
    let large: Vec<_> = (0..513).map(|_| asset(3, "9")).collect();
    capture.reset();
    let large_id = insert(&db.pool, "large", large, vec![]).await.unwrap();
    assert_eq!(capture.count(), 1);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM marketplace.trade_assets WHERE trade_id=$1::uuid")
            .bind(&large_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 513);
    let mut invalid: Vec<_> = (0..512).map(|_| asset(3, "9")).collect();
    invalid.push(asset(1, "-1"));
    assert!(insert(&db.pool, "bad", invalid, vec![]).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM marketplace.trades WHERE signature='bad'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    for kind in [1, 2, 3, 4, 99] {
        let mut invalid = asset(kind, "0");
        invalid.amount = None;
        invalid.token_id = None;
        invalid.item_id = None;
        assert!(matches!(
            insert(&db.pool, "missing", vec![invalid], vec![]).await,
            Err(TradeCreationError::InvalidStructure(_))
        ));
    }
    db.drop().await;
}
