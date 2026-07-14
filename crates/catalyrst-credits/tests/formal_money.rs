//! Money-invariant regression suite for `catalyrst-credits`.
//!
//! Every test here pins a defect that was LIVE in the crate and is reproducible
//! against a real PostgreSQL instance. They were derived from a formal
//! refutation run; the formal sources were scaffolding and are gone, so these
//! are the durable artifact.
//!
//! Each test fails without its fix. Run with a scratch database:
//!
//! ```text
//! CREDITS_TEST_PG_CONNECTION_STRING=postgres://.../scratch_db \
//!   cargo test -p catalyrst-credits --test formal_money
//! ```
//!
//! Without the env var (or the workspace-wide CATALYRST_TEST_PG) every test
//! here fails naming the variable; ALLOW_SKIPPED_INTEGRATION=1 downgrades that
//! to a skip.

use catalyrst_credits::ports::checkout::RepricedLine;
use catalyrst_credits::ports::credits::CreditsComponent;
use catalyrst_credits::ports::packs::ReversalOutcome;

mod common;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> Option<sqlx::PgPool> {
    let url = catalyrst_testgate::require_pg("CREDITS_TEST_PG_CONNECTION_STRING")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("test PG unreachable");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("test PG migrations failed");
    Some(pool)
}

static ADDR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch_addr() -> String {
    let n = ADDR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    format!(
        "0xfm{:012x}{:012x}{:012x}",
        nanos,
        std::process::id() as u64,
        n
    )
    .chars()
    .take(42)
    .collect()
}

async fn seed_wallet(pool: &sqlx::PgPool, addr: &str, earned: &str, paid: &str) {
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) \
         VALUES ($1, $2::numeric + $3::numeric, $2::numeric)",
    )
    .bind(addr)
    .bind(earned)
    .bind(paid)
    .execute(pool)
    .await
    .unwrap();
    for (kind, bucket, amt) in [("claim", "earned", earned), ("grant", "paid", paid)] {
        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
             SELECT $1, $2, $3::numeric, $4, FALSE WHERE $3::numeric > 0",
        )
        .bind(addr)
        .bind(kind)
        .bind(amt)
        .bind(bucket)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Balance as exact NUMERIC text -- never f64, which is the whole point.
async fn available(pool: &sqlx::PgPool, addr: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE((SELECT available::text FROM user_credits WHERE address = $1), '0')",
    )
    .bind(addr)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `true` when the signed ledger sum equals the balance, for BOTH the total and
/// the earned bucket -- evaluated by PostgreSQL in NUMERIC.
async fn ledger_matches_balance(pool: &sqlx::PgPool, addr: &str) -> (bool, String, String) {
    let row: (bool, String, String) = sqlx::query_as(
        "WITH l AS ( \
             SELECT COALESCE(SUM(CASE \
                        WHEN kind IN ('grant','refund','purchase','claim') THEN amount \
                        WHEN kind IN ('spend','consume','expire') THEN -amount \
                        ELSE 0 END), 0) AS total, \
                    COALESCE(SUM(CASE \
                        WHEN bucket <> 'earned' THEN 0 \
                        WHEN kind IN ('grant','refund','purchase','claim') THEN amount \
                        WHEN kind IN ('spend','consume','expire') THEN -amount \
                        ELSE 0 END), 0) AS earned \
             FROM credit_ledger WHERE address = $1 \
         ), u AS ( \
             SELECT COALESCE((SELECT available FROM user_credits WHERE address = $1), 0) AS av, \
                    COALESCE((SELECT earned_available FROM user_credits WHERE address = $1), 0) \
                        AS ea \
         ) \
         SELECT (l.total = u.av AND l.earned = u.ea), \
                (l.total - u.av)::text, (l.earned - u.ea)::text \
         FROM l, u",
    )
    .bind(addr)
    .fetch_one(pool)
    .await
    .unwrap();
    row
}

async fn assert_reconciles(pool: &sqlx::PgPool, addr: &str, ctx: &str) {
    let (ok, total_drift, earned_drift) = ledger_matches_balance(pool, addr).await;
    assert!(
        ok,
        "{ctx}: ledger replay must reproduce the balance \
         (total drift {total_drift}, earned drift {earned_drift})"
    );
}

async fn ledger_kinds(pool: &sqlx::PgPool, addr: &str) -> Vec<(String, String, String)> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT kind, bucket, amount::text FROM credit_ledger \
         WHERE address = $1 ORDER BY id",
    )
    .bind(addr)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn seed_paid_purchase(
    pool: &sqlx::PgPool,
    addr: &str,
    pi: &str,
    credits: &str,
    amount_cents: i64,
) {
    sqlx::query(
        "INSERT INTO credit_purchases \
             (address, sku, credits, amount_cents, currency, stripe_payment_intent, \
              method, status) \
         VALUES ($1, 'test-pack', $2::numeric, $3, 'usd', $4, 'card', 'paid')",
    )
    .bind(addr)
    .bind(credits)
    .bind(amount_cents)
    .bind(pi)
    .execute(pool)
    .await
    .unwrap();
}

async fn purchase_revoked(pool: &sqlx::PgPool, pi: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT revoked_credits::text FROM credit_purchases WHERE stripe_payment_intent = $1",
    )
    .bind(pi)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, addr: &str) {
    for sql in [
        "DELETE FROM credit_ledger WHERE address = $1",
        "DELETE FROM credit_purchases WHERE address = $1",
        "DELETE FROM credit_refund_idempotency WHERE address = $1",
        "DELETE FROM credit_spend_idempotency WHERE address = $1",
        "DELETE FROM credit_grant_idempotency WHERE address = $1",
        "DELETE FROM admin_audit WHERE address = $1",
        "DELETE FROM user_credits WHERE address = $1",
    ] {
        sqlx::query(sql).bind(addr).execute(pool).await.unwrap();
    }
}

/// DEFECT: `record_full_reversal` compensated a dispute by calling
/// `refund_in_tx`, and refund ADDS credits. On `charge.dispute.*` the buyer got
/// the fiat back from Stripe, KEPT the credits, and was credited that amount
/// AGAIN -- a 100-credit purchase left the wallet at 200.
///
/// CORRECT: a reversed fiat payment REVOKES the credits it granted.
#[tokio::test]
async fn dispute_revokes_granted_credits_instead_of_paying_the_buyer_twice() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_dispute_{addr}");
    let credits = CreditsComponent::new(pool.clone());

    seed_paid_purchase(&pool, &addr, &pi, "100", 1000).await;
    credits
        .admin_grant_credits(
            &addr,
            "100",
            "purchase",
            Some("stripe purchase"),
            Some("stripe"),
            None,
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(available(&pool, &addr).await, "100");

    let outcome = credits
        .record_full_reversal(&pi, "disputed", &format!("evt_dispute_{addr}"))
        .await
        .unwrap();

    match outcome {
        ReversalOutcome::Reversed {
            charged_back,
            removed,
            has_shortfall,
            ..
        } => {
            assert_eq!(charged_back.parse::<f64>().unwrap(), 100.0);
            assert_eq!(removed.parse::<f64>().unwrap(), 100.0);
            assert!(!has_shortfall, "the buyer had not spent anything");
        }
        other => panic!("expected a reversal, got {other:?}"),
    }

    assert_eq!(
        available(&pool, &addr).await.parse::<f64>().unwrap(),
        0.0,
        "a chargeback must REMOVE the granted credits, never add them"
    );
    let rows = ledger_kinds(&pool, &addr).await;
    assert!(
        !rows.iter().any(|(kind, _, _)| kind == "refund"),
        "a fiat reversal is not a credits refund; ledger kinds must be honest: {rows:?}"
    );
    assert!(
        rows.iter().any(|(kind, _, _)| kind == "consume"),
        "the reversal must appear as a debit row: {rows:?}"
    );
    assert_reconciles(&pool, &addr, "after dispute").await;

    cleanup(&pool, &addr).await;
}

/// The same defect on the `charge.refunded` path, plus the shortfall report:
/// when the buyer already spent the credits we can only claw back what is left,
/// and the remainder must be surfaced as a loss rather than silently ignored.
#[tokio::test]
async fn charge_refund_revokes_and_reports_the_unrecoverable_shortfall() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_short_{addr}");
    let credits = CreditsComponent::new(pool.clone());

    seed_paid_purchase(&pool, &addr, &pi, "100", 1000).await;
    credits
        .admin_grant_credits(
            &addr,
            "100",
            "purchase",
            None,
            Some("stripe"),
            None,
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    credits
        .spend(&addr, "70", "checkout:formal-shortfall", None)
        .await
        .unwrap();
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 30.0);

    let outcome = credits
        .record_charge_refund(&pi, 1000, &format!("evt_short_{addr}"))
        .await
        .unwrap();
    match outcome {
        ReversalOutcome::Reversed {
            charged_back,
            removed,
            shortfall,
            has_shortfall,
            ..
        } => {
            assert_eq!(charged_back.parse::<f64>().unwrap(), 100.0);
            assert_eq!(removed.parse::<f64>().unwrap(), 30.0);
            assert_eq!(shortfall.parse::<f64>().unwrap(), 70.0);
            assert!(has_shortfall);
        }
        other => panic!("expected a reversal, got {other:?}"),
    }
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 0.0);
    assert_reconciles(&pool, &addr, "after partial-clawback reversal").await;

    cleanup(&pool, &addr).await;
}

/// INTENT PIN, not a defect. `revoke_in_tx` debits **paid-first** -- the
/// opposite of the earned-first spend rule -- and its doc comment used to claim
/// earned-first, contradicting its own SQL. The SQL is the correct half: a
/// chargeback reverses a PURCHASE, a purchase grants PAID credits, so the paid
/// bucket is what a reversal takes back first. A buyer who charges back a pack
/// must not lose credits they earned by playing while paid credits sit
/// untouched.
///
/// The witness: earned 40 / paid 60, revoke 50 -> earned 40 / paid 10. Under
/// earned-first it would be earned 0 / paid 50.
#[tokio::test]
async fn revoke_debits_the_paid_bucket_first() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_paidfirst_{addr}");
    let credits = CreditsComponent::new(pool.clone());

    seed_wallet(&pool, &addr, "40", "60").await;
    seed_paid_purchase(&pool, &addr, &pi, "50", 500).await;

    credits
        .record_full_reversal(&pi, "disputed", &format!("evt_pf_{addr}"))
        .await
        .unwrap();

    let (av, ea): (String, String) = sqlx::query_as(
        "SELECT available::text, earned_available::text FROM user_credits WHERE address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (av.as_str(), ea.as_str()),
        ("50", "40"),
        "a chargeback takes PAID credits back first; the earned bucket is only \
         squeezed once paid is exhausted"
    );

    let debits: Vec<(String, String)> = sqlx::query_as(
        "SELECT bucket, amount::text FROM credit_ledger \
         WHERE address = $1 AND kind = 'consume' ORDER BY id",
    )
    .bind(&addr)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        debits,
        vec![("paid".to_string(), "50".to_string())],
        "the whole debit came out of the paid bucket: {debits:?}"
    );

    // The spill-over half of the rule: revoking more than the paid bucket holds
    // reaches into earned for the remainder, and no further.
    let pi2 = format!("pi_paidfirst2_{addr}");
    seed_paid_purchase(&pool, &addr, &pi2, "30", 300).await;
    credits
        .record_full_reversal(&pi2, "disputed", &format!("evt_pf2_{addr}"))
        .await
        .unwrap();
    let (av, ea): (String, String) = sqlx::query_as(
        "SELECT available::text, earned_available::text FROM user_credits WHERE address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (av.as_str(), ea.as_str()),
        ("20", "20"),
        "paid 10 absorbed first, the remaining 20 came out of earned"
    );
    assert_reconciles(&pool, &addr, "after paid-first revocations").await;

    cleanup(&pool, &addr).await;
}

/// DEFECT: when `revoke_in_tx` found no `user_credits` row it early-returned a
/// full-amount shortfall with NO audit row and NO ledger row -- so a chargeback
/// against an address that never had a wallet vanished from every record, even
/// though it is a 100% unrecovered loss and the single worst outcome the
/// function can produce.
///
/// CORRECT: the no-wallet path writes the same audit trail as the normal path,
/// recording an explicit zero removal and a full shortfall. It still writes no
/// LEDGER row, deliberately: the balance did not move, and the ledger's
/// contract is that its signed replay reproduces the balance.
#[tokio::test]
async fn revoke_without_a_wallet_row_is_still_audited_as_a_total_loss() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_nowallet_{addr}");
    let credits = CreditsComponent::new(pool.clone());

    seed_paid_purchase(&pool, &addr, &pi, "100", 1000).await;
    assert_eq!(available(&pool, &addr).await, "0");

    let outcome = credits
        .record_full_reversal(&pi, "disputed", &format!("evt_nw_{addr}"))
        .await
        .unwrap();
    match outcome {
        ReversalOutcome::Reversed {
            removed,
            shortfall,
            has_shortfall,
            ..
        } => {
            assert_eq!(removed.parse::<f64>().unwrap(), 0.0);
            assert_eq!(shortfall.parse::<f64>().unwrap(), 100.0);
            assert!(has_shortfall, "nothing could be clawed back at all");
        }
        other => panic!("expected a reversal, got {other:?}"),
    }

    let audits: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT action, amount::text, detail FROM admin_audit \
         WHERE address = $1 AND action = 'credits.revoke' ORDER BY id",
    )
    .bind(&addr)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        audits.len(),
        1,
        "the no-wallet chargeback must leave exactly one audit row: {audits:?}"
    );
    let (_, amount, detail) = &audits[0];
    assert_eq!(amount.parse::<f64>().unwrap(), 0.0, "nothing was removed");
    assert_eq!(detail["removed"], serde_json::json!("0"));
    assert_eq!(detail["shortfall"], serde_json::json!("100"));
    assert_eq!(detail["chargedBack"], serde_json::json!("100"));
    assert_eq!(
        detail["walletRow"],
        serde_json::json!("missing"),
        "finance must be able to tell this apart from a partial clawback"
    );

    assert!(ledger_kinds(&pool, &addr).await.is_empty());
    assert_reconciles(&pool, &addr, "after a no-wallet chargeback").await;

    cleanup(&pool, &addr).await;
}

/// DEFECT: `refund_in_tx` skipped its cumulative clamp entirely when the
/// tx_ref had no `spend` rows (`ELSE $2::numeric`), and `ports/packs.rs` passed
/// a Stripe EVENT ID as tx_ref -- event ids never have spend rows. Every Stripe
/// reversal was therefore an UNBOUNDED credit.
///
/// CORRECT: a refund restores what was spent under its tx_ref, and nothing else.
#[tokio::test]
async fn refund_under_a_tx_ref_with_no_spend_applies_nothing() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "0", "15").await;

    let outcome = credits
        .refund(&addr, "1000000", "evt_1Abc_stripe_event_id", None)
        .await
        .unwrap();

    assert_eq!(
        outcome.applied.parse::<f64>().unwrap(),
        0.0,
        "an unmatched tx_ref must not mint credits"
    );
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 15.0);
    assert_reconciles(&pool, &addr, "after unmatched refund").await;

    cleanup(&pool, &addr).await;
}

/// The pack/purchase path is bounded by the PURCHASE, not by spend rows:
/// however Stripe replays or reorders its events, cumulative charge-backs can
/// never exceed the credits the purchase granted.
#[tokio::test]
async fn pack_reversal_is_bounded_by_the_purchase_across_replays() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_bound_{addr}");
    let credits = CreditsComponent::new(pool.clone());

    seed_paid_purchase(&pool, &addr, &pi, "100", 1000).await;
    credits
        .admin_grant_credits(
            &addr,
            "100",
            "purchase",
            None,
            Some("stripe"),
            None,
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    credits
        .record_charge_refund(&pi, 300, &format!("evt_b1_{addr}"))
        .await
        .unwrap();
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 70.0);

    assert!(matches!(
        credits
            .record_charge_refund(&pi, 300, &format!("evt_b1r_{addr}"))
            .await
            .unwrap(),
        ReversalOutcome::NothingToReverse
    ));
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 70.0);

    credits
        .record_charge_refund(&pi, 1000, &format!("evt_b2_{addr}"))
        .await
        .unwrap();
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 0.0);
    assert_eq!(
        purchase_revoked(&pool, &pi).await.parse::<f64>().unwrap(),
        100.0,
        "a fully reversed purchase must land on exactly its granted credits"
    );

    let after = credits
        .record_full_reversal(&pi, "disputed", &format!("evt_b3_{addr}"))
        .await
        .unwrap();
    assert!(
        matches!(after, ReversalOutcome::NoPaidPurchase),
        "a fully-refunded purchase is no longer 'paid': {after:?}"
    );
    assert_eq!(
        purchase_revoked(&pool, &pi).await.parse::<f64>().unwrap(),
        100.0
    );
    assert_reconciles(&pool, &addr, "after bounded reversals").await;

    cleanup(&pool, &addr).await;
}

/// The bound is enforced by the database too, not only by the code path: the
/// `credit_purchases_revoked_bound` CHECK makes over-revocation unrepresentable.
#[tokio::test]
async fn over_revocation_is_rejected_by_the_database() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let pi = format!("pi_check_{addr}");
    seed_paid_purchase(&pool, &addr, &pi, "100", 1000).await;

    let err = sqlx::query(
        "UPDATE credit_purchases SET revoked_credits = 100.0000001 \
         WHERE stripe_payment_intent = $1",
    )
    .bind(&pi)
    .execute(&pool)
    .await
    .expect_err("revoking more than the purchase granted must be rejected");
    assert!(
        err.to_string().contains("credit_purchases_revoked_bound"),
        "expected the revocation bound to fire, got: {err}"
    );

    cleanup(&pool, &addr).await;
}

/// DEFECT: `spend` had no amount guard while `refund_in_tx` did. A negative
/// amount MINTED credits: `100 >= -5` passed the sufficiency check,
/// `LEAST(earned_available, -5)` was -5, and `available := available + 5`.
/// `pub fn spend` is a public entry point.
///
/// NOTE the guard is `parse_non_negative`, not `parse_positive`: zero is a
/// legitimate spend (see `zero_spend_is_a_no_op_not_an_error`). The anti-mint
/// property is about NEGATIVES, and it is what this test pins.
#[tokio::test]
async fn spend_rejects_non_positive_amounts_and_never_mints() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "20", "80").await;

    for bad in [
        "-5",
        "-0.000001",
        "-1e-9",
        "-1e400",
        "NaN",
        "Infinity",
        "1e400",
        "abc",
        "",
    ] {
        let err = credits
            .spend(&addr, bad, "checkout:formal-guard", None)
            .await
            .expect_err("spend must reject {bad:?}");
        assert_eq!(common::status_of(err), 400, "for amount {bad:?}");
        assert_eq!(
            available(&pool, &addr).await.parse::<f64>().unwrap(),
            100.0,
            "rejected spend {bad:?} must not move the balance"
        );
    }
    assert_reconciles(&pool, &addr, "after rejected spends").await;

    // The admin paths carry the same guard: a negative revoke would mint too.
    for bad in ["-5", "0"] {
        assert_eq!(
            common::status_of(
                credits
                    .admin_revoke_credits(&addr, bad, None, Some("t"), &serde_json::json!({}))
                    .await
                    .expect_err("admin revoke must reject non-positive amounts")
            ),
            400
        );
        assert_eq!(
            common::status_of(
                credits
                    .admin_grant_credits(
                        &addr,
                        bad,
                        "grant",
                        None,
                        Some("t"),
                        None,
                        &serde_json::json!({})
                    )
                    .await
                    .expect_err("admin grant must reject non-positive amounts")
            ),
            400
        );
    }
    assert_eq!(available(&pool, &addr).await.parse::<f64>().unwrap(), 100.0);

    cleanup(&pool, &addr).await;
}

/// The other edge of the same guard, and a REGRESSION of the guard's first
/// version: `parse_positive` rejected "0", but `ports/checkout.rs` computes the
/// spend amount as `COALESCE(SUM(unit_price_credits * qty), 0)`, so an all-free
/// cart legitimately spends 0 and got a 400.
///
/// DECIDED SEMANTICS: a zero spend is a NO-OP. It succeeds, it changes no
/// balance, and it writes NO ledger row -- a zero-amount row would be noise on a
/// wallet that did not move, and the ledger's contract is that its signed
/// replay reproduces the balance.
#[tokio::test]
async fn zero_spend_is_a_no_op_not_an_error() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "20", "80").await;
    let ledger_before = ledger_kinds(&pool, &addr).await;

    // Every spelling of exact zero PostgreSQL would accept as NUMERIC 0.
    for zero in ["0", "0.00", "0e10", "-0", "+0", ".0"] {
        let outcome = credits
            .spend(&addr, zero, "checkout:formal-zero", None)
            .await
            .unwrap_or_else(|e| panic!("zero spend {zero:?} must succeed, got {e:?}"));
        assert!(!outcome.replayed);
        assert_eq!(
            outcome.applied.parse::<f64>().unwrap(),
            0.0,
            "zero spend {zero:?} must report applying nothing"
        );
        assert_eq!(
            outcome.available, "100",
            "zero spend {zero:?} must report the untouched balance"
        );
        assert_eq!(
            available(&pool, &addr).await,
            "100",
            "zero spend {zero:?} must not move the balance"
        );
    }

    assert_eq!(
        ledger_kinds(&pool, &addr).await,
        ledger_before,
        "a zero spend must write NO ledger row"
    );
    assert_reconciles(&pool, &addr, "after zero spends").await;

    // Zero on a wallet that has no row at all is still a no-op, not a 402.
    let fresh = scratch_addr();
    let outcome = credits
        .spend(&fresh, "0", "checkout:formal-zero-fresh", None)
        .await
        .expect("a zero spend needs no wallet row");
    assert_eq!(outcome.available, "0");
    assert!(ledger_kinds(&pool, &fresh).await.is_empty());

    // A zero spend must NOT burn an idempotency key: there is no effect to
    // deduplicate, and a later real spend under that key must still work.
    let key = format!("t:zero-{addr}");
    credits
        .spend(&addr, "0", "checkout:formal-zero", Some(&key))
        .await
        .unwrap();
    let real = credits
        .spend(&addr, "25", "checkout:formal-zero", Some(&key))
        .await
        .expect("a zero spend must not consume the key for a later real spend");
    assert_eq!(real.applied, "25");
    assert_eq!(available(&pool, &addr).await, "75");
    assert_reconciles(&pool, &addr, "after zero-then-real spend").await;

    cleanup(&pool, &addr).await;
    cleanup(&pool, &fresh).await;
}

/// The end-to-end shape of the same defect: a cart of free items produces
/// `total = 0`, and the whole checkout must complete rather than 400 (or, on a
/// wallet with no `user_credits` row, 402 for "insufficient" against a total of
/// zero).
#[tokio::test]
async fn zero_total_checkout_completes_and_moves_nothing() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let credits = CreditsComponent::new(pool.clone());

    const COLLECTION: &str = "0x59a90bad9570ecd08895f132daf7b79696337f61";
    const URN: &str =
        "urn:decentraland:matic:collections-v2:0x59a90bad9570ecd08895f132daf7b79696337f61:1";
    let free_line = |item: &str| RepricedLine {
        item_id: item.into(),
        collection: COLLECTION.into(),
        urn: URN.into(),
        category: "wearable".into(),
        qty: 1,
        unit_price_credits: "0".into(),
        token_id: None,
        trade_id: None,
        basis_wei: Some("0".into()),
        mode: "primary".into(),
    };

    let addr = scratch_addr();
    seed_wallet(&pool, &addr, "20", "80").await;
    let before = ledger_kinds(&pool, &addr).await;
    credits
        .add_item(&addr, "1", COLLECTION, URN, "wearable", 1, "0")
        .await
        .unwrap();
    let out = credits
        .run_checkout(&addr, &format!("t:zero-co-{addr}"), &[free_line("1")])
        .await
        .expect("an all-free cart must check out");
    assert!(!out.replayed);
    assert_eq!(out.status, "fulfilling");
    assert_eq!(
        available(&pool, &addr).await,
        "100",
        "a zero-total checkout must not move the balance"
    );
    assert_eq!(
        ledger_kinds(&pool, &addr).await,
        before,
        "a zero-total checkout must write no ledger row"
    );
    assert_reconciles(&pool, &addr, "after a zero-total checkout").await;

    let fresh = scratch_addr();
    credits
        .add_item(&fresh, "1", COLLECTION, URN, "wearable", 1, "0")
        .await
        .unwrap();
    let out = credits
        .run_checkout(&fresh, &format!("t:zero-co-{fresh}"), &[free_line("1")])
        .await
        .expect("a zero total is affordable without a wallet row");
    assert_eq!(out.status, "fulfilling");
    assert!(ledger_kinds(&pool, &fresh).await.is_empty());

    for a in [&addr, &fresh] {
        sqlx::query(
            "DELETE FROM fulfillment_outbox \
             WHERE checkout_id IN (SELECT id FROM checkouts WHERE address = $1)",
        )
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
        for sql in [
            "DELETE FROM checkouts WHERE address = $1",
            "DELETE FROM cart_items WHERE cart_id IN (SELECT id FROM carts WHERE address = $1)",
            "DELETE FROM carts WHERE address = $1",
        ] {
            sqlx::query(sql).bind(a).execute(&pool).await.unwrap();
        }
        cleanup(&pool, a).await;
    }
}

/// DEFECT: three sites decided in `f64` whether to write a ledger row while the
/// effect was applied in NUMERIC. MEASURED: PostgreSQL says
/// `1e-400::numeric > 0` is TRUE; `f64::from_str("1e-400")` is `0.0`, so
/// `> 0.0` is FALSE. The balance moved, the ledger row was omitted (or, via the
/// "write one anyway" fallback, was written to the WRONG bucket), and reconcile
/// diverged forever.
///
/// The amount below is 1e-400 written out in full decimal, so nothing but the
/// parse rules is being tested.
#[tokio::test]
async fn sub_f64_amounts_keep_ledger_and_balance_in_lockstep() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let credits = CreditsComponent::new(pool.clone());
    let dust = format!("0.{}1", "0".repeat(399));
    assert_eq!(
        dust.parse::<f64>().unwrap(),
        0.0,
        "the witness must be invisible to f64"
    );

    let addr = scratch_addr();
    seed_wallet(&pool, &addr, "20", "15").await;
    credits
        .spend(&addr, &dust, "checkout:formal-dust", None)
        .await
        .unwrap();
    assert_reconciles(&pool, &addr, "after sub-f64 spend").await;
    let spends: Vec<(String, String)> = sqlx::query_as(
        "SELECT bucket, amount::text FROM credit_ledger \
         WHERE address = $1 AND kind = 'spend'",
    )
    .bind(&addr)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(spends.len(), 1, "exactly one debit row: {spends:?}");
    assert_eq!(spends[0].0, "earned", "the debit came out of earned");

    credits
        .refund(&addr, &dust, "checkout:formal-dust", None)
        .await
        .unwrap();
    assert_reconciles(&pool, &addr, "after sub-f64 refund").await;

    credits
        .admin_revoke_credits(&addr, &dust, None, Some("t"), &serde_json::json!({}))
        .await
        .unwrap();
    assert_reconciles(&pool, &addr, "after sub-f64 revoke").await;

    cleanup(&pool, &addr).await;
}

/// DEFECT: `reconcile_earned_balance` assumes a ledger-faithful origin, which
/// migration 0012's `GREATEST(0, LEAST(available, earned_sum))` backfill does
/// not provide for pre-existing wallets. Such a wallet trips the reconcile
/// alarm forever through no fault of any later write.
///
/// DECISION (see `realign_ledger_to_balances`): record the clamp as an explicit
/// adjustment ledger row so replay reproduces the stored balance, rather than
/// teaching reconcile to tolerate a special origin.
#[tokio::test]
async fn clamped_backfill_origin_is_recorded_as_an_adjustment_row() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());

    // Reproduce a 0012-clamped wallet: the earned ledger says 50, but the
    // backfill stored LEAST(available, 50) = 10.
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) VALUES ($1,10,10)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
         VALUES ($1, 'claim', 50, 'earned', FALSE)",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();

    let (ok, _, _) = ledger_matches_balance(&pool, &addr).await;
    assert!(!ok, "the clamped origin must start out divergent");

    let applied = credits
        .realign_ledger_to_balances("test: 0012 clamp")
        .await
        .unwrap();
    assert!(
        applied.iter().any(|a| a.address == addr),
        "the clamped wallet must get an adjustment row: {applied:?}"
    );
    assert_reconciles(&pool, &addr, "after realignment").await;

    let adjustments: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT kind, bucket, amount::text FROM credit_ledger \
         WHERE address = $1 AND tx_ref = $2 ORDER BY id",
    )
    .bind(&addr)
    .bind(catalyrst_credits::ports::reconcile::REALIGN_TX_REF)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        adjustments,
        vec![("consume".into(), "earned".into(), "40".into())],
        "earned ledger sum 50 vs stored 10 -> a -40 earned adjustment"
    );

    let second = credits
        .realign_ledger_to_balances("test: rerun")
        .await
        .unwrap();
    assert!(
        !second.iter().any(|a| a.address == addr),
        "realignment must be idempotent: {second:?}"
    );
    assert_reconciles(&pool, &addr, "after idempotent rerun").await;

    cleanup(&pool, &addr).await;
}

/// The realignment is BATCHED, and that is a correctness-of-operations
/// property, not a micro-optimisation: the first version took `FOR UPDATE` over
/// the ENTIRE `user_credits` table, twice, in one transaction, with a
/// correlated per-wallet ledger aggregate and no LIMIT -- a long exclusive lock
/// across every wallet on the money path.
///
/// This drives the walk with a chunk size of ONE so the multi-chunk path is
/// exercised without seeding thousands of wallets, and pins what batching must
/// not break: every divergent wallet is still realigned, the result is still
/// idempotent, and each chunk commits on its own (so a rerun resumes rather
/// than redoing).
#[tokio::test]
async fn realignment_batches_the_walk_and_stays_idempotent() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let credits = CreditsComponent::new(pool.clone());

    // Three wallets, each divergent in a different direction, so a chunk
    // boundary cannot hide behind a uniform delta.
    let addrs: Vec<String> = (0..3).map(|_| scratch_addr()).collect();
    for (i, addr) in addrs.iter().enumerate() {
        sqlx::query(
            "INSERT INTO user_credits (address, available, earned_available) VALUES ($1,$2,$3)",
        )
        .bind(addr)
        .bind(10 + i as i32)
        .bind(10 + i as i32)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
             VALUES ($1, 'claim', $2, 'earned', FALSE)",
        )
        .bind(addr)
        .bind(50 + i as i32)
        .execute(&pool)
        .await
        .unwrap();
        let (ok, _, _) = ledger_matches_balance(&pool, addr).await;
        assert!(!ok, "wallet {addr} must start out divergent");
    }

    // Chunk size 1: at least one transaction per wallet in the table.
    let applied = credits
        .realign_ledger_to_balances_in_batches("test: batched walk", 1)
        .await
        .unwrap();
    for addr in &addrs {
        assert!(
            applied.iter().any(|a| &a.address == addr),
            "batching must not skip {addr}: {applied:?}"
        );
        assert_reconciles(&pool, addr, "after batched realignment").await;
    }

    let second = credits
        .realign_ledger_to_balances_in_batches("test: batched rerun", 1)
        .await
        .unwrap();
    for addr in &addrs {
        assert!(
            !second.iter().any(|a| &a.address == addr),
            "a rerun must find nothing left to do for {addr}: {second:?}"
        );
        assert_reconciles(&pool, addr, "after batched rerun").await;
    }

    let third = credits
        .realign_ledger_to_balances("test: default batch size")
        .await
        .unwrap();
    for addr in &addrs {
        assert!(!third.iter().any(|a| &a.address == addr));
        cleanup(&pool, addr).await;
    }
}

/// NOT A DEFECT -- pinned so it is not "fixed" later.
///
/// A refund restores the earned bucket FIRST rather than proportionally to the
/// original spend's earned/paid split. That was refuted as a *property* by the
/// formal run, but it is correct BY DESIGN: it mirrors the earned-first debit
/// rule, so a spend followed by its full refund is exactly the identity, and no
/// sequence of partial refunds can leave the wallet with more paid credits than
/// it started with.
#[tokio::test]
async fn refund_restores_earned_first_by_design() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "20", "15").await;

    credits
        .spend(&addr, "25", "checkout:formal-earned-first", None)
        .await
        .unwrap();
    let (av, ea): (String, String) = sqlx::query_as(
        "SELECT available::text, earned_available::text FROM user_credits WHERE address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((av.as_str(), ea.as_str()), ("10", "0"));

    // Refund 5: earned-FIRST restore, so all 5 land in earned -- NOT the 4:1
    // proportional split (4 earned / 1 paid) the refuted property expected.
    credits
        .refund(&addr, "5", "checkout:formal-earned-first", None)
        .await
        .unwrap();
    let (av, ea): (String, String) = sqlx::query_as(
        "SELECT available::text, earned_available::text FROM user_credits WHERE address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (av.as_str(), ea.as_str()),
        ("15", "5"),
        "earned-first restore mirrors the earned-first debit rule"
    );

    credits
        .refund(&addr, "20", "checkout:formal-earned-first", None)
        .await
        .unwrap();
    let (av, ea): (String, String) = sqlx::query_as(
        "SELECT available::text, earned_available::text FROM user_credits WHERE address = $1",
    )
    .bind(&addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (av.as_str(), ea.as_str()),
        ("35", "20"),
        "spend then full refund is the identity"
    );
    assert_reconciles(&pool, &addr, "after earned-first round trip").await;

    cleanup(&pool, &addr).await;
}

/// DEFECT (order dependence). The refund clamp used to be conditional -- the
/// `CASE WHEN spend_rows > 0` branch in `wallet.rs` -- so a refund issued under a
/// `tx_ref` that had no spend rows YET was applied unclamped and then ATE the
/// window: `GREATEST(0, spent - refunded)` went negative and floored at zero, so
/// the legitimate refund that followed the real spend applied 0.
///
/// The witness: refund 100 first, then spend 50, then refund 50 -> the wallet
/// gained 100 against a cumulative spend of 50, and the honest refund got
/// nothing. The invariant is that cumulative refunds under one `tx_ref` never
/// exceed cumulative spends under it, in ANY order.
#[tokio::test]
async fn cumulative_refund_never_exceeds_cumulative_spend_in_any_order() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "0", "1000").await;
    let tx_ref = format!("checkout:formal-window-{addr}");

    // a refund BEFORE any spend under this tx_ref applies nothing at all
    let early = credits
        .refund(&addr, "100", &tx_ref, Some(&format!("t:w1-{addr}")))
        .await
        .unwrap();
    assert_eq!(
        early.applied, "0",
        "a refund with no spend to refund must apply nothing"
    );
    assert_eq!(available(&pool, &addr).await, "1000");

    credits.spend(&addr, "50", &tx_ref, None).await.unwrap();
    assert_eq!(available(&pool, &addr).await, "950");

    // ...and the honest refund that follows the real spend is NOT starved by it
    let honest = credits
        .refund(&addr, "50", &tx_ref, Some(&format!("t:w2-{addr}")))
        .await
        .unwrap();
    assert_eq!(honest.applied, "50");
    assert_eq!(
        available(&pool, &addr).await,
        "1000",
        "spend then full refund is the identity; the early refund must not have \
         consumed the window"
    );

    // over-refunding past the window is clamped, not applied
    let over = credits
        .refund(&addr, "50", &tx_ref, Some(&format!("t:w3-{addr}")))
        .await
        .unwrap();
    assert_eq!(over.applied, "0");
    assert_eq!(available(&pool, &addr).await, "1000");
    assert_reconciles(&pool, &addr, "after the refund-window round trip").await;

    cleanup(&pool, &addr).await;
}

/// REGRESSION GUARD for an undocumented database dependency, not a defect.
///
/// Two refunds racing under one `tx_ref` with DISTINCT idempotency keys -- which
/// is exactly what production does, since the reversal path uses `reversal:N`
/// and the admin path `admin:refund:N` -- are serialized only by the wallet row
/// lock plus READ COMMITTED re-evaluation. Nothing else stops them: the clamp
/// each one computes is only sound if it sees the other's write.
///
/// If someone moves the split read above the `FOR UPDATE`, or runs these at
/// REPEATABLE READ (where the second transaction would re-read its own stale
/// snapshot), this starts failing with the wallet at 200 instead of 100.
#[tokio::test]
async fn concurrent_refunds_under_one_tx_ref_cannot_double_pay() {
    let _serial = SERIAL.lock().await;
    let Some(pool) = pool().await else { return };
    let addr = scratch_addr();
    let credits = CreditsComponent::new(pool.clone());
    seed_wallet(&pool, &addr, "0", "100").await;
    let tx_ref = format!("checkout:formal-race-{addr}");

    credits.spend(&addr, "100", &tx_ref, None).await.unwrap();
    assert_eq!(available(&pool, &addr).await, "0");

    let (c1, c2) = (credits.clone(), credits.clone());
    let (a1, a2) = (addr.clone(), addr.clone());
    let (t1, t2) = (tx_ref.clone(), tx_ref.clone());
    let (k1, k2) = (format!("reversal:{addr}"), format!("admin:refund:{addr}"));
    let (r1, r2) = tokio::join!(
        tokio::spawn(async move { c1.refund(&a1, "100", &t1, Some(&k1)).await }),
        tokio::spawn(async move { c2.refund(&a2, "100", &t2, Some(&k2)).await }),
    );
    r1.unwrap().unwrap();
    r2.unwrap().unwrap();

    assert_eq!(
        available(&pool, &addr).await,
        "100",
        "two concurrent refunds of one 100-credit spend paid out twice; the \
         FOR UPDATE / READ COMMITTED dependency in wallet.rs has been broken"
    );
    assert_reconciles(&pool, &addr, "after concurrent refunds").await;

    cleanup(&pool, &addr).await;
}
