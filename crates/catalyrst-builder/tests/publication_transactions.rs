use alloy::primitives::keccak256;
use axum::{extract::State, routing::post, Json, Router};
use catalyrst_builder::ports::{
    drafts::{CollectionDraft, ItemDraft},
    items::ItemsComponent,
    publication::{COLLECTION_FACTORY, COLLECTION_MANAGER},
    publication_chain::{publication_calldata, PublicationChain},
};
use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const OWNER: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const CONTRACT: &str = "0x3333333333333333333333333333333333333333";
const HASH: &str = "0x4444444444444444444444444444444444444444444444444444444444444444";
const BLOCK: &str = "0x5555555555555555555555555555555555555555555555555555555555555555";

async fn rpc(State(state): State<Arc<Mutex<Value>>>, Json(req): Json<Value>) -> Json<Value> {
    let s = state.lock().unwrap();
    let result = match req["method"].as_str().unwrap() {
        "eth_chainId" => json!("0x89"),
        "eth_getTransactionByHash" => s["tx"].clone(),
        "eth_getTransactionReceipt" => s["receipt"].clone(),
        "eth_getBlockByNumber" => {
            if req["params"][0] == "finalized" {
                json!({"number":s["finalized"], "hash":BLOCK})
            } else {
                json!({"number":"0x10", "hash":s["canonical"]})
            }
        }
        "eth_call" => {
            let selector = format!("0x{}", &keccak256("creator()").to_string()[2..10]);
            if req["params"][0]["data"] == selector {
                json!(format!(
                    "0x{:0>64}",
                    s["creator"].as_str().unwrap().trim_start_matches("0x")
                ))
            } else {
                json!(format!("0x{:064x}", s["count"].as_u64().unwrap()))
            }
        }
        method => panic!("unexpected RPC method {method}"),
    };
    Json(json!({"jsonrpc":"2.0", "id":req["id"], "result":result}))
}

async fn draft(store: &ItemsComponent) -> (CollectionDraft, ItemDraft) {
    let id = Uuid::new_v4();
    let collection: CollectionDraft =
        serde_json::from_value(json!({"id":id, "name":"Transaction collection"})).unwrap();
    store
        .save_collection_draft(OWNER, &collection)
        .await
        .unwrap();
    let mut item: ItemDraft = serde_json::from_value(json!({"id":Uuid::new_v4(), "name":"Hat", "type":"wearable", "collection_id":id,
        "price":"1000000000000000001", "rarity":"rare", "thumbnail":"thumbnail.png",
        "data":{"category":"hat", "representations":[{"mainFile":"model.glb", "contents":["model.glb"], "bodyShapes":["urn:decentraland:off-chain:base-avatars:BaseMale"]}]}})).unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    item.contents = store
        .save_item_files(
            OWNER,
            item.id,
            vec![
                ("model.glb".into(), b"model".to_vec()),
                ("thumbnail.png".into(), b"image".to_vec()),
            ],
        )
        .await
        .unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    (collection, item)
}

#[tokio::test]
async fn frozen_revision_requires_a_matching_finalized_receipt_and_recovers_after_revert() {
    let Some(db) = ScratchSchema::create("CATALYRST_BUILDER_TEST_PG", "builder_transactions").await
    else {
        return;
    };
    for migration in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0003_uploaded_contents.sql"),
        include_str!("../migrations/0004_publication_intents.sql"),
        include_str!("../migrations/0005_publication_signing.sql"),
    ] {
        db.apply_sql(migration).await;
    }
    let store = ItemsComponent::new(db.pool.clone());
    let (collection, mut item) = draft(&store).await;
    let id = collection.id;
    let prepared = store.prepare_publication(OWNER, id).await.unwrap();
    assert!(store
        .begin_publication(OTHER, id, &prepared.revision)
        .await
        .is_err());
    assert!(store
        .begin_publication(OWNER, id, "outdated")
        .await
        .is_err());
    let active = store
        .begin_publication(OWNER, id, &prepared.revision)
        .await
        .unwrap();
    assert_eq!(active.status, "prepared");
    assert_eq!(
        store
            .begin_publication(OWNER, id, &prepared.revision)
            .await
            .unwrap()
            .preparation
            .revision,
        prepared.revision
    );
    assert!(store
        .save_collection_draft(OWNER, &collection)
        .await
        .is_err());
    assert!(store.save_item_draft(OWNER, &item).await.is_err());
    assert!(store
        .save_item_files(
            OWNER,
            item.id,
            vec![("model.glb".into(), b"changed".to_vec())]
        )
        .await
        .is_err());
    item.collection_id = None;
    assert!(
        store.save_item_draft(OWNER, &item).await.is_err(),
        "detaching cannot bypass a publication lock"
    );
    item.collection_id = Some(id);
    assert!(store.cancel_publication(OTHER, id).await.is_err());
    let (first_claim, second_claim) = tokio::join!(
        store.claim_publication(OWNER, id, &prepared.revision),
        store.claim_publication(OWNER, id, &prepared.revision),
    );
    assert_ne!(
        first_claim.is_ok(),
        second_claim.is_ok(),
        "only one tab may request the collection transaction"
    );
    assert_eq!(
        store
            .publication_state(OWNER, id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "signing"
    );
    store.cancel_publication(OWNER, id).await.unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    assert!(
        store
            .begin_publication(OWNER, id, &prepared.revision)
            .await
            .is_err(),
        "an edit invalidates the original review"
    );
    let prepared = store.prepare_publication(OWNER, id).await.unwrap();
    store
        .begin_publication(OWNER, id, &prepared.revision)
        .await
        .unwrap();
    let transaction = json!({"hash":HASH, "from":OWNER, "to":COLLECTION_MANAGER, "value":"0x0", "input":publication_calldata(&prepared).unwrap(), "blockNumber":"0x10", "blockHash":BLOCK});
    let receipt = json!({"transactionHash":HASH, "from":OWNER, "to":COLLECTION_MANAGER, "status":"0x1", "blockNumber":"0x10", "blockHash":BLOCK,
        "logs":[{"address":COLLECTION_FACTORY, "topics":[format!("{:#x}", keccak256("ProxyCreated(address,bytes32)")), format!("0x{:0>64}", &CONTRACT[2..])], "data":prepared.salt}]});
    let state = Arc::new(Mutex::new(
        json!({"tx":transaction, "receipt":null, "finalized":"0x10", "canonical":BLOCK, "creator":OWNER, "count":1}),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:5368")
        .await
        .unwrap();
    let router = Router::new()
        .route("/", post(rpc))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    let chain = PublicationChain {
        http: &http,
        url: "http://127.0.0.1:5368/",
    };
    for (field, value) in [
        ("from", json!(OTHER)),
        ("to", json!(OTHER)),
        ("input", json!("0xdeadbeef")),
        ("value", json!("0x1")),
    ] {
        state.lock().unwrap()["tx"][field] = value;
        assert!(
            chain.verify(&store, OWNER, id, HASH).await.is_err(),
            "accepted wrong transaction {field}"
        );
        assert!(store
            .publication_state(OWNER, id)
            .await
            .unwrap()
            .unwrap()
            .tx_hash
            .is_none());
        state.lock().unwrap()["tx"] = transaction.clone();
    }
    assert!(chain.verify(&store, OTHER, id, HASH).await.is_err());
    assert_eq!(
        chain.verify(&store, OWNER, id, HASH).await.unwrap().status,
        "submitted"
    );
    assert!(store.cancel_publication(OWNER, id).await.is_err());
    state.lock().unwrap()["receipt"] = receipt.clone();
    state.lock().unwrap()["finalized"] = json!("0xf");
    assert_eq!(
        chain.verify(&store, OWNER, id, HASH).await.unwrap().status,
        "submitted"
    );
    state.lock().unwrap()["finalized"] = json!("0x10");
    for (path, value) in [
        ("/canonical", json!(HASH)),
        ("/creator", json!(OTHER)),
        ("/count", json!(2)),
        ("/receipt/logs/0/address", json!(OTHER)),
        ("/receipt/logs/0/data", json!(HASH)),
        ("/receipt/transactionHash", json!(BLOCK)),
    ] {
        let previous = state.lock().unwrap().pointer(path).unwrap().clone();
        *state.lock().unwrap().pointer_mut(path).unwrap() = value;
        assert!(
            chain.verify(&store, OWNER, id, HASH).await.is_err(),
            "accepted invalid receipt {path}"
        );
        assert!(
            !store
                .collection_by_id(&id)
                .await
                .unwrap()
                .unwrap()
                .is_published
        );
        *state.lock().unwrap().pointer_mut(path).unwrap() = previous;
    }
    let published = chain.verify(&store, OWNER, id, HASH).await.unwrap();
    assert_eq!(published.status, "published");
    assert_eq!(published.contract_address.as_deref(), Some(CONTRACT));
    assert_eq!(
        chain.verify(&store, OWNER, id, HASH).await.unwrap().status,
        "published"
    );
    let saved = store.item_draft(OWNER, item.id).await.unwrap();
    assert_eq!(saved["is_published"], true);
    assert_eq!(saved["blockchain_item_id"], "0");
    assert_eq!(
        saved["urn_suffix"],
        format!("urn:decentraland:matic:collections-v2:{CONTRACT}:0")
    );
    assert_eq!(saved["beneficiary"], OWNER);
    assert!(store.save_item_draft(OWNER, &item).await.is_err());
    let (second, second_item) = draft(&store).await;
    let preparation = store.prepare_publication(OWNER, second.id).await.unwrap();
    store
        .begin_publication(OWNER, second.id, &preparation.revision)
        .await
        .unwrap();
    let failed_hash = format!("0x{}", "66".repeat(32));
    {
        let mut s = state.lock().unwrap();
        s["tx"]["hash"] = json!(failed_hash);
        s["tx"]["input"] = json!(publication_calldata(&preparation).unwrap());
        s["receipt"]["transactionHash"] = json!(failed_hash);
        s["receipt"]["status"] = json!("0x0");
    }
    assert_eq!(
        chain
            .verify(&store, OWNER, second.id, &failed_hash)
            .await
            .unwrap()
            .status,
        "reverted"
    );
    store.save_item_draft(OWNER, &second_item).await.unwrap();
    let preparation = store.prepare_publication(OWNER, second.id).await.unwrap();
    assert_eq!(
        store
            .begin_publication(OWNER, second.id, &preparation.revision)
            .await
            .unwrap()
            .status,
        "prepared"
    );
    task.abort();
    db.drop().await;
}

#[tokio::test]
async fn concurrent_edit_and_publication_cannot_both_accept_the_old_revision() {
    let Some(db) =
        ScratchSchema::create("CATALYRST_BUILDER_TEST_PG", "builder_publication_race").await
    else {
        return;
    };
    for migration in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0003_uploaded_contents.sql"),
        include_str!("../migrations/0004_publication_intents.sql"),
        include_str!("../migrations/0005_publication_signing.sql"),
    ] {
        db.apply_sql(migration).await;
    }
    let store = ItemsComponent::new(db.pool.clone());
    let (collection, mut item) = draft(&store).await;
    let prepared = store
        .prepare_publication(OWNER, collection.id)
        .await
        .unwrap();
    item.name = "Changed while paying".into();
    let (begin, edit) = tokio::join!(
        store.begin_publication(OWNER, collection.id, &prepared.revision),
        store.save_item_draft(OWNER, &item)
    );
    assert_ne!(
        begin.is_ok(),
        edit.is_ok(),
        "exactly one operation may accept the reviewed revision"
    );
    let saved = store.item_draft(OWNER, item.id).await.unwrap();
    if begin.is_ok() {
        assert_eq!(saved["name"], "Hat");
        assert_eq!(
            store
                .prepare_publication(OWNER, collection.id)
                .await
                .unwrap()
                .revision,
            prepared.revision
        );
    } else {
        assert_eq!(saved["name"], "Changed while paying");
        assert!(store
            .publication_state(OWNER, collection.id)
            .await
            .unwrap()
            .is_none());
    }
    db.drop().await;
}

#[tokio::test]
async fn waiting_detach_and_upload_recheck_the_committed_publication_lock() {
    let Some(db) =
        ScratchSchema::create("CATALYRST_BUILDER_TEST_PG", "builder_waiting_writer").await
    else {
        return;
    };
    for migration in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0003_uploaded_contents.sql"),
        include_str!("../migrations/0004_publication_intents.sql"),
        include_str!("../migrations/0005_publication_signing.sql"),
    ] {
        db.apply_sql(migration).await;
    }
    let store = ItemsComponent::new(db.pool.clone());
    for detach in [true, false] {
        let (collection, mut item) = draft(&store).await;
        let item_id = item.id;
        let original = store.item_draft(OWNER, item_id).await.unwrap();
        let writer_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&db.url())
            .await
            .unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&writer_pool)
            .await
            .unwrap();
        let mut blocker = db.pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM collections WHERE id=$1 FOR UPDATE")
            .bind(collection.id)
            .fetch_all(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM items WHERE id=$1 FOR UPDATE")
            .bind(item_id)
            .fetch_all(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("UPDATE collections SET publication_pending=true WHERE id=$1")
            .bind(collection.id)
            .execute(&mut *blocker)
            .await
            .unwrap();
        let writer = ItemsComponent::new(writer_pool.clone());
        let attempt = tokio::spawn(async move {
            if detach {
                item.collection_id = None;
                writer.save_item_draft(OWNER, &item).await.map(|_| ())
            } else {
                writer
                    .save_item_files(
                        OWNER,
                        item_id,
                        vec![("model.glb".into(), b"replacement".to_vec())],
                    )
                    .await
                    .map(|_| ())
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT COALESCE(wait_event_type='Lock',false) FROM pg_stat_activity WHERE pid=$1")
                    .bind(writer_pid).fetch_one(&db.pool).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("writer must actually wait on the publication transaction");
        blocker.commit().await.unwrap();
        assert!(
            attempt.await.unwrap().is_err(),
            "waiting writer bypassed the committed publication lock: detach={detach}"
        );
        assert_eq!(store.item_draft(OWNER, item_id).await.unwrap(), original);
        writer_pool.close().await;
    }
    db.drop().await;
}
