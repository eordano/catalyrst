# Auth chain validation (EIP-1271 / EIP-1654) and signed writes

Every deployment to `POST /content/entities` is signed by a chain of authorizations rooted in an
Ethereum-controlled address. Validation is fail-closed everywhere: an unverifiable chain rejects,
never passes through.
Sources: `crates/catalyrst-crypto/src/{auth_chain,verify,eip1654,rpc_validator,validation_cache,recover}.rs`.

## Two verifier surfaces - pick the right one

| Function | Smart-wallet (EIP-1654)? | When to call |
|---|---|---|
| `verify_auth_chain_with_validator` | **REJECTS unconditionally** | sync-only code paths; tests; legacy |
| `verify_auth_chain_async` | validates via the supplied async validator | the real deploy hot path; anything that must accept smart wallets |

The synchronous verifier ignores its `_eip1654_validator` parameter and returns
`Eip1654NotImplemented` for any chain containing an EIP-1654 link - the parameter exists only for
signature compatibility, and the sync path can only ever reject. Callers that must support
smart-wallet auth chains MUST use `verify_auth_chain_async`.

**`ETH_RPC_URL` must be HTTPS - startup-enforced.** It gates EIP-1654 signature validity via
`eth_call isValidSignature`; a MITM'd endpoint can forge returns and authorize deployments under
any contract wallet's address. `bin/live.rs` refuses plaintext `http://` at startup - do not
remove that check; use a trusted operator (`rpc.decentraland.org`, your own node, or equivalent).

Fail-closed defaults:

- `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` defaults to false; only an explicit `=true` bypasses on-chain
  ownership/access checks (only legitimate use: historical-profile sync).
- Third-party roots: if neither the squid DB nor the registry subgraph is configured, third-party
  access rejects (`Ok(false)`) - no root is ever trusted by default
  (`squid_checker.rs::ThirdPartyChecker`).
- Auth-chain link decoding (`write_deployer.rs::validator_chain_to_crypto`) JSON-round-trips and
  rejects unknown link types instead of dropping them.

Validators:

- `rpc_validator` - ABI-encoded `isValidSignature(bytes32,bytes)` via `eth_call`; valid iff the
  contract returns the EIP-1271 magic value `0x1626ba7e`. Dynamic args are 32-byte-aligned
  (structural EIP-1271 encoding, not a catalyrst quirk).
- `validation_cache` - in-process, size-bounded cache of `(contract, hash, signature) -> bool`
  with a short TTL, so burst re-fetches of the same chain don't fan out to the RPC. Failures are
  not cached negatively for long - an RPC blip must not durably reject a valid signature.

ECDSA recovery: `v` may arrive as `27/28` or `0/1`; normalize to `0/1` before
`secp256k1::recover_id`. (EIP-155 chain-id-in-`v` does not apply - these are EIP-712 /
personal-sign payloads.)

Beyond content deploys, the same crate verifies:

- signed-fetch (ADR-44): `x-identity-auth-chain-*` headers over `METHOD:PATH:TIMESTAMP:METADATA`,
  used by notifications, camera-reel, world-storage, quests, and the comms gatekeeper;
- authoritative-storage delegations (world-storage `delegation.rs`): the
  `x-authoritative-scope` header carries base64 JSON `{payload, signature}` - a fixed-format
  plain-text claim (`Ephemeral`/`World`/`SceneId`/`Parcel`/`Expiration`) personal-signed by the
  authoritative server key (`AUTHORITATIVE_SERVER_ADDRESS`; EOA only, EIP-1654 rejected here).
  The claim's ephemeral address must equal the signed-fetch request signer; unknown, missing, or
  duplicate claim lines fail closed. Minted by `catalyrst-deploy-signer --serve-delegations`,
  consumed by scene-state's `~system/SignedFetch`;
- federation writes: EIP-712 `Signed<T>` envelopes (see [federation.md](./federation.md));
  receivers re-run full verification, gossip is never trusted transitively;
- admin console sign-in: a single EIP-191 personal-sign over a SIWE-style message, exchanged for
  a stateless HMAC session cookie (see [operations/admin-console.md](./operations/admin-console.md)).
