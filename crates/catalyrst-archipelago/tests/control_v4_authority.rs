use catalyrst_archipelago::control_v4::{AssignmentAuthority, AssignmentPayload, LaneKey, V4Owner};
#[path = "support/control_v4.rs"]
mod support;
use support::Fixture;

fn owner(address: &str, session: &str, epoch: u64) -> V4Owner {
    V4Owner {
        address: address.into(),
        session: session.into(),
        epoch,
    }
}

fn assignment(room: &str) -> AssignmentPayload {
    AssignmentPayload {
        island_id: room.into(),
        connection_string: format!("livekit:wss://media.example?access_token={room}"),
        from_island_id: None,
        peers: Default::default(),
    }
}

#[tokio::test]
async fn wallets_and_lanes_have_independent_owners_across_replicas() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let replica_a = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let replica_b = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    assert_eq!(replica_a.incarnation(), replica_b.incarnation());

    let alice = owner("0x0000000000000000000000000000000000000001", "session-a", 1);
    let bob = owner("0x0000000000000000000000000000000000000002", "session-b", 1);
    let realm = LaneKey::parse("realm").unwrap();
    let scene = LaneKey::parse("scene:abc").unwrap();
    replica_a
        .claim_lanes(&alice, &[realm.clone(), scene.clone()])
        .await
        .unwrap();
    replica_a
        .publish_assignment(&alice, &realm, assignment("alice-realm"))
        .await
        .unwrap();
    replica_a
        .publish_assignment(&alice, &scene, assignment("alice-scene"))
        .await
        .unwrap();
    replica_b
        .claim_lanes(&bob, std::slice::from_ref(&realm))
        .await
        .unwrap();
    replica_b
        .publish_assignment(&bob, &realm, assignment("bob-realm"))
        .await
        .unwrap();

    assert_eq!(
        replica_b.snapshot(&alice, &realm).await.unwrap().assignment,
        Some(assignment("alice-realm"))
    );
    assert_eq!(
        replica_b.snapshot(&alice, &scene).await.unwrap().assignment,
        Some(assignment("alice-scene"))
    );
    assert_eq!(
        replica_a.snapshot(&bob, &realm).await.unwrap().assignment,
        Some(assignment("bob-realm"))
    );
    drop((replica_a, replica_b));
    fixture.finish().await;
}

#[tokio::test]
async fn replacement_clears_credentials_and_rejects_stale_mutations() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let replica_a = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let replica_b = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let old = owner("0x0000000000000000000000000000000000000001", "session-a", 1);
    let new = owner(&old.address, &old.session, 2);
    let realm = LaneKey::parse("realm").unwrap();
    replica_a
        .claim_lanes(&old, std::slice::from_ref(&realm))
        .await
        .unwrap();
    let previous = replica_a
        .publish_assignment(&old, &realm, assignment("old-room"))
        .await
        .unwrap();
    let next = replica_b
        .claim_lanes(&new, std::slice::from_ref(&realm))
        .await
        .unwrap()
        .remove(0);
    assert!(
        next.assignment.is_none(),
        "a new owner must not receive old credentials"
    );
    assert!(next.assignment_revision > previous.assignment_revision);
    assert!(next.fencing_token > previous.fencing_token);
    assert!(replica_a
        .publish_assignment(&old, &realm, assignment("stale-room"))
        .await
        .is_err());
    assert!(replica_a
        .ack(&old, &realm, previous.assignment_revision)
        .await
        .is_err());
    assert!(replica_a.snapshot(&old, &realm).await.is_err());
    assert!(
        replica_a
            .claim_lanes(&old, std::slice::from_ref(&realm))
            .await
            .is_err(),
        "a late claim cannot retake a newer epoch"
    );
    assert_eq!(replica_b.snapshot(&new, &realm).await.unwrap(), next);
    drop((replica_a, replica_b));
    fixture.finish().await;
}

#[tokio::test]
async fn identical_claim_is_idempotent_and_future_ack_is_rejected() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let subject = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let realm = LaneKey::parse("realm").unwrap();
    authority
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap();
    let published = authority
        .publish_assignment(&subject, &realm, assignment("room"))
        .await
        .unwrap();
    let duplicate = authority
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap()
        .remove(0);
    assert_eq!(published, duplicate);
    assert!(authority
        .ack(&subject, &realm, published.assignment_revision + 1)
        .await
        .is_err());
    assert_eq!(
        authority
            .ack(&subject, &realm, published.assignment_revision)
            .await
            .unwrap(),
        published
    );
    assert!(authority.ack(&subject, &realm, u64::MAX).await.is_err());
    assert!(authority
        .claim_lanes(&owner(&subject.address, "overflow", u64::MAX), &[realm])
        .await
        .is_err());
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn rejected_multi_lane_claim_does_not_partially_change_ownership() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let old = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let new = owner(&old.address, &old.session, 2);
    let realm = LaneKey::parse("realm").unwrap();
    let scene = LaneKey::parse("scene:abc").unwrap();
    authority
        .claim_lanes(&new, std::slice::from_ref(&realm))
        .await
        .unwrap();
    assert!(authority
        .claim_lanes(&old, &[scene.clone(), realm.clone()])
        .await
        .is_err());
    assert!(
        authority.snapshot(&old, &scene).await.is_err(),
        "failed batch must roll back the first lane too"
    );
    assert!(authority.snapshot(&new, &realm).await.is_ok());
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn an_epoch_cannot_be_reused_by_a_different_session() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let first = owner(
        "0x0000000000000000000000000000000000000001",
        "session-one",
        1,
    );
    let second = owner(&first.address, "session-two", first.epoch);
    let realm = LaneKey::parse("realm").unwrap();
    let initial = authority
        .claim_lanes(&first, std::slice::from_ref(&realm))
        .await
        .unwrap();
    assert!(authority
        .claim_lanes(&second, std::slice::from_ref(&realm))
        .await
        .is_err());
    assert_eq!(
        authority.snapshot(&first, &realm).await.unwrap(),
        initial[0]
    );
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn replicas_can_initialize_one_empty_authority_concurrently() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let (first, second) = tokio::join!(
        AssignmentAuthority::pg(fixture.pool.clone()),
        AssignmentAuthority::pg(fixture.pool.clone()),
    );
    let first = first.expect("first replica initializes");
    let second = second.expect("second replica initializes");
    assert_eq!(first.incarnation(), second.incarnation());
    let first_epoch = first.next_connection_epoch().await.unwrap();
    let second_epoch = second.next_connection_epoch().await.unwrap();
    assert!(second_epoch > first_epoch);
    drop((first, second));
    fixture.finish().await;
}

#[tokio::test]
async fn deployments_sharing_a_database_do_not_share_lane_ownership() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let first = AssignmentAuthority::pg_with_namespace(fixture.pool.clone(), "realm-one")
        .await
        .unwrap();
    let second = AssignmentAuthority::pg_with_namespace(fixture.pool.clone(), "realm-two")
        .await
        .unwrap();
    let subject = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let realm = LaneKey::parse("realm").unwrap();
    first
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap();
    let expected = first
        .publish_assignment(&subject, &realm, assignment("first-room"))
        .await
        .unwrap();
    second
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap();
    assert!(second
        .snapshot(&subject, &realm)
        .await
        .unwrap()
        .assignment
        .is_none());
    second
        .publish_assignment(&subject, &realm, assignment("second-room"))
        .await
        .unwrap();
    assert_eq!(first.snapshot(&subject, &realm).await.unwrap(), expected);
    drop((first, second));
    fixture.finish().await;
}

#[tokio::test]
async fn unavailable_database_cannot_mint_a_fallback_epoch() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    fixture.pool.close().await;
    assert!(authority.next_connection_epoch().await.is_err());
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn assignment_updates_keep_the_owner_fence_and_reject_oversized_payloads() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let subject = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let realm = LaneKey::parse("realm").unwrap();
    let claimed = authority
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap()
        .remove(0);
    let published = authority
        .publish_assignment(&subject, &realm, assignment("room"))
        .await
        .unwrap();
    assert_eq!(published.fencing_token, claimed.fencing_token);
    assert!(published.assignment_revision > claimed.assignment_revision);
    assert!(authority
        .publish_assignment(&subject, &realm, assignment(&"x".repeat(65536)))
        .await
        .is_err());
    assert_eq!(
        authority.snapshot(&subject, &realm).await.unwrap(),
        published
    );
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn invalid_stored_assignments_are_not_silently_treated_as_empty() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let subject = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let realm = LaneKey::parse("realm").unwrap();
    authority
        .claim_lanes(&subject, std::slice::from_ref(&realm))
        .await
        .unwrap();
    sqlx::query("UPDATE archipelago_v4_assignments SET assignment_json = '{}'::jsonb WHERE owner_address = $1")
        .bind(&subject.address)
        .execute(&fixture.pool).await.unwrap();
    assert!(authority.snapshot(&subject, &realm).await.is_err());
    drop(authority);
    fixture.finish().await;
}

#[tokio::test]
async fn duplicate_lanes_and_zero_epochs_are_rejected_before_admission() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let subject = owner("0x0000000000000000000000000000000000000001", "session", 1);
    let realm = LaneKey::parse("realm").unwrap();
    assert!(authority
        .claim_lanes(&subject, &[realm.clone(), realm.clone()])
        .await
        .is_err());
    assert!(authority
        .claim_lanes(
            &owner(&subject.address, &subject.session, 0),
            std::slice::from_ref(&realm)
        )
        .await
        .is_err());
    assert!(authority.snapshot(&subject, &realm).await.is_err());
    drop(authority);
    fixture.finish().await;
}

async fn renewal_of(pool: &sqlx::PgPool, address: &str, lane: &str) -> String {
    sqlx::query_scalar(
        "SELECT CASE
            WHEN updated_at = '-infinity' THEN 'released'
            WHEN updated_at > now() - interval '1 minute' THEN 'fresh'
            ELSE 'aged'
         END
         FROM archipelago_v4_assignments
         WHERE owner_address = $1 AND lane_key = $2",
    )
    .bind(address)
    .bind(lane)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn renewal_and_release_reach_only_the_rows_a_socket_still_owns() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let address = "0x0000000000000000000000000000000000000003";
    let older = owner(address, "session-a", 1);
    let newer = owner(address, "session-a", 2);
    let realm = LaneKey::parse("realm").unwrap();
    let scene = LaneKey::parse("scene:abc").unwrap();
    authority
        .claim_lanes(&older, &[realm.clone(), scene.clone()])
        .await
        .unwrap();
    authority.claim_lanes(&newer, &[realm]).await.unwrap();
    sqlx::query(
        "UPDATE archipelago_v4_assignments SET updated_at = now() - interval '70 seconds'
         WHERE owner_address = $1",
    )
    .bind(address)
    .execute(&fixture.pool)
    .await
    .unwrap();

    authority.renew(&older).await.unwrap();
    assert_eq!(
        renewal_of(&fixture.pool, address, "scene:abc").await,
        "fresh"
    );
    assert_eq!(renewal_of(&fixture.pool, address, "realm").await, "aged");

    authority.release(&older).await.unwrap();
    assert_eq!(
        renewal_of(&fixture.pool, address, "scene:abc").await,
        "released"
    );
    assert_eq!(renewal_of(&fixture.pool, address, "realm").await, "aged");

    authority.renew(&newer).await.unwrap();
    assert_eq!(renewal_of(&fixture.pool, address, "realm").await, "fresh");
    fixture.finish().await;
}

#[tokio::test]
async fn expired_or_released_owners_cannot_read_renew_publish_ack_or_reclaim() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let realm = LaneKey::parse("realm").unwrap();
    for released in [false, true] {
        let old = owner(if released { "released" } else { "expired" }, "session", 1);
        authority
            .claim_lanes(&old, std::slice::from_ref(&realm))
            .await
            .unwrap();
        let assignment = authority
            .publish_assignment(&old, &realm, assignment("room"))
            .await
            .unwrap();
        if released {
            authority.release(&old).await.unwrap();
        } else {
            sqlx::query("UPDATE archipelago_v4_assignments SET updated_at = clock_timestamp() - interval '91 seconds' WHERE owner_address = $1")
                .bind(&old.address).execute(&fixture.pool).await.unwrap();
        }
        let before: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(a) FROM archipelago_v4_assignments a WHERE owner_address = $1",
        )
        .bind(&old.address)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
        assert!(authority.snapshot(&old, &realm).await.is_err());
        assert!(authority.renew(&old).await.is_err());
        assert!(authority
            .publish_assignment(&old, &realm, assignment.assignment.unwrap())
            .await
            .is_err());
        assert!(authority
            .ack(&old, &realm, assignment.assignment_revision)
            .await
            .is_err());
        assert!(authority
            .claim_lanes(&old, std::slice::from_ref(&realm))
            .await
            .is_err());
        let after: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(a) FROM archipelago_v4_assignments a WHERE owner_address = $1",
        )
        .bind(&old.address)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        let next = owner(&old.address, "new-session", 2);
        let claimed = authority
            .claim_lanes(&next, std::slice::from_ref(&realm))
            .await
            .unwrap()
            .remove(0);
        assert!(claimed.assignment.is_none());
        assert!(claimed.fencing_token > assignment.fencing_token);
    }
    fixture.finish().await;
}

#[tokio::test]
async fn an_owner_that_expires_while_waiting_for_a_lock_is_not_revived() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let authority = AssignmentAuthority::pg(fixture.pool.clone()).await.unwrap();
    let realm = LaneKey::parse("realm").unwrap();
    for operation in 0..5 {
        let owner = owner(&format!("lock-{operation}"), "session", 1);
        authority
            .claim_lanes(&owner, std::slice::from_ref(&realm))
            .await
            .unwrap();
        sqlx::query("UPDATE archipelago_v4_assignments SET updated_at = clock_timestamp() - interval '89.8 seconds' WHERE owner_address = $1")
            .bind(&owner.address).execute(&fixture.pool).await.unwrap();
        let mut lock = fixture.pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM archipelago_v4_assignments WHERE owner_address = $1 FOR UPDATE")
            .bind(&owner.address)
            .execute(&mut *lock)
            .await
            .unwrap();
        let waiting_authority = authority.clone();
        let waiting_owner = owner.clone();
        let waiting_realm = realm.clone();
        let pending = tokio::spawn(async move {
            match operation {
                0 => waiting_authority.renew(&waiting_owner).await,
                1 => waiting_authority
                    .publish_assignment(&waiting_owner, &waiting_realm, assignment("room"))
                    .await
                    .map(|_| ()),
                2 => waiting_authority
                    .ack(&waiting_owner, &waiting_realm, 1)
                    .await
                    .map(|_| ()),
                3 => waiting_authority
                    .claim_lanes(&waiting_owner, std::slice::from_ref(&waiting_realm))
                    .await
                    .map(|_| ()),
                _ => waiting_authority.release(&waiting_owner).await,
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            !pending.is_finished(),
            "operation must wait for the row lock"
        );
        lock.commit().await.unwrap();
        assert!(pending.await.unwrap().is_err());
        assert!(authority.snapshot(&owner, &realm).await.is_err());
    }
    fixture.finish().await;
}
