use alloy::{
    primitives::{address, B256, U256},
    signers::{local::PrivateKeySigner, SignerSync},
    sol_types::{eip712_domain, sol, SolStruct},
};
use catalyrst_builder::ports::{
    drafts::{CollectionDraft, ItemDraft},
    items::ItemsComponent,
    linked_publication::LinkedPublicationCheque,
};
use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::{json, Value};
use uuid::Uuid;

const OWNER: &str = "0x19e7e376e7c213b7e7e7e46cc70a5dd086daff2a";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const PROVIDER: &str = "urn:decentraland:matic:collections-thirdparty:provider";

sol! {
    #![sol(alloy_sol_types = alloy::sol_types)]
    struct ConsumeSlots {
        string thirdPartyId;
        uint256 qty;
        bytes32 salt;
    }
}

fn authorization(salt: &str) -> LinkedPublicationCheque {
    let signer: PrivateKeySigner = "11".repeat(32).parse().unwrap();
    let domain = eip712_domain! {
        name: "Decentraland Third Party Registry",
        version: "1",
        verifying_contract: address!("1C436C1EFb4608dFfDC8bace99d2B03c314f3348"),
        salt: B256::from(U256::from(137).to_be_bytes::<32>()),
    };
    let data = ConsumeSlots {
        thirdPartyId: PROVIDER.into(),
        qty: U256::from(1),
        salt: salt.parse().unwrap(),
    };
    LinkedPublicationCheque {
        qty: 1,
        salt: salt.into(),
        signature: signer
            .sign_hash_sync(&data.eip712_signing_hash(&domain))
            .unwrap()
            .to_string(),
    }
}

#[tokio::test]
async fn linked_publication_freezes_content_and_recovers_the_same_authorization() {
    let Some(db) =
        ScratchSchema::create("CATALYRST_BUILDER_TEST_PG", "builder_linked_publication").await
    else {
        return;
    };
    for migration in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0003_uploaded_contents.sql"),
        include_str!("../migrations/0004_publication_intents.sql"),
        include_str!("../migrations/0005_publication_signing.sql"),
        include_str!("../migrations/0006_linked_publications.sql"),
        include_str!("../migrations/0007_linked_publication_receipts.sql"),
    ] {
        db.apply_sql(migration).await;
    }
    let store = ItemsComponent::new(db.pool.clone());
    let id = Uuid::new_v4();
    let mut collection: CollectionDraft = serde_json::from_value(json!({
        "id": id, "name": "Linked hats", "urn": format!("{PROVIDER}:{id}")
    }))
    .unwrap();
    store
        .save_collection_draft(OWNER, &collection)
        .await
        .unwrap();
    assert!(store.prepare_linked_publication(OWNER, id).await.is_err());
    let mut item: ItemDraft = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "Hat", "type": "wearable", "collection_id": id,
        "thumbnail": "thumbnail.png", "data": {"category": "hat", "representations": [{
            "mainFile": "hat.glb", "contents": ["hat.glb"], "bodyShapes": ["urn:decentraland:off-chain:base-avatars:BaseMale"]
        }]}, "metrics": {"triangles": 100}
    })).unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    assert!(store.prepare_linked_publication(OWNER, id).await.is_err());
    item.contents = store
        .save_item_files(
            OWNER,
            item.id,
            vec![
                ("hat.glb".into(), b"model".to_vec()),
                ("thumbnail.png".into(), b"thumbnail".to_vec()),
            ],
        )
        .await
        .unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    let stale = store.prepare_linked_publication(OWNER, id).await.unwrap();
    collection.name = "Updated hats".into();
    store
        .save_collection_draft(OWNER, &collection)
        .await
        .unwrap();
    assert!(store
        .begin_linked_publication(OWNER, id, &stale.revision)
        .await
        .is_err());
    assert!(store.prepare_linked_publication(OTHER, id).await.is_err());
    assert!(store.prepare_publication(OWNER, id).await.is_err());
    let prepared = store.prepare_linked_publication(OWNER, id).await.unwrap();
    assert_eq!(prepared.item_ids, vec![item.id.to_string()]);
    let (a, b) = tokio::join!(
        store.begin_linked_publication(OWNER, id, &prepared.revision),
        store.begin_linked_publication(OWNER, id, &prepared.revision)
    );
    let first = a.unwrap();
    assert_eq!(first.salt, b.unwrap().salt);
    assert_eq!(first.status, "prepared");
    assert!(store
        .save_collection_draft(OWNER, &collection)
        .await
        .is_err());
    assert!(store.save_item_draft(OWNER, &item).await.is_err());
    assert!(store
        .save_item_files(
            OWNER,
            item.id,
            vec![("hat.glb".into(), b"changed".to_vec())]
        )
        .await
        .is_err());
    store.cancel_publication(OWNER, id).await.unwrap();
    assert!(
        store.save_item_draft(OWNER, &item).await.is_err(),
        "standard cancellation must not unlock linked publication"
    );
    assert!(store.cancel_linked_publication(OTHER, id).await.is_err());
    store.cancel_linked_publication(OWNER, id).await.unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    let prepared = store.prepare_linked_publication(OWNER, id).await.unwrap();
    let second = store
        .begin_linked_publication(OWNER, id, &prepared.revision)
        .await
        .unwrap();
    assert_ne!(first.salt, second.salt);
    let cheque = authorization(&second.salt);
    assert!(store
        .authorize_linked_publication(OWNER, id, &cheque)
        .await
        .is_err());
    let claimed = store
        .claim_linked_publication(OWNER, id, &prepared.revision)
        .await
        .unwrap();
    assert_eq!(claimed.status, "signing");
    assert_eq!(
        store
            .claim_linked_publication(OWNER, id, &prepared.revision)
            .await
            .unwrap()
            .salt,
        second.salt
    );
    assert!(store
        .claim_linked_publication(OTHER, id, &prepared.revision)
        .await
        .is_err());
    assert!(store
        .claim_linked_publication(OWNER, id, "other revision")
        .await
        .is_err());
    assert!(store.cancel_linked_publication(OWNER, id).await.is_err());
    let mut invalid = cheque.clone();
    invalid.qty = 2;
    assert!(store
        .authorize_linked_publication(OWNER, id, &invalid)
        .await
        .is_err());
    invalid = cheque.clone();
    invalid.salt = first.salt;
    assert!(store
        .authorize_linked_publication(OWNER, id, &invalid)
        .await
        .is_err());
    invalid = cheque.clone();
    invalid.signature = format!("0x{}", "00".repeat(65));
    assert!(store
        .authorize_linked_publication(OWNER, id, &invalid)
        .await
        .is_err());
    assert_eq!(
        store
            .linked_publication_state(OWNER, id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "signing"
    );
    assert!(store
        .authorize_linked_publication(OTHER, id, &cheque)
        .await
        .is_err());
    let authorized = store
        .authorize_linked_publication(OWNER, id, &cheque)
        .await
        .unwrap();
    assert_eq!(authorized.status, "authorized");
    let reopened = ItemsComponent::new(db.pool.clone());
    let persisted = reopened
        .linked_publication_state(OWNER, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.cheque, Some(cheque.clone()));
    assert_eq!(persisted.preparation.revision, prepared.revision);
    assert_eq!(persisted.salt, second.salt);
    assert!(reopened
        .linked_publication_state(OTHER, id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        reopened
            .authorize_linked_publication(OWNER, id, &cheque)
            .await
            .unwrap()
            .status,
        "authorized"
    );
    assert!(reopened.cancel_linked_publication(OWNER, id).await.is_err());
    assert!(reopened.save_item_draft(OWNER, &item).await.is_err());
    let snapshot: Value = sqlx::query_scalar(
        "SELECT snapshot FROM linked_collection_publications WHERE collection_id=$1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(snapshot["items"][0]["contents"], json!(item.contents));
    assert_eq!(snapshot["collection"]["name"], "Updated hats");
    let published: bool = sqlx::query_scalar("SELECT is_published FROM collections WHERE id=$1")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(!published, "authorization is not proof of publication");
    db.drop().await;
}
