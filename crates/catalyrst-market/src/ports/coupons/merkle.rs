use std::collections::HashSet;
use std::str::FromStr;

use alloy_primitives::{keccak256, Address};
use catalyrst_crypto::eip712::word_address;

use super::errors::NO_COLLECTIONS;

/// The Merkle tree a CollectionDiscountCoupon is verified against: one leaf per collection
/// address, hashed the way the contract hashes it -- keccak256(keccak256(abi.encode(address)))
/// -- which is OpenZeppelin's standard leaf for the single-value type `['address']`.
///
/// The layout is OpenZeppelin's StandardMerkleTree and not a hand-rolled one. The contract only
/// calls MerkleProof.verify, which folds a proof without caring how the tree was laid out, so
/// any self-consistent tree settles on chain -- but the root is a contract between whoever signs
/// the coupon and whoever rebuilds it here, and a tree that carries odd nodes up diverges from
/// StandardMerkleTree at five, seven and nine leaves while agreeing at four, six and eight. Both
/// the shop and marketplace-server build it with @openzeppelin/merkle-tree, so this reproduces
/// that algorithm exactly: leaves sorted ascending, laid into a 2n-1 node array from the end,
/// parents hashed over the sorted pair.
#[derive(Debug, PartialEq, Eq)]
pub enum MerkleError {
    NoCollections,
    NotCovered(String),
    InvalidAddress(String),
}

impl std::fmt::Display for MerkleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MerkleError::NoCollections => write!(f, "{NO_COLLECTIONS}"),
            MerkleError::NotCovered(collection) => {
                write!(
                    f,
                    "The collection {collection} is not covered by this coupon"
                )
            }
            MerkleError::InvalidAddress(value) => {
                write!(f, "{value} is not a collection address")
            }
        }
    }
}

/// Lower-cased and de-duplicated, in input order.
///
/// The coupon that gets STORED and the Merkle root the contract VERIFIES have to be built from
/// the same list, so both go through here.
pub fn unique_collections(collections: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(collections.len());
    for collection in collections {
        let lower = collection.to_lowercase();
        if seen.insert(lower.clone()) {
            out.push(lower);
        }
    }
    out
}

/// The leaf the contract computes for a collection, spelled out as the contract spells it.
pub fn collection_leaf(collection: &str) -> Result<[u8; 32], MerkleError> {
    let address = Address::from_str(&collection.to_lowercase())
        .map_err(|_| MerkleError::InvalidAddress(collection.to_string()))?;
    Ok(keccak256(keccak256(word_address(address))).0)
}

fn node_hash(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&first);
    buf[32..].copy_from_slice(&second);
    keccak256(buf).0
}

struct Tree {
    nodes: Vec<[u8; 32]>,
    leaves: Vec<[u8; 32]>,
}

fn build_tree(collections: &[String]) -> Result<Tree, MerkleError> {
    let unique = unique_collections(collections);
    if unique.is_empty() {
        return Err(MerkleError::NoCollections);
    }
    let mut leaves = unique
        .iter()
        .map(|collection| collection_leaf(collection))
        .collect::<Result<Vec<_>, _>>()?;
    leaves.sort();

    let count = leaves.len();
    let mut nodes = vec![[0u8; 32]; 2 * count - 1];
    for (i, leaf) in leaves.iter().enumerate() {
        let last = nodes.len() - 1;
        nodes[last - i] = *leaf;
    }
    for i in (0..count - 1).rev() {
        nodes[i] = node_hash(nodes[2 * i + 1], nodes[2 * i + 2]);
    }
    Ok(Tree { nodes, leaves })
}

/// The root for a set of collections. Errors on an empty set: a coupon has to cover something.
pub fn collections_root(collections: &[String]) -> Result<[u8; 32], MerkleError> {
    Ok(build_tree(collections)?.nodes[0])
}

/// The proof a buyer passes in `callerData` for one collection of the set. Empty for a
/// one-collection coupon.
pub fn collection_proof(
    collections: &[String],
    collection: &str,
) -> Result<Vec<[u8; 32]>, MerkleError> {
    let tree = build_tree(collections)?;
    let leaf = collection_leaf(collection)?;
    let position = tree
        .leaves
        .iter()
        .position(|candidate| *candidate == leaf)
        .ok_or_else(|| MerkleError::NotCovered(collection.to_string()))?;

    let mut index = tree.nodes.len() - 1 - position;
    let mut proof = Vec::new();
    while index > 0 {
        let sibling = if index % 2 == 1 { index + 1 } else { index - 1 };
        proof.push(tree.nodes[sibling]);
        index = (index - 1) / 2;
    }
    Ok(proof)
}

/// Recomputes the root from a leaf and its proof, the way the contract does.
pub fn verify_collection_proof(root: [u8; 32], collection: &str, proof: &[[u8; 32]]) -> bool {
    let Ok(leaf) = collection_leaf(collection) else {
        return false;
    };
    proof.iter().fold(leaf, |acc, node| node_hash(acc, *node)) == root
}

pub fn from_hex32(value: &str) -> Option<[u8; 32]> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .ok()?
        .try_into()
        .ok()
}

pub fn to_hex32(value: [u8; 32]) -> String {
    format!("0x{}", hex::encode(value))
}

/// The shop catalogue embeds one of these per discounted listing, so the buy side never rebuilds
/// the tree.
pub fn collection_proof_hex(
    collections: &[String],
    collection: &str,
) -> Result<Vec<String>, MerkleError> {
    Ok(collection_proof(collections, collection)?
        .into_iter()
        .map(to_hex32)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five collections and the root upstream pins in test/unit/coupons-logic.spec.ts.
    const COLLECTIONS: [&str; 5] = [
        "0x4c09495cd2d4e3d3fa2808eb655d013de426157b",
        "0xb0d0d31910da4a14d4e05a9d51b6e9a99a85d676",
        "0x7079fda5934f9bdcdfbe9c84e286a04dadfeb9e4",
        "0x96054dc54939d3c632796dbce4884705ed7c8977",
        "0xf8a87150ca602dbeb2e748ad7c9c790d55d10528",
    ];

    fn owned(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// Pinned on purpose: every other test here is shape-agnostic and would pass under any
    /// self-consistent tree. This value is the StandardMerkleTree root, which is what a client
    /// signs. A tree that pairs level by level and carries an odd node up returns
    /// 0xedc52722864df2a9a75b36296621e7ca7314f4211333ee61b2017ca50b13b967 for these same five
    /// collections and would reject every coupon.
    #[test]
    fn the_root_is_the_one_openzeppelin_standard_merkle_tree_builds() {
        assert_eq!(
            to_hex32(collections_root(&owned(&COLLECTIONS)).unwrap()),
            "0xbb275d33d9fbb90ff34fd53181283e8cdebb0dd8e764f5cb6852d976a4495fa9"
        );
    }

    #[test]
    fn a_single_collection_roots_at_its_own_leaf_with_an_empty_proof() {
        let one = owned(&COLLECTIONS[..1]);
        let leaf = collection_leaf(COLLECTIONS[0]).unwrap();
        assert_eq!(collections_root(&one).unwrap(), leaf);
        assert!(collection_proof(&one, COLLECTIONS[0]).unwrap().is_empty());
    }

    #[test]
    fn two_collections_hash_as_the_sorted_pair() {
        let a = collection_leaf(COLLECTIONS[0]).unwrap();
        let b = collection_leaf(COLLECTIONS[1]).unwrap();
        assert_eq!(
            collections_root(&owned(&COLLECTIONS[..2])).unwrap(),
            node_hash(a, b)
        );
    }

    #[test]
    fn every_proof_recomputes_the_root() {
        for size in 1..=COLLECTIONS.len() {
            let set = owned(&COLLECTIONS[..size]);
            let root = collections_root(&set).unwrap();
            for collection in &set {
                let proof = collection_proof(&set, collection).unwrap();
                assert!(
                    verify_collection_proof(root, collection, &proof),
                    "{size} leaves, {collection}"
                );
            }
        }
    }

    #[test]
    fn a_proof_for_a_collection_outside_the_set_neither_builds_nor_verifies() {
        let set = owned(&COLLECTIONS);
        let root = collections_root(&set).unwrap();
        let outsider = "0x0000000000000000000000000000000000000001";
        assert_eq!(
            collection_proof(&set, outsider),
            Err(MerkleError::NotCovered(outsider.to_string()))
        );
        let borrowed = collection_proof(&set, COLLECTIONS[0]).unwrap();
        assert!(!verify_collection_proof(root, outsider, &borrowed));
    }

    #[test]
    fn the_root_ignores_order_casing_and_duplicates() {
        let mut shuffled: Vec<String> = COLLECTIONS
            .iter()
            .rev()
            .map(|c| format!("0x{}", c[2..].to_uppercase()))
            .collect();
        shuffled.push(COLLECTIONS[2].to_string());
        assert_eq!(
            collections_root(&shuffled).unwrap(),
            collections_root(&owned(&COLLECTIONS)).unwrap()
        );
    }

    #[test]
    fn a_coupon_must_cover_something() {
        assert_eq!(collections_root(&[]), Err(MerkleError::NoCollections));
        assert_eq!(
            collections_root(&[]).unwrap_err().to_string(),
            NO_COLLECTIONS
        );
    }

    #[test]
    fn a_malformed_collection_is_not_an_address() {
        assert_eq!(
            collection_leaf("not-an-address"),
            Err(MerkleError::InvalidAddress("not-an-address".to_string()))
        );
    }
}
