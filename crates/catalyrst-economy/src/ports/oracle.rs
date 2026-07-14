use alloy::primitives::{address, Address, Bytes, U256};
use alloy::sol;
use alloy::sol_types::SolCall;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::http::errors::ApiError;
use crate::ports::signer::DirectSigner;

pub const MANA_USD_AGGREGATOR_POLYGON: Address =
    address!("0xA1CbF3Fe43BC3501e3Fc4b573e822c70e76A7512");

pub const ONCHAIN_MANA_USD_STALE_TOLERANCE_SECS: u64 = 60;

const WAD: U256 = U256::from_limbs([1_000_000_000_000_000_000u64, 0, 0, 0]);

const BPS_DENOMINATOR: u64 = 10_000;

static STALE_REFUSALS: AtomicU64 = AtomicU64::new(0);

pub fn stale_refusal_count() -> u64 {
    STALE_REFUSALS.load(Ordering::Relaxed)
}

pub fn exceeds_onchain_stale_tolerance(max_age_secs: u64) -> bool {
    max_age_secs > ONCHAIN_MANA_USD_STALE_TOLERANCE_SECS
}

sol! {
    function latestRoundData() external view returns (uint80 roundId, int256 answer, uint256 startedAt, uint256 updatedAt, uint80 answeredInRound);
    function decimals() external view returns (uint8);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManaUsdRate {
    pub rate_18: U256,
    pub updated_at_s: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct UsdPeggedExpectation {
    pub rate: ManaUsdRate,
    pub max_age_secs: u64,
    pub slippage_bps: u64,
    pub quote_mana_wei: Option<U256>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsdPeggedCharge {
    pub usd_wei: U256,
    pub rate_18: U256,
    pub rate_updated_at_s: i64,
    pub mana_wei: U256,
}

fn err(msg: impl Into<String>) -> ApiError {
    ApiError::InvalidTransaction(msg.into())
}

pub fn decode_rate_reading(decimals_ret: &[u8], round_ret: &[u8]) -> Result<ManaUsdRate, ApiError> {
    let feed_decimals = decimalsCall::abi_decode_returns(decimals_ret)
        .map_err(|e| ApiError::RelayerFailed(format!("could not decode decimals(): {e}")))?;
    let round = latestRoundDataCall::abi_decode_returns(round_ret)
        .map_err(|e| ApiError::RelayerFailed(format!("could not decode latestRoundData(): {e}")))?;
    normalize_rate(feed_decimals, round.answer, round.updatedAt)
}

pub fn normalize_rate(
    feed_decimals: u8,
    answer: alloy::primitives::I256,
    updated_at: U256,
) -> Result<ManaUsdRate, ApiError> {
    if answer.is_negative() || answer.is_zero() {
        return Err(err(format!(
            "MANA/USD aggregator answer {answer} is not a positive rate; refusing"
        )));
    }
    if feed_decimals > 18 {
        return Err(err(format!(
            "MANA/USD aggregator reports {feed_decimals} decimals (>18); refusing"
        )));
    }
    let scale = U256::from(10u64).pow(U256::from(18 - feed_decimals as u64));
    let rate_18 = answer
        .into_raw()
        .checked_mul(scale)
        .ok_or_else(|| err("MANA/USD rate normalization overflowed; refusing"))?;
    let updated_at_s = u64::try_from(updated_at)
        .ok()
        .and_then(|v| i64::try_from(v).ok())
        .ok_or_else(|| {
            err(format!(
                "MANA/USD aggregator updatedAt {updated_at} is not a sane timestamp; refusing"
            ))
        })?;
    Ok(ManaUsdRate {
        rate_18,
        updated_at_s,
    })
}

pub async fn fetch_mana_usd_rate(
    signer: &DirectSigner,
    aggregator: Address,
) -> Result<ManaUsdRate, ApiError> {
    let decimals_ret = signer
        .eth_call(aggregator, Bytes::from(decimalsCall {}.abi_encode()))
        .await?;
    let round_ret = signer
        .eth_call(aggregator, Bytes::from(latestRoundDataCall {}.abi_encode()))
        .await?;
    decode_rate_reading(&decimals_ret, &round_ret)
}

pub fn ensure_fresh(rate: &ManaUsdRate, now_s: i64, max_age_secs: u64) -> Result<(), ApiError> {
    let age = now_s as i128 - rate.updated_at_s as i128;
    if age > max_age_secs as i128 {
        let refusals = STALE_REFUSALS.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::warn!(
            %age,
            max_age_secs,
            refusals,
            "refusing a USD-pegged conversion on a stale MANA/USD oracle round; \
             if this recurs the feed cadence has slowed past USD_PEGGED_ORACLE_MAX_AGE_SECS"
        );
        return Err(ApiError::Conflict(format!(
            "MANA/USD oracle round is stale ({age}s old, max {max_age_secs}s; \
             USD_PEGGED_ORACLE_MAX_AGE_SECS); refusing to convert at a stale rate"
        )));
    }
    Ok(())
}

pub fn usd_to_mana_wei(usd_wei: U256, rate_18: U256) -> Result<U256, ApiError> {
    if rate_18.is_zero() {
        return Err(err("MANA/USD rate is 0; refusing"));
    }
    let scaled = usd_wei
        .checked_mul(WAD)
        .ok_or_else(|| err("USD amount too large to convert (mul overflow); refusing"))?;
    Ok(scaled / rate_18)
}

pub fn within_slippage_bps(exec_mana: U256, quote_mana: U256, tolerance_bps: u64) -> bool {
    if quote_mana.is_zero() {
        return false;
    }
    let diff = if exec_mana >= quote_mana {
        exec_mana - quote_mana
    } else {
        quote_mana - exec_mana
    };
    match (
        diff.checked_mul(U256::from(BPS_DENOMINATOR)),
        quote_mana.checked_mul(U256::from(tolerance_bps)),
    ) {
        (Some(scaled_diff), Some(bound)) => scaled_diff <= bound,
        _ => false,
    }
}

pub fn charge_basis_for_usd_pegged(
    usd_wei: U256,
    exp: &UsdPeggedExpectation,
    now_s: i64,
) -> Result<UsdPeggedCharge, ApiError> {
    ensure_fresh(&exp.rate, now_s, exp.max_age_secs)?;
    let mana_wei = usd_to_mana_wei(usd_wei, exp.rate.rate_18)?;
    if mana_wei.is_zero() {
        return Err(err(format!(
            "USD amount {usd_wei} converts to 0 MANA at rate {}; refusing a free 'sale'",
            exp.rate.rate_18
        )));
    }
    if let Some(quote) = exp.quote_mana_wei {
        if !within_slippage_bps(mana_wei, quote, exp.slippage_bps) {
            return Err(ApiError::Conflict(format!(
                "MANA/USD rate moved beyond the slippage bound: execution-time charge is \
                 {mana_wei} wei MANA vs the listing-time quote {quote} wei (tolerance {} bps; \
                 USD_PEGGED_SLIPPAGE_BPS); re-quote and retry",
                exp.slippage_bps
            )));
        }
    }
    Ok(UsdPeggedCharge {
        usd_wei,
        rate_18: exp.rate.rate_18,
        rate_updated_at_s: exp.rate.updated_at_s,
        mana_wei,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::I256;

    fn wei(n: u64) -> U256 {
        U256::from(n)
    }

    fn eth(n: u64) -> U256 {
        U256::from(n) * WAD
    }

    fn word(v: U256) -> [u8; 32] {
        v.to_be_bytes::<32>()
    }

    fn encode_round(
        round_id: u64,
        answer: I256,
        started: u64,
        updated: u64,
        in_round: u64,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 * 32);
        out.extend_from_slice(&word(U256::from(round_id)));
        out.extend_from_slice(&answer.to_be_bytes::<32>());
        out.extend_from_slice(&word(U256::from(started)));
        out.extend_from_slice(&word(U256::from(updated)));
        out.extend_from_slice(&word(U256::from(in_round)));
        out
    }

    fn encode_decimals(d: u8) -> Vec<u8> {
        word(U256::from(d)).to_vec()
    }

    fn rate(rate_18: U256, updated_at_s: i64) -> ManaUsdRate {
        ManaUsdRate {
            rate_18,
            updated_at_s,
        }
    }

    fn expectation(rate_18: U256, updated_at_s: i64, quote: Option<U256>) -> UsdPeggedExpectation {
        UsdPeggedExpectation {
            rate: rate(rate_18, updated_at_s),
            max_age_secs: 60,
            slippage_bps: 100,
            quote_mana_wei: quote,
        }
    }

    #[test]
    fn chainlink_8_decimal_answer_normalizes_to_18() {
        let decimals_ret = encode_decimals(8);
        let round_ret = encode_round(1, I256::try_from(25_000_000i64).unwrap(), 1_000, 1_000, 1);
        let got = decode_rate_reading(&decimals_ret, &round_ret).unwrap();
        assert_eq!(got.rate_18, U256::from(250_000_000_000_000_000u64));
        assert_eq!(got.updated_at_s, 1_000);
    }

    #[test]
    fn eighteen_decimal_feed_passes_through_unscaled() {
        let got = normalize_rate(
            18,
            I256::try_from(250_000_000_000_000_000u128).unwrap(),
            U256::from(5u64),
        )
        .unwrap();
        assert_eq!(got.rate_18, U256::from(250_000_000_000_000_000u64));
    }

    #[test]
    fn negative_zero_and_wide_decimals_answers_are_refused() {
        assert!(normalize_rate(8, I256::try_from(-1i64).unwrap(), U256::ZERO).is_err());
        assert!(normalize_rate(8, I256::ZERO, U256::ZERO).is_err());
        assert!(normalize_rate(19, I256::ONE, U256::ZERO).is_err());
    }

    #[test]
    fn absurd_updated_at_is_refused_not_wrapped() {
        assert!(normalize_rate(8, I256::ONE, U256::MAX).is_err());
        assert!(normalize_rate(8, I256::ONE, U256::from(u64::MAX)).is_err());
    }

    #[test]
    fn garbage_abi_bytes_are_refused() {
        assert!(decode_rate_reading(&[0u8; 3], &encode_round(1, I256::ONE, 0, 0, 1)).is_err());
        assert!(decode_rate_reading(&encode_decimals(8), &[0u8; 7]).is_err());
    }

    #[test]
    fn staleness_bound_is_inclusive_and_tolerates_future_rounds() {
        let r = rate(WAD, 1_000);
        assert!(ensure_fresh(&r, 1_060, 60).is_ok());
        assert!(ensure_fresh(&r, 1_061, 60).is_err());
        assert!(ensure_fresh(&r, 900, 60).is_ok());
        let old = rate(WAD, i64::MIN);
        assert!(ensure_fresh(&old, i64::MAX, 60).is_err());
    }

    #[test]
    fn stale_refusals_increment_the_observable_counter() {
        let r = rate(WAD, 1_000);
        let before = stale_refusal_count();
        for _ in 0..3 {
            assert!(ensure_fresh(&r, 1_061, 60).is_err());
        }
        assert!(stale_refusal_count() >= before + 3);
    }

    #[test]
    fn onchain_stale_tolerance_bounds_the_local_max_age() {
        assert_eq!(ONCHAIN_MANA_USD_STALE_TOLERANCE_SECS, 60);
        assert!(!exceeds_onchain_stale_tolerance(59));
        assert!(!exceeds_onchain_stale_tolerance(60));
        assert!(exceeds_onchain_stale_tolerance(61));
    }

    #[test]
    fn usd_to_mana_matches_the_contract_floor_division() {
        assert_eq!(
            usd_to_mana_wei(eth(2), U256::from(250_000_000_000_000_000u64)).unwrap(),
            eth(8)
        );
        assert_eq!(
            usd_to_mana_wei(wei(1), U256::from(300_000_000_000_000_000u64)).unwrap(),
            wei(3)
        );
        assert_eq!(usd_to_mana_wei(eth(1), WAD).unwrap(), eth(1));
        assert!(usd_to_mana_wei(eth(1), U256::ZERO).is_err());
        assert!(usd_to_mana_wei(U256::MAX, WAD).is_err());
    }

    #[test]
    fn slippage_bound_is_inclusive_both_directions() {
        assert!(within_slippage_bps(wei(10_100), wei(10_000), 100));
        assert!(!within_slippage_bps(wei(10_101), wei(10_000), 100));
        assert!(within_slippage_bps(wei(9_900), wei(10_000), 100));
        assert!(!within_slippage_bps(wei(9_899), wei(10_000), 100));
        assert!(within_slippage_bps(wei(10_000), wei(10_000), 0));
        assert!(!within_slippage_bps(wei(10_001), wei(10_000), 0));
        assert!(!within_slippage_bps(wei(1), U256::ZERO, 100));
        assert!(!within_slippage_bps(U256::MAX, wei(1), 100));
    }

    #[test]
    fn charge_basis_converts_at_the_execution_rate() {
        let exp = expectation(U256::from(250_000_000_000_000_000u64), 1_000, Some(eth(8)));
        let got = charge_basis_for_usd_pegged(eth(2), &exp, 1_030).unwrap();
        assert_eq!(got.mana_wei, eth(8));
        assert_eq!(got.usd_wei, eth(2));
        assert_eq!(got.rate_18, U256::from(250_000_000_000_000_000u64));
        assert_eq!(got.rate_updated_at_s, 1_000);
    }

    #[test]
    fn charge_basis_refuses_a_stale_round() {
        let exp = expectation(U256::from(250_000_000_000_000_000u64), 1_000, Some(eth(8)));
        let err = charge_basis_for_usd_pegged(eth(2), &exp, 1_061).unwrap_err();
        assert!(format!("{err}").contains("stale"), "got: {err}");
    }

    #[test]
    fn charge_basis_refuses_when_the_rate_moved_beyond_the_quote() {
        let exp = expectation(U256::from(250_000_000_000_000_000u64), 1_000, Some(eth(7)));
        let err = charge_basis_for_usd_pegged(eth(2), &exp, 1_030).unwrap_err();
        assert!(format!("{err}").contains("slippage"), "got: {err}");
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[test]
    fn charge_basis_without_a_quote_skips_the_slippage_bound() {
        let exp = expectation(U256::from(250_000_000_000_000_000u64), 1_000, None);
        let got = charge_basis_for_usd_pegged(eth(2), &exp, 1_030).unwrap();
        assert_eq!(got.mana_wei, eth(8));
    }

    #[test]
    fn charge_basis_refuses_a_zero_mana_conversion() {
        let exp = expectation(eth(1_000_000), 1_000, None);
        let err = charge_basis_for_usd_pegged(U256::ZERO, &exp, 1_030).unwrap_err();
        assert!(format!("{err}").contains("0 MANA"), "got: {err}");
    }
}
