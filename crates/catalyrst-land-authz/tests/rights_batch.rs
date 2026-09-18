use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_land_authz::events::{KIND_APPROVED_FOR_ALL, KIND_UPDATE_MANAGER};
use catalyrst_land_authz::LandAuthzStore;
use catalyrst_validator::squid_checker::LandOperatorResolver;

const LAND: &str = "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d";
const ESTATE: &str = "0x959e104e1a4db6317fa58f8295f586e1a978c297";
const ALICE: &str = "0xaaaa000000000000000000000000000000000001";
const BOB: &str = "0xbbbb000000000000000000000000000000000002";
const CARL: &str = "0xcccc000000000000000000000000000000000003";
const DANA: &str = "0xdddd000000000000000000000000000000000004";

const DDL: &str = "
CREATE TABLE squid_marketplace.parcel (
    id TEXT PRIMARY KEY, x INTEGER NOT NULL, y INTEGER NOT NULL, token_id NUMERIC,
    owner_id TEXT, estate_id TEXT);
CREATE TABLE squid_marketplace.estate (id TEXT PRIMARY KEY, token_id NUMERIC, owner_id TEXT);
CREATE TABLE land_authz.token_right (
    token_address TEXT NOT NULL, token_id NUMERIC NOT NULL, x INTEGER, y INTEGER,
    operator TEXT, update_operator TEXT, updated_block BIGINT NOT NULL, updated_log INTEGER NOT NULL,
    PRIMARY KEY (token_address, token_id));
CREATE TABLE land_authz.account_right (
    token_address TEXT NOT NULL, account TEXT NOT NULL, operator TEXT NOT NULL, kind TEXT NOT NULL,
    is_approved BOOLEAN NOT NULL, updated_block BIGINT NOT NULL, updated_log INTEGER NOT NULL,
    PRIMARY KEY (token_address, account, operator, kind));
INSERT INTO squid_marketplace.estate VALUES ('estate-1', 500, '0xAAAA000000000000000000000000000000000001-ETHEREUM');
INSERT INTO squid_marketplace.parcel VALUES
    ('p-1', 1, 1, 11, '0xBBBB000000000000000000000000000000000002-ETHEREUM', NULL),
    ('p-2', 2, 2, 12, '0xbbbb000000000000000000000000000000000002-ETHEREUM', 'estate-1'),
    ('p-3', 3, 3, 13, '0xbbbb000000000000000000000000000000000002-ETHEREUM', 'estate-1'),
    ('p-4', 4, 4, 14, NULL, NULL);
INSERT INTO land_authz.token_right VALUES
    ('0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d', 11, 1, 1, '0xCCCC000000000000000000000000000000000003', NULL, 1, 0),
    ('0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d', 13, 3, 3, NULL, '0xdddd000000000000000000000000000000000004', 1, 1),
    ('0x959e104e1a4db6317fa58f8295f586e1a978c297', 500, NULL, NULL, '0xcccc000000000000000000000000000000000003', '0xcccc000000000000000000000000000000000003', 1, 2);
INSERT INTO land_authz.account_right VALUES
    ('0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d', '0xbbbb000000000000000000000000000000000002', '0xdddd000000000000000000000000000000000004', 'update_manager', true, 1, 3),
    ('0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d', '0xbbbb000000000000000000000000000000000002', '0xcccc000000000000000000000000000000000003', 'update_manager', true, 1, 4),
    ('0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d', '0xbbbb000000000000000000000000000000000002', '0xaaaa000000000000000000000000000000000001', 'update_manager', false, 1, 5),
    ('0x959e104e1a4db6317fa58f8295f586e1a978c297', '0xaaaa000000000000000000000000000000000001', '0xdddd000000000000000000000000000000000004', 'approved_for_all', true, 1, 6);
";

async fn setup() -> Option<(ScratchDb, LandAuthzStore)> {
    let db = ScratchDb::builder("CATALYRST_LAND_AUTHZ_TEST_PG", "laz_rights")
        .schemas(["land_authz", "squid_marketplace"])
        .build()
        .await?;
    db.apply_sql(DDL).await;
    let store = LandAuthzStore::new(db.pool.clone());
    Some((db, store))
}

#[tokio::test]
async fn rights_batch_matches_the_per_leg_reading() {
    let Some((db, store)) = setup().await else {
        return;
    };

    let parcels = [(1, 1), (2, 2), (9, 9), (3, 3), (4, 4), (1, 1)];
    let batch = store.rights_batch(&parcels).await.unwrap();
    assert_eq!(batch.len(), parcels.len(), "positional by input");

    for (i, &(x, y)) in parcels.iter().enumerate() {
        let subject = store.parcel_subject(x, y).await.unwrap();
        let got = batch[i].as_ref().map(|r| r.subject.clone());
        assert_eq!(got, subject, "subject of ({x},{y})");
        let Some(subject) = subject else {
            continue;
        };
        let rights = batch[i].as_ref().unwrap();
        let managers = store
            .account_grants(&subject.registry, &subject.owner, KIND_UPDATE_MANAGER)
            .await
            .unwrap();
        let approved = store
            .account_grants(&subject.registry, &subject.owner, KIND_APPROVED_FOR_ALL)
            .await
            .unwrap();
        assert_eq!(rights.operators.update_managers, managers, "({x},{y})");
        assert_eq!(rights.operators.approved_for_all, approved, "({x},{y})");
        assert_eq!(rights.operators.operator, subject.operator);
        assert_eq!(rights.operators.update_operator, subject.update_operator);
    }

    let p1 = batch[0].as_ref().unwrap();
    assert_eq!(p1.subject.owner, BOB);
    assert_eq!(p1.subject.operator.as_deref(), Some(CARL));
    assert_eq!(p1.operators.update_managers, vec![CARL, DANA]);
    assert!(p1.operators.approved_for_all.is_empty());

    let p2 = batch[1].as_ref().unwrap();
    assert!(p2.subject.belongs_to_estate);
    assert_eq!(p2.subject.owner, ALICE);
    assert_eq!(p2.subject.registry, ESTATE);
    assert_eq!(p2.subject.operator.as_deref(), Some(CARL));
    assert_eq!(p2.subject.update_operator.as_deref(), Some(CARL));
    assert_eq!(p2.operators.approved_for_all, vec![DANA]);
    assert!(p2.operators.update_managers.is_empty());

    assert!(batch[2].is_none(), "unindexed parcel");
    let p3 = batch[3].as_ref().unwrap();
    assert_eq!(
        p3.subject.update_operator.as_deref(),
        Some(DANA),
        "a per-parcel update operator wins over the estate's"
    );
    let p4 = batch[4].as_ref().unwrap();
    assert_eq!(p4.subject.owner, "");
    assert_eq!(p4.subject.registry, LAND);

    let via_trait = LandOperatorResolver::operators_batch(&store, &parcels).await;
    assert_eq!(via_trait.len(), parcels.len());
    assert!(via_trait[2].as_ref().unwrap().is_none());
    assert_eq!(
        via_trait[0]
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .update_managers,
        vec![CARL, DANA]
    );
    assert!(store.rights_batch(&[]).await.unwrap().is_empty());

    db.drop().await;
}

#[tokio::test]
async fn update_operator_page_carries_the_total() {
    let Some((db, store)) = setup().await else {
        return;
    };
    sqlx::query(
        "INSERT INTO land_authz.token_right VALUES ($1, 12, 2, 2, NULL, $2, 2, 0), ($1, 14, 4, 4, NULL, $2, 2, 1)",
    )
    .bind(LAND)
    .bind(DANA)
    .execute(&db.pool)
    .await
    .unwrap();

    let all = store.parcels_with_update_operator(DANA).await.unwrap();
    assert_eq!(all.len(), 3);

    let (page, total) = store
        .parcels_with_update_operator_page(DANA, 2, 0)
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(page, all[..2].to_vec());

    let (page, total) = store
        .parcels_with_update_operator_page(DANA, 2, 2)
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(page, all[2..].to_vec());

    let (page, total) = store
        .parcels_with_update_operator_page(DANA, 2, 10)
        .await
        .unwrap();
    assert_eq!(
        (page.len(), total),
        (0, 3),
        "past the end still reports the total"
    );

    let (page, total) = store
        .parcels_with_update_operator_page(ALICE, 2, 0)
        .await
        .unwrap();
    assert_eq!((page.len(), total), (0, 0));

    db.drop().await;
}
