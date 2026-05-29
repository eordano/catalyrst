//! Big-integer arithmetic on decimal strings — mirrors the `bn.js` shape the TS
//! source uses. Marketplace volumes and Wei prices stay well below the practical
//! limits of plain string-based addition, so this avoids pulling a full bignum
//! crate just for `acc + day.volume` style reductions.

/// Add two non-negative decimal integers represented as strings. Treats empty
/// or non-numeric strings as 0 (matching `new BN('0')` fallback behaviour).
pub fn bn_add(a: &str, b: &str) -> String {
    let a = sanitize(a);
    let b = sanitize(b);
    let a_bytes: Vec<u8> = a.bytes().rev().collect();
    let b_bytes: Vec<u8> = b.bytes().rev().collect();
    let mut out = Vec::with_capacity(a_bytes.len().max(b_bytes.len()) + 1);
    let mut carry: u32 = 0;
    for i in 0..a_bytes.len().max(b_bytes.len()) {
        let av = a_bytes.get(i).map(|c| (c - b'0') as u32).unwrap_or(0);
        let bv = b_bytes.get(i).map(|c| (c - b'0') as u32).unwrap_or(0);
        let s = av + bv + carry;
        carry = s / 10;
        out.push(b'0' + (s % 10) as u8);
    }
    if carry > 0 {
        out.push(b'0' + carry as u8);
    }
    while out.len() > 1 && *out.last().unwrap() == b'0' {
        out.pop();
    }
    let s: String = out.into_iter().rev().map(|b| b as char).collect();
    s
}

/// Compare two non-negative decimal integer strings. Returns Less / Equal / Greater.
pub fn bn_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a = sanitize(a);
    let b = sanitize(b);
    a.len().cmp(&b.len()).then_with(|| a.cmp(&b))
}

fn sanitize(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return "0".to_string();
    }
    // strip leading zeros
    let no_lead = trimmed.trim_start_matches('0');
    if no_lead.is_empty() {
        "0".to_string()
    } else {
        no_lead.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_zero_correctly() {
        assert_eq!(bn_add("0", "0"), "0");
        assert_eq!(bn_add("", "0"), "0");
        assert_eq!(bn_add("10", "5"), "15");
    }

    #[test]
    fn adds_large() {
        assert_eq!(
            bn_add("999999999999999999999", "1"),
            "1000000000000000000000"
        );
    }

    #[test]
    fn compares() {
        assert_eq!(bn_cmp("10", "9"), std::cmp::Ordering::Greater);
        assert_eq!(bn_cmp("100", "100"), std::cmp::Ordering::Equal);
    }
}
