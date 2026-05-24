# Third-party Merkle proof verification

Third-party wearable / emote collections (Adidas, DolceGabbana, etc.)
are deployed by external operators. Catalyst doesn't ingest those
deployments directly; it stores a Merkle root per third-party
collection (from the on-chain registry) and verifies each deployment by
checking the deployed entity's content hash against the registry root
using a Merkle proof.

Any byte-level deviation in how we compute the leaf or combine nodes
produces a different root and rejects deploys that are valid upstream.
This page pins the byte rules.

Sources:
- `crates/catalyrst-validator/src/merkle.rs`
- `crates/catalyrst-validator/src/third_party.rs`
- `crates/catalyrst-validator/src/tp_subgraph.rs`
- Reference: `@dcl/content-hash-tree` (TS)

## Leaf hash

```
leaf = keccak256(
  solidityPacked(
    ["uint256", "string"],
    [index, contentHash]
  )
)
```

Concretely, `solidityPacked` here means `abi.encodePacked` (NOT
`abi.encode`):

- `uint256 index` — 32 bytes, **big-endian**, left-padded with zeros.
- `string contentHash` — raw UTF-8 bytes of the hash string with **NO
  length prefix**.

Use `abi.encode` and you get extra padding/length headers; use
little-endian and the bytes don't match. Either way the leaf hash is
wrong and proof verification fails for every third-party deploy.

## Node combine

```
node = keccak256(sortAndConcat(a, b))
```

`sortAndConcat`:

1. Take the two 32-byte child hashes `a` and `b`.
2. Order them by **lexicographic byte comparison** of the hash bytes
   (content-derived order — NOT by tree index, NOT by leaf index).
3. Concatenate the smaller one followed by the larger one (64 bytes total).
4. keccak256 over the 64 bytes.

**Common mistake:** ordering by tree index ("left child, right child")
matches some Merkle libraries but is NOT how `@dcl/content-hash-tree`
works. Use the byte-lex order.

## Proof verification

Siblings are folded into the leaf in **proof order**:

```
acc = leaf
for sibling in proof:
    acc = node(acc, sibling)
assert acc == root
```

Don't reverse the proof order; don't try to deduce position from
sibling values. The reference verifier is index-driven only at the
leaf-hash step; the combine step uses byte-lex.

## Zero-root sentinel

The upstream third-party registry uses `0x0000...0000` (32 zero bytes)
as the "not yet set" root. `fetch_all_third_parties` treats both:

- empty string, and
- the all-zero hex sentinel,

as `None`. Callers don't accept a deployment as proven against an unset
root.

This is a security property — without the sentinel mapping, an attacker
could trick the validator into "verifying" a Merkle proof against a
zero root (every hash trivially Merkle-verifies against zero).

## L2 block lookup window

`tp_subgraph::block_for_timestamp` queries the blocks subgraph for the
most recent block in the window:

```
[ timestamp - 5 minutes - 7 seconds,  timestamp + 8 seconds ]
```

The asymmetric offsets `(+8, -5m7s)` match the reference content-validator's
`getBlockForTimestampRange` exactly. Changing these breaks parity with
the reference behavior on edge-of-block deploys. The literal `+8` and
`60*5 - 7` constants live in `tp_subgraph.rs` (search for `+ 8` and
`60 * 5 - 7`).

## Block-pinned vs current-head verification

Two modes for resolving "what is the third-party root at the time the
entity was deployed":

| Mode             | Source                                                 | Use when                                  |
|------------------|--------------------------------------------------------|-------------------------------------------|
| current-head     | `squid_marketplace.third_party` (single row per id)    | Simple, fast; assumes operators trust the current root |
| block-pinned     | `squid_marketplace.third_party_root_change` (event log)| Operators want point-in-time correctness; needs a configured blocks-subgraph source |

`THIRD_PARTY_ROOT_SOURCE=squid` plus a configured blocks subgraph =
block-pinned. Without a blocks subgraph, falls back to current-head.

## Self-bootstrapping `third_party` table

`catalyrst-server::third_party_refresh` is a pure-Rust replacement for
running the Node squid processor solely to track third-party root
changes. It refreshes the local `third_party` table from the registry
subgraph on a slow interval (roots change rarely; default is hours, not
minutes). Decouples the registry-subgraph dependency from the deploy
hot path: the validator reads the local table, this background task
keeps it warm.

The table is self-bootstrapping — a `CREATE TABLE IF NOT EXISTS` on
service start lets a fresh deployment work without a manual seed.
