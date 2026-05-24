use ethers_core::utils::keccak256;

pub fn to_node(index: u64, content_hash: &str) -> [u8; 32] {
    let mut packed = Vec::with_capacity(32 + content_hash.len());
    packed.extend_from_slice(&[0u8; 24]);
    packed.extend_from_slice(&index.to_be_bytes());
    packed.extend_from_slice(content_hash.as_bytes());
    keccak256(packed)
}

pub fn combined_hash(first: [u8; 32], second: [u8; 32]) -> [u8; 32] {
    let (lo, hi) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&lo);
    buf[32..].copy_from_slice(&hi);
    keccak256(buf)
}

pub fn generate_root(index: u64, content_hash: &str, proof: &[[u8; 32]]) -> [u8; 32] {
    let mut pair = to_node(index, content_hash);
    for sibling in proof {
        pair = combined_hash(pair, *sibling);
    }
    pair
}

pub fn verify_proof(index: u64, content_hash: &str, proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
    &generate_root(index, content_hash, proof) == root
}

pub fn decode_hash32(s: &str) -> Option<[u8; 32]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTORS: &[(u64, &str, &[&str])] = &[
        (
            0,
            "QmTQZXMoieaUbTVmvoL1wgTfuxFMTCgrTfSoLD5sJQr1E4",
            &[
                "0x48c0ca76d7c07296b6ab47abc261e5ff457183e7e0267e62942b32e33c00ca0a",
                "0x884f61ae97ac3c79a2c8252e53e80fc4fdd0a12c4ca6c6b6aac6b9838b2bd5be",
            ],
        ),
        (
            1,
            "QmTZzpvVdNbXXMrakkZvGnapuwJRAos9e4VnfCBcNva78N",
            &["0x796a113deaa9b91240bca898272af6c57fa81c48d7349336b19a1ca3bd9ef321"],
        ),
        (
            2,
            "QmVxw4Nvg4FMTcCwa7doiNZpshLJbCq8dokzgRTPGtqUy8",
            &[
                "0x70718eb9ee724e904cfafa344172fd064aa8432ff88b313e1ecde6cb31e689f9",
                "0x884f61ae97ac3c79a2c8252e53e80fc4fdd0a12c4ca6c6b6aac6b9838b2bd5be",
            ],
        ),
    ];

    fn proof_of(strs: &[&str]) -> Vec<[u8; 32]> {
        strs.iter().map(|s| decode_hash32(s).unwrap()).collect()
    }

    #[test]
    fn all_members_regenerate_one_shared_root() {
        let roots: Vec<[u8; 32]> = VECTORS
            .iter()
            .map(|(idx, hash, proof)| generate_root(*idx, hash, &proof_of(proof)))
            .collect();

        assert_eq!(roots[0], roots[1]);
        assert_eq!(roots[1], roots[2]);

        let root = roots[0];
        for (idx, hash, proof) in VECTORS {
            assert!(verify_proof(*idx, hash, &proof_of(proof), &root));
        }
    }

    #[test]
    fn tampered_proof_is_rejected() {
        let (idx, hash, proof) = VECTORS[0];
        let root = generate_root(idx, hash, &proof_of(proof));

        assert!(!verify_proof(idx + 1, hash, &proof_of(proof), &root));
        assert!(!verify_proof(idx, "QmDifferentContentHashAAAAAAAAAAAAAAAAAAAAAAAA", &proof_of(proof), &root));

        let mut bad = proof_of(proof);
        bad[0][0] ^= 0xff;
        assert!(!verify_proof(idx, hash, &bad, &root));
    }
}
