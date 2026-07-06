# Third-party Merkle proof verification - byte rules

Third-party wearable/emote collections are deployed by external operators. Catalyst stores one
Merkle root per collection (from the on-chain registry) and verifies each deployment's content
hash against it with a Merkle proof; any byte-level deviation in leaf or node computation changes
the root and rejects deploys that are valid upstream. Reference: `@dcl/content-hash-tree` (TS).
Sources: `crates/catalyrst-validator/src/{merkle,third_party,tp_subgraph}.rs`.

**Leaf hash:** `leaf = keccak256( solidityPacked(["uint256","string"], [index, contentHash]) )`

`solidityPacked` = `abi.encodePacked`, NOT `abi.encode`: `uint256 index` = 32 bytes, big-endian,
zero-left-padded; `string contentHash` = raw UTF-8 bytes, no length prefix. `abi.encode`'s
padding/length headers, or little-endian bytes, fail proof verification for every third-party deploy.

**Node combine:** `node = keccak256( sortAndConcat(a, b) )`

Order the two 32-byte children by lexicographic byte comparison (content-derived order - NOT tree
index, NOT leaf index), concatenate smaller || larger (64 bytes), keccak256. "Left child, right
child" ordering is not what `@dcl/content-hash-tree` does.

**Proof folding** - siblings fold in proof order; don't reverse, don't deduce position from
values (the reference is index-driven only at the leaf-hash step):

```
acc = leaf
for sibling in proof: acc = node(acc, sibling)
assert acc == root
```

**Zero-root sentinel:** the registry uses 32 zero bytes as "root not yet set". Both the empty
string and the all-zero hex sentinel map to `None`, and callers never accept a proof against an
unset root - otherwise an attacker could "verify" any proof against a zero root.

**L2 block lookup window:** `tp_subgraph::block_for_timestamp` queries the blocks subgraph over
`[timestamp - 5min - 7s, timestamp + 8s]`. The asymmetric `+8` / `60*5 - 7` constants match the
reference content-validator's `getBlockForTimestampRange` exactly; changing them breaks parity on
edge-of-block deploys.

**Block-pinned vs current-head root resolution:**

| Mode | Source | Use when |
|---|---|---|
| current-head | `squid_marketplace.third_party` (one row per id) | simple/fast; operators trust the current root |
| block-pinned | `squid_marketplace.third_party_root_change` (event log) | point-in-time correctness; needs a configured blocks-subgraph source |

`THIRD_PARTY_ROOT_SOURCE=squid` + a blocks subgraph = block-pinned; without a blocks subgraph it
falls back to current-head.

**Self-bootstrapping `third_party` table:** `catalyrst-server::third_party_refresh`
(`THIRD_PARTY_REFRESH_HOURS` > 0) refreshes the local table from the registry subgraph on a slow
interval, replacing the Node squid processor for root tracking and keeping the subgraph off the
deploy hot path. The table is `CREATE TABLE IF NOT EXISTS`-bootstrapped, so a fresh deployment
needs no manual seed. Operator flow: DEPLOYMENT.md section 2 - pick one writer (this refresher or
the legacy Node squid), never both.
