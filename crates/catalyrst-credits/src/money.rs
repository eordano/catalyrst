//! Exact decimal handling for Credits amounts.
//!
//! Credits are stored as PostgreSQL `NUMERIC` and carried across the wire as
//! decimal *text*, never as `f64`. Every amount that reaches a money-moving
//! statement must therefore be validated with **decimal** semantics, because
//! `f64` and `NUMERIC` disagree in both directions:
//!
//! * `f64::from_str("1e-400") == 0.0`, so an `f64` guard reads a positive
//!   NUMERIC as zero. PostgreSQL says `1e-400::numeric > 0` is TRUE (measured).
//!   A guard that decides in `f64` while the effect applies in `NUMERIC` moves
//!   the balance without writing the matching ledger row, and reconcile then
//!   diverges forever.
//! * `f64::from_str("1e400") == inf`, so an `f64` guard reads an absurd but
//!   perfectly storable NUMERIC as non-finite.
//!
//! [`CreditAmount`] is the single gate: it parses the literal with exact
//! decimal rules, rejects anything that is not a sanely bounded, non-negative,
//! finite decimal, and hands back the *original text* to bind straight to
//! `NUMERIC`. There is deliberately no float anywhere in this module. Whether
//! *zero* is additionally rejected is the caller's choice --
//! [`CreditAmount::parse_positive`] (the default) versus
//! [`CreditAmount::parse_non_negative`] -- because a zero spend is a real,
//! reachable request while a zero refund/revoke/grant is a caller bug.
//!
//! The companion rule for the *effect* side lives in the SQL: a ledger row is
//! written by a `WHERE portion > 0` predicate evaluated by PostgreSQL in
//! NUMERIC, in the same statement family that applies the balance change -- so
//! the decision and the effect can never be taken in different types again.

use crate::http::ApiError;

/// Reject `|amount| >= 10^30`. Credits are a retail currency; anything at or
/// above this magnitude is a malformed literal (e.g. `1e400`), not money.
///
/// TODO(owner-decision): these bounds are a policy choice about what counts as
/// a well-formed Credits amount, and they move two edges relative to the old
/// per-callsite `f64` guard. `1e400` stays rejected (it was rejected before, as
/// non-finite in `f64`); `1e-400` is now ACCEPTED, because PostgreSQL agrees it
/// is a positive NUMERIC and the ledger row must be written whenever the
/// balance moves. Rejecting sub-unit dust outright instead would also be
/// coherent -- but only if the same threshold is applied to the SQL `> 0`
/// predicates, or the two types diverge again.
const MAX_MAGNITUDE_EXP: i64 = 30;

/// Reject `|amount| < 10^-1000`. Well inside PostgreSQL's NUMERIC exponent
/// range (`1e-16000` parses, `1e-20000` raises "value overflows numeric
/// format"), so a `CreditAmount` never turns into a 500 at bind time.
const MIN_MAGNITUDE_EXP: i64 = -1000;

/// Longest accepted literal. Bounds parse cost and keeps the value inside
/// NUMERIC's digit limits.
const MAX_LITERAL_LEN: usize = 4096;

/// A finite, sanely bounded, non-negative decimal Credits amount.
///
/// Constructing one is the *only* supported way to get an amount into a
/// balance-moving statement. The wrapped text is the caller's original literal,
/// which PostgreSQL parses as NUMERIC losslessly.
///
/// Two constructors, because the two rules are genuinely different:
/// [`Self::parse_positive`] is the default and rejects zero, while
/// [`Self::parse_non_negative`] admits zero for the one operation where a zero
/// amount is a legitimate *request* rather than a caller bug -- see the
/// zero-spend no-op in `ports::wallet::CreditsComponent::spend_in_tx`.
/// Negatives are rejected by BOTH: the anti-mint property is about negatives,
/// and it is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditAmount {
    text: String,
    zero: bool,
}

impl CreditAmount {
    /// Parse a strictly positive decimal amount.
    ///
    /// Accepts the decimal grammar `[+-]?(digits[.digits] | .digits)[eE[+-]digits]`
    /// only. `NaN` and `Infinity` are *valid PostgreSQL NUMERIC literals* and
    /// are rejected here by construction -- the grammar has no room for them.
    pub fn parse_positive(amount: &str) -> Result<Self, ApiError> {
        let parsed = Self::parse(amount, BAD_POSITIVE_AMOUNT)?;
        if parsed.zero {
            return Err(ApiError::bad_request(BAD_POSITIVE_AMOUNT));
        }
        Ok(parsed)
    }

    /// Parse a non-negative decimal amount: everything
    /// [`Self::parse_positive`] accepts, plus exact zero.
    ///
    /// Zero is admitted so the *caller* can decide what a zero amount means
    /// rather than having the parser turn a legitimate request into a 400. The
    /// only caller that wants this is `spend`, where a zero-total checkout (an
    /// empty or all-free cart, `COALESCE(SUM(...), 0)` in `ports/checkout.rs`)
    /// is a genuine no-op. NEGATIVE, `NaN`, `Infinity` and malformed literals
    /// are rejected exactly as before.
    ///
    /// `-0` and `-0.000` are accepted as zero: PostgreSQL agrees they equal
    /// `0::numeric`, and a zero amount never reaches a bind site anyway (the
    /// caller must short-circuit on [`Self::is_zero`]).
    pub fn parse_non_negative(amount: &str) -> Result<Self, ApiError> {
        Self::parse(amount, BAD_NON_NEGATIVE_AMOUNT)
    }

    fn parse(amount: &str, err: &'static str) -> Result<Self, ApiError> {
        let d = Decimal::parse(amount).ok_or_else(|| ApiError::bad_request(err))?;
        if !d.has_nonzero_digit {
            // Exact zero (`0`, `0.00`, `0e10`, `-0`): sign is meaningless, and
            // the magnitude bound below is undefined for it.
            return Ok(CreditAmount {
                text: amount.to_string(),
                zero: true,
            });
        }
        if d.negative {
            return Err(ApiError::bad_request(err));
        }
        let mag = d.magnitude_exp();
        if !(MIN_MAGNITUDE_EXP..MAX_MAGNITUDE_EXP).contains(&mag) {
            return Err(ApiError::bad_request(err));
        }
        Ok(CreditAmount {
            text: amount.to_string(),
            zero: false,
        })
    }

    /// Exactly zero, decided by the same decimal parse that validated the
    /// literal -- never by an `f64` round-trip and never by string comparison
    /// (`"0.00" != "0"` as text, but they are the same NUMERIC).
    pub fn is_zero(&self) -> bool {
        self.zero
    }

    /// The exact literal, for binding to `NUMERIC`.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Display for CreditAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

const BAD_POSITIVE_AMOUNT: &str = "amount must be a finite decimal greater than zero \
                                   (magnitude between 1e-1000 and 1e30)";

const BAD_NON_NEGATIVE_AMOUNT: &str = "amount must be a finite decimal of zero or more \
                                       (magnitude between 1e-1000 and 1e30)";

/// Structural view of a decimal literal: sign, whether any significant digit
/// exists, and where the most significant digit sits relative to the point.
struct Decimal {
    negative: bool,
    has_nonzero_digit: bool,
    /// Index of the first non-zero digit within `int_digits ++ frac_digits`.
    first_significant: usize,
    /// Number of digits before the decimal point.
    int_len: usize,
    exp: i64,
}

impl Decimal {
    fn parse(s: &str) -> Option<Self> {
        if s.is_empty() || s.len() > MAX_LITERAL_LEN {
            return None;
        }
        let bytes = s.as_bytes();
        let mut i = 0usize;

        let negative = match bytes[0] {
            b'-' => {
                i = 1;
                true
            }
            b'+' => {
                i = 1;
                false
            }
            _ => false,
        };

        let int_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let int_len = i - int_start;

        let frac_start;
        let frac_len;
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            frac_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            frac_len = i - frac_start;
        } else {
            frac_start = i;
            frac_len = 0;
        }
        // At least one digit total; bare "." / "-" / "e5" are not numbers.
        if int_len + frac_len == 0 {
            return None;
        }

        let mut exp: i64 = 0;
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            let neg_exp = match bytes.get(i) {
                Some(b'-') => {
                    i += 1;
                    true
                }
                Some(b'+') => {
                    i += 1;
                    false
                }
                _ => false,
            };
            let digits_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == digits_start {
                return None;
            }
            // Saturating: an exponent this large is rejected by the magnitude
            // bound anyway, and saturation keeps the parse total.
            let mut v: i64 = 0;
            for b in &bytes[digits_start..i] {
                v = v.saturating_mul(10).saturating_add(i64::from(b - b'0'));
                if v > 1_000_000 {
                    v = 1_000_000;
                    break;
                }
            }
            exp = if neg_exp { -v } else { v };
        }
        // Anything left over (whitespace, "NaN", "Infinity", "0x...") is invalid.
        if i != bytes.len() {
            return None;
        }

        let digits = |idx: usize| -> u8 {
            if idx < int_len {
                bytes[int_start + idx]
            } else {
                bytes[frac_start + (idx - int_len)]
            }
        };
        let total = int_len + frac_len;
        let mut first_significant = total;
        for idx in 0..total {
            if digits(idx) != b'0' {
                first_significant = idx;
                break;
            }
        }

        Some(Decimal {
            negative,
            has_nonzero_digit: first_significant < total,
            first_significant,
            int_len,
            exp,
        })
    }

    /// Base-10 exponent of the most significant digit: the value lies in
    /// `[10^mag, 10^(mag+1))`. Only meaningful when `has_nonzero_digit`.
    fn magnitude_exp(&self) -> i64 {
        self.int_len as i64 - self.first_significant as i64 - 1 + self.exp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        CreditAmount::parse_positive(s).is_ok()
    }

    fn ok_nn(s: &str) -> bool {
        CreditAmount::parse_non_negative(s).is_ok()
    }

    #[test]
    fn accepts_ordinary_amounts() {
        for s in ["1", "25", "0.5", ".5", "+3", "12.750", "1e3", "1E3", "1e-3"] {
            assert!(ok(s), "{s:?} must be accepted");
        }
        assert_eq!(CreditAmount::parse_positive("25").unwrap().as_str(), "25");
    }

    #[test]
    fn rejects_zero_and_negative() {
        // A negative amount reaching `spend` MINTS credits: `100 >= -5` passes
        // the sufficiency check and `available := available - (-5)`.
        for s in ["-5", "-0.0001", "0", "-0", "0.0", "0.000", "0e10", "-1e-9"] {
            assert!(!ok(s), "{s:?} must be rejected");
        }
    }

    /// `parse_non_negative` moves EXACTLY one edge relative to `parse_positive`:
    /// zero. Everything else -- and in particular every negative, which is the
    /// literal that MINTS credits when it reaches `spend` -- is still rejected.
    #[test]
    fn non_negative_admits_zero_and_nothing_else_extra() {
        for s in ["0", "-0", "0.0", "0.000", "0e10", "+0", ".0"] {
            let z = CreditAmount::parse_non_negative(s)
                .unwrap_or_else(|_| panic!("{s:?} must parse as zero"));
            assert!(z.is_zero(), "{s:?} must report is_zero()");
            assert!(!ok(s), "{s:?} must still be rejected by parse_positive");
        }

        // The anti-mint property is about NEGATIVES, and it is untouched.
        for s in ["-5", "-0.0001", "-1e-9", "-1", "-0.000000001"] {
            assert!(!ok_nn(s), "{s:?} must be rejected by parse_non_negative");
            assert!(!ok(s), "{s:?} must be rejected by parse_positive");
        }

        // Non-numeric NUMERIC literals and garbage stay rejected on both paths.
        for s in [
            "NaN",
            "nan",
            "Infinity",
            "-Infinity",
            "inf",
            "1e400",
            "-1e400",
            "abc",
            "",
            " 0",
            "0 ",
            "0x0",
            "1,5",
            ".",
        ] {
            assert!(!ok_nn(s), "{s:?} must be rejected by parse_non_negative");
        }

        // Positive amounts parse identically on both paths and are NOT zero.
        for s in ["1", "0.5", "1e-400", "12.750"] {
            let a = CreditAmount::parse_non_negative(s).unwrap();
            assert!(!a.is_zero(), "{s:?} is positive");
            assert_eq!(a.as_str(), s, "the original literal is preserved");
        }
    }

    #[test]
    fn rejects_non_numeric_literals_postgres_would_accept() {
        // PostgreSQL parses these as NUMERIC; they must never reach a
        // money-moving statement.
        for s in ["NaN", "nan", "Infinity", "-Infinity", "inf"] {
            assert!(!ok(s), "{s:?} must be rejected");
        }
    }

    #[test]
    fn rejects_malformed_literals() {
        for s in [
            "", " 1", "1 ", "1,5", ".", "-", "e5", "1e", "1e+", "0x10", "1.2.3", "1d5",
        ] {
            assert!(!ok(s), "{s:?} must be rejected");
        }
    }

    #[test]
    fn magnitude_bounds_match_numeric_not_f64() {
        // `f64::from_str("1e400")` is +inf and `f64::from_str("1e-400")` is 0.0,
        // but PostgreSQL stores both as finite NUMERIC. The bound is decided on
        // the decimal exponent, not on a float round-trip.
        assert!(ok("1e29"));
        assert!(ok("999999999999999999999999999999")); // 30 digits, < 1e30
        assert!(!ok("1e30"));
        assert!(!ok("1e400"));

        // Subnormal-to-f64 but positive to NUMERIC: accepted, so that the
        // ledger-row decision (SQL `> 0`) and the guard agree.
        assert!(ok("1e-400"));
        assert!(ok(&format!("0.{}1", "0".repeat(399))));
        assert!(!ok("1e-1001"));
    }

    #[test]
    fn f64_would_disagree_on_these() {
        // Documents the exact divergence this type exists to remove.
        assert_eq!("1e-400".parse::<f64>().unwrap(), 0.0);
        assert!("1e400".parse::<f64>().unwrap().is_infinite());
        assert!(ok("1e-400"), "NUMERIC-positive amount must be accepted");
    }
}

/// Characterizes `CreditAmount` on the shared edge-input set used across all
/// decimal-string validators in this crate (see the sibling
/// `characterization_*` tests in ports/pricing.rs, ports/checkout.rs,
/// purchase_intent.rs, and handlers/packs.rs). This is the ONLY validator
/// here that: (a) accepts scientific notation (`1e18`/`1E18`), (b) has NO
/// `.trim()` -- so any surrounding whitespace is a hard reject, unlike every
/// other validator in this crate -- and (c) enforces a magnitude bound
/// (`MAX_MAGNITUDE_EXP`) instead of a raw digit-count cap, so a 50-digit
/// plain integer is rejected here but accepted by `charge_is_positive` /
/// `parse_nonneg_decimal` (which have no magnitude bound at all).
#[cfg(test)]
mod characterization_credit_amount {
    use super::CreditAmount;

    fn positive_ok(s: &str) -> bool {
        CreditAmount::parse_positive(s).is_ok()
    }

    fn non_negative_ok(s: &str) -> bool {
        CreditAmount::parse_non_negative(s).is_ok()
    }

    #[test]
    fn current_accept_reject_on_edge_inputs() {
        // Scientific notation IS accepted (the documented divergence from
        // pricing/checkout/purchase_intent, all of which reject it).
        assert!(positive_ok("1e18"));
        assert!(positive_ok("1E18"));
        assert!(non_negative_ok("1e18"));
        assert!(non_negative_ok("1E18"));

        // No .trim(): surrounding whitespace is a hard reject here, unlike
        // charge_is_positive/parse_nonneg_decimal/parse_decimal which all
        // trim before validating.
        assert!(!positive_ok(" 1.5 "));
        assert!(!non_negative_ok(" 1.5 "));

        assert!(positive_ok(".5"));
        assert!(positive_ok("5."));
        assert!(positive_ok("01.50"));

        assert!(!positive_ok(""));
        assert!(!non_negative_ok(""));
        assert!(!positive_ok("-1"));
        assert!(!non_negative_ok("-1"));
        assert!(!positive_ok("1.2.3"));
        assert!(!non_negative_ok("1.2.3"));

        // A 50-digit plain integer overflows MAX_MAGNITUDE_EXP (30) and is
        // rejected -- where pricing/checkout would accept it (no magnitude
        // bound there at all).
        let huge = "9".repeat(50);
        assert!(!positive_ok(&huge));
        assert!(!non_negative_ok(&huge));
    }
}
