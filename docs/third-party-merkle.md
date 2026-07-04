# Third-party Merkle proof verification — byte rules

> Status: distilled 2026-07-04; invariants last re-verified against code
> 2026-07-03 (docs-stale-audit).

Third-party wearable/emote collections (Adidas, DolceGabbana, …) are deployed
by external operators. Catalyst stores one Merkle root per collection (from the
on-chain registry) and verifies each deployment's content hash against that
root with a Merkle proof. Any byte-level deviation in leaf or node computation
produces a different root and rejects deploys that are valid upstream. This
page pins the byte rules; the reference is `@dcl/content-hash-tree` (TS).

Sources: `crates/catalyrst-validator/src/{merkle,third_party,tp_subgraph}.rs`.

## Leaf hash

```
leaf = keccak256( solidityPacked(["uint256","string"], [index, contentHash]) )
```

`solidityPacked` = `abi.encodePacked`, NOT `abi.encode`:

- `uint256 index` — 32 bytes, big-endian, zero-left-padded;
- `string contentHash` — raw UTF-8 bytes, **no length prefix**.

`abi.encode` adds padding/length headers; little-endian flips the bytes.
Either mistake fails proof verification for every third-party deploy.

## Node combine

```
node = keccak256( sortAndConcat(a, b) )
```

Order the two 32-byte children by **lexicographic byte comparison**
(content-derived order — NOT tree index, NOT leaf index), concatenate
smaller‖larger (64 bytes), keccak256. "Left child, right child" ordering
matches some Merkle libraries but is not how `@dcl/content-hash-tree` works.

## Proof folding

```
acc = leaf
for sibling in proof: acc = node(acc, sibling)
assert acc == root
```

Siblings fold in proof order. Don't reverse; don't deduce position from
values — the reference is index-driven only at the leaf-hash step.

## Zero-root sentinel — a security property

The registry uses 32 zero bytes as "root not yet set". Both the empty string
and the all-zero hex sentinel map to `None`, and callers never accept a proof
against an unset root. Without this, an attacker could "verify" any proof
against a zero root.

## L2 block lookup window

`tp_subgraph::block_for_timestamp` queries the blocks subgraph over
`[timestamp − 5min − 7s, timestamp + 8s]`. The asymmetric `+8` / `60*5 − 7`
constants match the reference content-validator's `getBlockForTimestampRange`
exactly; changing them breaks parity on edge-of-block deploys.

## Block-pinned vs current-head root resolution

| Mode | Source | Use when |
|---|---|---|
| current-head | `squid_marketplace.third_party` (one row per id) | simple/fast; operators trust the current root |
| block-pinned | `squid_marketplace.third_party_root_change` (event log) | point-in-time correctness; needs a configured blocks-subgraph source |

`THIRD_PARTY_ROOT_SOURCE=squid` + a blocks subgraph = block-pinned; without a
blocks subgraph it falls back to current-head.

## Self-bootstrapping `third_party` table

`catalyrst-server::third_party_refresh` (`THIRD_PARTY_REFRESH_HOURS` > 0) is a
pure-Rust replacement for running the Node squid processor just to track root
changes: it refreshes the local table from the registry subgraph on a slow
interval (roots change rarely), decoupling the subgraph dependency from the
deploy hot path. The table is `CREATE TABLE IF NOT EXISTS`-bootstrapped, so a
fresh deployment works without a manual seed. See DEPLOYMENT.md §2 for the
operator flow (and the legacy Node-squid alternative — pick one writer, never
both).
