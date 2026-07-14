# Third-party Merkle proofs (byte rules)

Operators deploy third-party wearable/emote collections; catalyst stores one on-chain Merkle root per collection, proof-verifying each deployment's content hash against it. Byte-level deviation in leaf or node computation changes the root, rejecting deploys valid upstream. Reference: `@dcl/content-hash-tree` (TS). Sources: `crates/catalyrst-validator/src/{merkle,third_party,tp_subgraph}.rs`.

**Leaf hash:** `leaf = keccak256( solidityPacked(["uint256","string"], [index, contentHash]) )`

`solidityPacked` = `abi.encodePacked`, NOT `abi.encode`: `uint256 index` = 32 bytes big-endian zero-left-padded; `string contentHash` = raw UTF-8, no length prefix. `abi.encode` padding/length headers, or little-endian bytes, fail every proof.

**Node combine:** `node = keccak256( sortAndConcat(a, b) )` - order children by lexicographic byte comparison (content-derived - NOT tree/leaf index), concat smaller || larger (64 bytes). Not "left child, right child".

**Proof folding** - siblings fold in proof order; don't reverse, don't deduce position from values (the reference is index-driven only at the leaf-hash step):

```
acc = leaf
for sibling in proof: acc = node(acc, sibling)
assert acc == root
```

**Zero-root sentinel:** registry's 32 zero bytes = "root not yet set". Empty string and all-zero hex map to `None`; never accept a proof against an unset root - any proof "verifies" against zero.

**L2 block lookup window:** `tp_subgraph::block_for_timestamp` queries the blocks subgraph over `[timestamp - 5min - 7s, timestamp + 8s]`. The asymmetric `+8` / `60*5 - 7` constants match the reference `getBlockForTimestampRange`; changing them breaks edge-of-block parity.

**Root resolution:**

| Mode | Source | Use when |
|---|---|---|
| current-head | `squid_marketplace.third_party` (one row per id) | simple/fast; trusts current root |
| block-pinned | `squid_marketplace.third_party_root_change` (event log) | point-in-time correctness; needs blocks-subgraph source |

`THIRD_PARTY_ROOT_SOURCE=squid` + blocks subgraph = block-pinned; otherwise, current-head.

**Self-bootstrapping `third_party` table:** `catalyrst-server::third_party_refresh` (`THIRD_PARTY_REFRESH_HOURS` > 0) refreshes local table from registry subgraph periodically - replaces the Node squid processor, keeps subgraph off the deploy hot path. `CREATE TABLE IF NOT EXISTS` bootstraps it; fresh deployments need no seed. DEPLOYMENT.md section 2: pick one writer (this refresher or the Node squid), never both.
