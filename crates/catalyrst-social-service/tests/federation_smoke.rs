use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{RateLimiter, Signed, TypedMessage};
use catalyrst_social_service::rest::fed::apply;
use catalyrst_social_service::rest::fed::authority::{load_role, require_min_role, Role};
use catalyrst_social_service::rest::fed::ids::{community_id_hex, community_uuid_from_hex};
use catalyrst_social_service::rest::fed::messages::{
    CommunityBan, CommunityCreate, CommunityJoin, CommunityPost, CommunityRole,
};
use catalyrst_social_service::rest::fed::replay::Replay;
use rand::Rng;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_SOCIAL_SERVICE_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/communities".into()))
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    format!("test_fed_{}", hex::encode(b))
}

async fn setup() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    apply_migration(&pool, include_str!("../migrations/0001_initial.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0002_federation.sql")).await;

    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_block_comments(sql);
    let mut buf = String::new();
    let mut in_func = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.contains("$$ LANGUAGE plpgsql;") {
            in_func = false;
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
            continue;
        }
        if trimmed.contains("CREATE OR REPLACE FUNCTION") || trimmed.contains("CREATE FUNCTION") {
            in_func = true;
        }
        if !in_func && trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
            .execute(pool)
            .await
            .expect("trailing sql");
    }
}

fn strip_block_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn mk_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    PrivateKeySigner::from_slice(&key).expect("wallet from bytes")
}

fn addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

fn rand_nonce() -> [u8; 16] {
    let mut n = [0u8; 16];
    rand::rng().fill_bytes(&mut n);
    n
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn sign<T: TypedMessage>(
    wallet: &PrivateKeySigner,
    message: T,
    domain: Eip712Domain,
    nonce: [u8; 16],
    signed_at: i64,
) -> Signed<T> {
    let mut signed = Signed {
        domain,
        message,
        nonce,
        signed_at,
        signature: String::new(),
    };
    let hash = signed.hash();
    let sig = wallet.sign_message(&hash).await.unwrap();
    signed.signature = sig.to_string();
    signed
}

#[tokio::test]
async fn create_join_role_ban_post_flow() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping create_join_role_ban_post_flow: no postgres reachable");
        return;
    };
    let domain = domains::communities();

    let w1 = mk_wallet(11);
    let w2 = mk_wallet(22);
    let w3 = mk_wallet(33);
    let w4 = mk_wallet(44);
    let w5 = mk_wallet(55);

    let t0 = now();
    let create = sign(
        &w1,
        CommunityCreate {
            name: "TestCommunity".into(),
            description: "for tests".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        t0,
    )
    .await;
    create.verify(&addr(&w1), t0).expect("create verifies");

    let applied = apply::apply_create(&pool, &create, &addr(&w1))
        .await
        .expect("create persists");
    let cid = applied.community_id.clone();
    assert_eq!(
        load_role(&pool, &cid, &addr(&w1)).await.unwrap(),
        Role::Owner
    );

    let join = sign(
        &w2,
        CommunityJoin {
            community_id: cid.clone(),
        },
        domain.clone(),
        rand_nonce(),
        t0 + 1,
    )
    .await;
    join.verify(&addr(&w2), t0 + 1).unwrap();
    apply::apply_join(&pool, &join, &addr(&w2))
        .await
        .expect("join");
    assert_eq!(
        load_role(&pool, &cid, &addr(&w2)).await.unwrap(),
        Role::Member
    );

    let bad_role_actor = require_min_role(&pool, &cid, &addr(&w3), Role::Owner).await;
    assert!(bad_role_actor.is_err(), "w3 cannot grant role");

    let grant = sign(
        &w1,
        CommunityRole {
            community_id: cid.clone(),
            target: addr(&w2),
            role: "moderator".into(),
        },
        domain.clone(),
        rand_nonce(),
        t0 + 2,
    )
    .await;
    grant.verify(&addr(&w1), t0 + 2).unwrap();
    require_min_role(&pool, &cid, &addr(&w1), Role::Owner)
        .await
        .expect("w1 is owner");
    apply::apply_role(&pool, &grant, &addr(&w1))
        .await
        .expect("grant");
    assert_eq!(load_role(&pool, &cid, &addr(&w2)).await.unwrap(), Role::Mod);

    let join4 = sign(
        &w4,
        CommunityJoin {
            community_id: cid.clone(),
        },
        domain.clone(),
        rand_nonce(),
        t0 + 3,
    )
    .await;
    apply::apply_join(&pool, &join4, &addr(&w4)).await.unwrap();

    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES (gen_random_uuid(), $1, $2, 'pending', 'invite')",
    )
    .bind(community_uuid_from_hex(&cid))
    .bind(addr(&w4))
    .execute(&pool)
    .await
    .expect("seed pending request for ban target");

    let ban = sign(
        &w2,
        CommunityBan {
            community_id: cid.clone(),
            target: addr(&w4),
            reason: Some("spam".into()),
        },
        domain.clone(),
        rand_nonce(),
        t0 + 4,
    )
    .await;
    ban.verify(&addr(&w2), t0 + 4).unwrap();
    let _ = require_min_role(&pool, &cid, &addr(&w2), Role::Mod)
        .await
        .expect("w2 is moderator >= mod");
    apply::apply_ban(&pool, &ban, &addr(&w2))
        .await
        .expect("ban");
    assert_eq!(
        load_role(&pool, &cid, &addr(&w4)).await.unwrap(),
        Role::Banned
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM community_requests \
         WHERE community_id = $1 AND member_address = $2 AND status = 'pending'",
    )
    .bind(community_uuid_from_hex(&cid))
    .bind(addr(&w4))
    .fetch_one(&pool)
    .await
    .expect("pending probe");
    assert_eq!(
        pending, 0,
        "apply_ban must remove the target's pending requests"
    );

    let must_be_member = require_min_role(&pool, &cid, &addr(&w5), Role::Member).await;
    assert!(must_be_member.is_err(), "w5 not a member");

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn moderator_wire_role_persists_as_mod() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping moderator_wire_role_persists_as_mod: no postgres reachable");
        return;
    };
    let domain = domains::communities();
    let owner = mk_wallet(60);
    let member = mk_wallet(61);

    let create = sign(
        &owner,
        CommunityCreate {
            name: "RoleNorm".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let cid = apply::apply_create(&pool, &create, &addr(&owner))
        .await
        .unwrap()
        .community_id;

    let join = sign(
        &member,
        CommunityJoin {
            community_id: cid.clone(),
        },
        domain.clone(),
        rand_nonce(),
        now() + 1,
    )
    .await;
    apply::apply_join(&pool, &join, &addr(&member))
        .await
        .unwrap();

    let grant = sign(
        &owner,
        CommunityRole {
            community_id: cid.clone(),
            target: addr(&member),
            role: "moderator".into(),
        },
        domain.clone(),
        rand_nonce(),
        now() + 2,
    )
    .await;
    apply::apply_role(&pool, &grant, &addr(&owner))
        .await
        .expect("apply_role must not trip the role CHECK constraint");

    let logged: (String,) = sqlx::query_as(
        "SELECT role FROM community_role_log WHERE community_id = $1 AND target = $2 \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(&cid)
    .bind(addr(&member))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logged.0, "mod", "log must store canonical `mod`");

    assert_eq!(
        load_role(&pool, &cid, &addr(&member)).await.unwrap(),
        Role::Mod
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn tiebreaker_lower_sig_wins() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping tiebreaker_lower_sig_wins: no postgres reachable");
        return;
    };
    let domain = domains::communities();
    let creator = mk_wallet(101);
    let admin_a = mk_wallet(102);
    let admin_b = mk_wallet(103);
    let target = mk_wallet(104);

    let create = sign(
        &creator,
        CommunityCreate {
            name: "Tiebreaker".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let applied = apply::apply_create(&pool, &create, &addr(&creator))
        .await
        .unwrap();
    let cid = applied.community_id.clone();

    for w in [&admin_a, &admin_b, &target] {
        let j = sign(
            w,
            CommunityJoin {
                community_id: cid.clone(),
            },
            domain.clone(),
            rand_nonce(),
            now(),
        )
        .await;
        apply::apply_join(&pool, &j, &addr(w)).await.unwrap();
    }
    for w in [&admin_a, &admin_b] {
        let g = sign(
            &creator,
            CommunityRole {
                community_id: cid.clone(),
                target: addr(w),
                role: "moderator".into(),
            },
            domain.clone(),
            rand_nonce(),
            now(),
        )
        .await;
        apply::apply_role(&pool, &g, &addr(&creator)).await.unwrap();
    }

    let signed_at = now() + 5;
    let mut a_role = String::new();
    let mut b_role = String::new();
    let mut a_sig = String::new();
    let mut b_sig = String::new();
    for attempt in 0..40u32 {
        let mut na = rand_nonce();
        na[0] = (attempt & 0xff) as u8;
        let action_a = sign(
            &admin_a,
            CommunityRole {
                community_id: cid.clone(),
                target: addr(&target),
                role: "mod".into(),
            },
            domain.clone(),
            na,
            signed_at,
        )
        .await;
        let mut nb = rand_nonce();
        nb[0] = (attempt & 0xff) as u8;
        nb[15] = ((attempt + 1) & 0xff) as u8;
        let action_b = sign(
            &admin_b,
            CommunityRole {
                community_id: cid.clone(),
                target: addr(&target),
                role: "member".into(),
            },
            domain.clone(),
            nb,
            signed_at,
        )
        .await;
        let ha = hex::encode(action_a.hash());
        let hb = hex::encode(action_b.hash());
        if ha != hb {
            a_role = action_a.message.role.clone();
            b_role = action_b.message.role.clone();
            a_sig = ha.clone();
            b_sig = hb.clone();

            apply::apply_role(&pool, &action_a, &addr(&admin_a))
                .await
                .unwrap();
            apply::apply_role(&pool, &action_b, &addr(&admin_b))
                .await
                .unwrap();
            break;
        }
    }
    assert!(!a_sig.is_empty(), "should have picked two distinct sigs");

    let winner = load_role(&pool, &cid, &addr(&target)).await.unwrap();
    let expected_role = if a_sig < b_sig { &a_role } else { &b_role };
    assert_eq!(
        winner.as_str(),
        expected_role,
        "lower sig_hash {} should beat {}; expected role {}",
        std::cmp::min(&a_sig, &b_sig),
        std::cmp::max(&a_sig, &b_sig),
        expected_role
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn replay_rejects_duplicate_nonce() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping replay_rejects_duplicate_nonce: no postgres reachable");
        return;
    };
    let domain = domains::communities();
    let replay = Replay::new(pool.clone()).await.expect("replay init");
    let _limiter = Arc::new(RateLimiter::new(60, Duration::from_secs(60)));
    let w = mk_wallet(200);
    let nonce = rand_nonce();
    let signed_at = now();

    let create = sign(
        &w,
        CommunityCreate {
            name: "Once".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        nonce,
        signed_at,
    )
    .await;
    replay
        .check_and_record(&addr(&w), &create.nonce, create.signed_at)
        .await
        .expect("first ok");
    let second = replay
        .check_and_record(&addr(&w), &create.nonce, create.signed_at)
        .await;
    assert!(second.is_err(), "duplicate nonce must be rejected");

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn community_id_hash_is_deterministic() {
    let creator = "0xabc";
    let name = "MyName";
    let n = [7u8; 16];
    let a = community_id_hex(creator, name, &n);
    let b = community_id_hex(creator, name, &n);
    assert_eq!(a, b);
    let c = community_id_hex(creator, name, &[8u8; 16]);
    assert_ne!(a, c);
}

#[tokio::test]
async fn signed_message_roundtrips_bytes() {
    let domain = domains::communities();
    let w = mk_wallet(77);
    let m = CommunityCreate {
        name: "RT".into(),
        description: "ok".into(),
        private: true,
        unlisted: false,
        flags: vec!["e2ee".into()],
    };
    let s = sign(&w, m.clone(), domain.clone(), [1u8; 16], 1_700_000_000).await;
    let bytes = serde_json::to_vec(&s).unwrap();
    let back: Signed<CommunityCreate> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(back.hash(), s.hash());
    assert_eq!(back.message.encode_struct(), s.message.encode_struct());
    back.verify(&addr(&w), 1_700_000_000).expect("verifies");
}

#[tokio::test]
async fn signer_authority_enforced_in_handlers_via_apply() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!(
            "skipping signer_authority_enforced_in_handlers_via_apply: no postgres reachable"
        );
        return;
    };
    let domain = domains::communities();
    let creator = mk_wallet(31);
    let outsider = mk_wallet(32);

    let create = sign(
        &creator,
        CommunityCreate {
            name: "GateCheck".into(),
            description: "".into(),
            private: false,
            unlisted: false,
            flags: vec![],
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let applied = apply::apply_create(&pool, &create, &addr(&creator))
        .await
        .unwrap();
    let cid = applied.community_id.clone();

    let res = require_min_role(&pool, &cid, &addr(&outsider), Role::Mod).await;
    assert!(res.is_err());
    let res2 = require_min_role(&pool, &cid, &addr(&creator), Role::Owner).await;
    assert!(res2.is_ok());

    let post = sign(
        &outsider,
        CommunityPost {
            community_id: cid.clone(),
            content_hash: "QmFake".into(),
        },
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let post_gate = require_min_role(&pool, &cid, &addr(&outsider), Role::Member).await;
    assert!(post_gate.is_err(), "non-member cannot post");
    let _ = post;

    cleanup(&admin_url, &schema).await;
}
