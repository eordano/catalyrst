use catalyrst_builder::ports::drafts::{CollectionDraft, ItemDraft};
use catalyrst_builder::ports::items::ItemsComponent;
use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::json;
use uuid::Uuid;

const OWNER: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";

#[tokio::test]
async fn drafts_and_files_persist_with_owner_and_publication_guards() {
    let Some(db) = ScratchSchema::create("CATALYRST_BUILDER_TEST_PG", "builder_drafts").await
    else {
        return;
    };
    db.apply_sql(include_str!("../migrations/0001_initial.sql"))
        .await;
    db.apply_sql(include_str!("../migrations/0003_uploaded_contents.sql"))
        .await;
    db.apply_sql(include_str!("../migrations/0004_publication_intents.sql"))
        .await;
    db.apply_sql(include_str!("../migrations/0005_publication_signing.sql"))
        .await;
    let store = ItemsComponent::new(db.pool.clone());
    let collection = Uuid::new_v4();
    let draft: CollectionDraft =
        serde_json::from_value(json!({ "id": collection, "name": "Collection draft" })).unwrap();
    let saved = store.save_collection_draft(OWNER, &draft).await.unwrap();
    assert_eq!(saved.name, "Collection draft");
    assert!(!saved.is_published);
    assert!(saved.contract_address.is_none());
    assert!(store.save_collection_draft(OTHER, &draft).await.is_err());
    assert!(store.collection_drafts(OTHER).await.unwrap().is_empty());
    assert_eq!(store.collection_drafts(OWNER).await.unwrap().len(), 1);

    let id = Uuid::new_v4();
    let mut item: ItemDraft = serde_json::from_value(json!({
        "id": id, "name": "My wearable", "collection_id": collection,
        "type": "wearable", "data": { "category": "upper_body" }, "rarity": "common", "price": "0",
        "metrics": { "triangles": 42, "meshes": 1 }
    }))
    .unwrap();
    store.save_item_draft(OWNER, &item).await.unwrap();
    assert!(store.save_item_draft(OTHER, &item).await.is_err());
    assert!(store.item_draft(OTHER, id).await.is_err());
    let bytes = b"actual model bytes".to_vec();
    let files = store
        .save_item_files(OWNER, id, vec![("model.glb".into(), bytes.clone())])
        .await
        .unwrap();
    let hash = files["model.glb"].clone();
    assert!(catalyrst_hashing::verify_hash(&bytes, &hash));
    assert_eq!(
        store.uploaded_content(&hash).await.unwrap(),
        Some(bytes.clone())
    );
    assert!(store.uploaded_content_exists(&hash).await.unwrap());
    assert!(store
        .save_item_files(OTHER, id, vec![("model.glb".into(), b"overwrite".to_vec())])
        .await
        .is_err());
    item.name = "Edited wearable".into();
    item.metrics = None;
    item.contents = files;
    store.save_item_draft(OWNER, &item).await.unwrap();
    let reopened = ItemsComponent::new(db.pool.clone())
        .item_draft(OWNER, id)
        .await
        .unwrap();
    assert_eq!(
        store.item_drafts(OWNER).await.unwrap()[0]["id"],
        id.to_string()
    );
    assert!(store.item_drafts(OTHER).await.unwrap().is_empty());
    assert_eq!(reopened["name"], "Edited wearable");
    assert_eq!(reopened["metrics"], json!({ "triangles": 42, "meshes": 1 }));
    item.metrics = Some(json!({ "triangles": -1 }));
    assert!(store.save_item_draft(OWNER, &item).await.is_err());
    item.metrics = None;
    assert_eq!(reopened["data"]["category"], "upper_body");
    assert_eq!(reopened["contents"]["model.glb"], hash);

    item.contents.insert(
        "missing.glb".into(),
        catalyrst_hashing::hash_bytes_v1(b"not uploaded"),
    );
    assert!(store.save_item_draft(OWNER, &item).await.is_err());
    assert_eq!(
        store.item_draft(OWNER, id).await.unwrap(),
        reopened,
        "invalid writes roll back metadata and content links"
    );
    item.contents.remove("missing.glb");
    sqlx::query("UPDATE collections SET is_published=true WHERE id=$1")
        .bind(collection)
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(store.save_collection_draft(OWNER, &draft).await.is_err());
    assert!(store
        .save_item_files(OWNER, id, vec![("model.glb".into(), b"overwrite".to_vec())])
        .await
        .is_err());
    item.collection_id = None;
    assert!(
        store.save_item_draft(OWNER, &item).await.is_err(),
        "cannot detach an item to bypass publication protection"
    );
    assert_eq!(store.uploaded_content(&hash).await.unwrap(), Some(bytes));
    let linked_id = Uuid::new_v4();
    let provider = "urn:decentraland:matic:collections-thirdparty:example";
    let urn = format!("{provider}:{linked_id}");
    let mut linked: CollectionDraft =
        serde_json::from_value(json!({ "id": linked_id, "name": "Linked draft", "urn": urn }))
            .unwrap();
    let saved = store.save_collection_draft(OWNER, &linked).await.unwrap();
    assert_eq!(saved.urn.as_deref(), Some(urn.as_str()));
    assert_eq!(saved.third_party_id.as_deref(), Some(provider));
    assert!(!saved.is_published);
    linked.name = "Renamed linked draft".into();
    assert_eq!(
        store
            .save_collection_draft(OWNER, &linked)
            .await
            .unwrap()
            .name,
        linked.name
    );
    assert!(store.save_collection_draft(OTHER, &linked).await.is_err());
    linked.urn = None;
    assert!(store.save_collection_draft(OWNER, &linked).await.is_err());
    linked.urn = Some(format!(
        "urn:decentraland:matic:collections-thirdparty:other:{linked_id}"
    ));
    assert!(store.save_collection_draft(OWNER, &linked).await.is_err());
    assert_eq!(
        store
            .collection_by_id(&linked_id)
            .await
            .unwrap()
            .unwrap()
            .urn_suffix
            .as_deref(),
        Some(urn.as_str())
    );
    linked.urn = Some("urn:decentraland:matic:collections-v2:forged".into());
    assert!(linked.validate(linked_id, OWNER).is_err());
    linked.urn = Some(format!("{provider}:bad suffix"));
    assert!(linked.validate(linked_id, OWNER).is_err());
    db.drop().await;
}
