# Auth chain validation (EIP-1271 / EIP-1654)

Every deployment to `POST /content/entities` is signed by a chain of
authorizations rooted in an Ethereum-controlled address. Validation MUST
be fail-closed: an unverifiable chain must reject, never pass-through.

Sources:
- `crates/catalyrst-crypto/src/auth_chain.rs`
- `crates/catalyrst-crypto/src/verify.rs`
- `crates/catalyrst-crypto/src/eip1654.rs`
- `crates/catalyrst-crypto/src/helios_validator.rs`
- `crates/catalyrst-crypto/src/rpc_validator.rs`
- `crates/catalyrst-crypto/src/validation_cache.rs`

## Two verifier surfaces — pick the right one

Two top-level functions in `verify.rs`:

| Function                                | Smart-wallet (EIP-1654)? | When to call                                                       |
|-----------------------------------------|--------------------------|--------------------------------------------------------------------|
| `verify_auth_chain_with_validator`      | **REJECTS** unconditionally | Sync code paths where no async is available; tests; legacy        |
| `verify_auth_chain_async`               | Validates via the supplied async validator | The real deploy hot path; any flow that must accept smart wallets |

**Critical:** the synchronous verifier ignores its `_eip1654_validator`
parameter and unconditionally returns `Eip1654NotImplemented` for any
chain containing an EIP-1654 link. The parameter exists only for
signature compatibility. **Callers that must support smart-wallet auth
chains MUST use `verify_auth_chain_async`** — otherwise smart-wallet
signatures will be silently rejected even when an RPC validator is wired
in.

This is a fail-closed property: the sync path will never accept an
EIP-1654 chain, even if the caller forgot to pass a validator. There
is no foot-gun where forgetting an arg accidentally bypasses
validation.

## TLS requirement for `ETH_RPC_URL`

`ETH_RPC_URL` gates EIP-1654 signature validity. A MITM'd endpoint can
forge `isValidSignature` returns and thereby authorize deployments under
any contract wallet's address.

`bin/live.rs` **refuses plaintext `http://`** at startup. The endpoint
MUST be `https://` AND a trusted operator (Decentraland's
`rpc.decentraland.org`, your own node, or a similarly-vetted provider).

Do NOT remove the startup check.

## Fail-closed defaults

- `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` defaults to **false**. Only an
  explicit `=true` enables the bypass. An unset flag MUST NOT bypass
  on-chain ownership / access checks.
- `THIRD_PARTY_ROOT_SOURCE` — if neither the squid DB nor the registry
  subgraph is configured, third-party access must reject (return
  `Ok(false)`). Never trust any root by default.
  (`squid_checker.rs::ThirdPartyChecker`.)
- Auth-chain link decoding (`write_deployer.rs::validator_chain_to_crypto`)
  uses JSON round-trip and rejects unknown link types rather than
  silently dropping them.

## Validators

### `rpc_validator` — on-chain `isValidSignature`

ABI-encoded `isValidSignature(bytes32 hash, bytes signature)` call via
`eth_call` to a configured RPC. Returns `true` iff the contract returns
the EIP-1271 magic value `0x1626ba7e`.

ABI padding note: dynamic args are 32-byte-aligned; the encoder ceils
each variable-length section to the next 32 bytes. (This is structural
EIP-1271 encoding; it's not a catalyrst quirk.)

### `helios_validator` — trustless RPC

Helios is a light client that verifies the RPC's responses against
Ethereum consensus. Used when the operator wants to validate without
trusting a single RPC provider. Slower and heavier than `rpc_validator`.

### `validation_cache` — short-circuit redundant `eth_call`s

Caches `(contract, hash, signature) → bool` outcomes with a short TTL.
Burst loads of the same auth chain (e.g. profile re-fetch) don't fan
out to the RPC. Cache is in-process; size-bounded; failures aren't
cached negatively for long (RPC blip should not durably reject a valid
sig).

## Recovery (`recover.rs`)

Standard ECDSA recovery from `(r, s, v)`. `v` may be `27/28` or `0/1`;
normalize to `0/1` before passing to `secp256k1::recover_id`. Edge case
that surfaced in tests: a chain id added to `v` (EIP-155 transactions);
here we're dealing with EIP-712 / personal-sign payloads, so `v` is
plain.
