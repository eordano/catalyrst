# Auth chain validation (EIP-1271 / EIP-1654) and signed writes

Every deployment to `POST /content/entities` is signed by a chain of authorizations rooted in an Ethereum-controlled address. Validation is fail-closed everywhere. Sources: `crates/catalyrst-crypto/src/{auth_chain,verify,eip1654,rpc_validator,validation_cache,recover}.rs`.

## Two verifier surfaces

| Function | Smart-wallet (EIP-1654)? | When to call |
|---|---|---|
| `verify_auth_chain_with_validator` | **REJECTS unconditionally** | sync-only code paths; tests |
| `verify_auth_chain_async` | validates via the async validator | the deploy hot path; anything accepting smart wallets |

The synchronous verifier ignores its `_eip1654_validator` parameter and returns `Eip1654NotImplemented` for any chain with an EIP-1654 link (the parameter exists for signature compatibility); smart-wallet support MUST go through `verify_auth_chain_async`.

**`ETH_RPC_URL` must be HTTPS - startup-enforced.** It gates EIP-1654 validity via `eth_call isValidSignature`; a MITM'd endpoint can forge returns and authorize deploys under any contract wallet. `bin/live.rs` refuses plaintext `http://` - do not remove; use a trusted operator (`rpc.decentraland.org` or your own node).

Fail-closed defaults:

- `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` defaults false; only explicit `=true` bypasses on-chain ownership/access checks (only legitimate use: historical-profile sync).
- Third-party roots: neither squid DB nor registry subgraph configured = third-party access rejects (`Ok(false)`; `squid_checker.rs::ThirdPartyChecker`).
- Auth-chain link decoding (`write_deployer.rs::validator_chain_to_crypto`) JSON-round-trips and rejects unknown link types instead of dropping them.

Validators:

- `rpc_validator` - ABI-encoded `isValidSignature(bytes32,bytes)` via `eth_call`; valid iff the contract returns the EIP-1271 magic `0x1626ba7e`. Dynamic args 32-byte-aligned (structural EIP-1271 encoding).
- `validation_cache` - in-process size-bounded cache of `(contract, hash, signature) -> bool`, short TTL, avoiding RPC fan-out on repeat lookups. Failures aren't cached negatively for long: an RPC blip must not durably reject a valid signature.

ECDSA recovery: `v` arrives as `27/28` or `0/1`; normalize to `0/1` before `secp256k1::recover_id`. (EIP-155 chain-id-in-`v` doesn't apply to these EIP-712/personal-sign payloads.)

The same crate also verifies:

- signed-fetch (ADR-44): `x-identity-auth-chain-*` headers over `METHOD:PATH:TIMESTAMP:METADATA` - notifications, camera-reel, world-storage, quests, comms gatekeeper;
- authoritative-storage delegations (world-storage `delegation.rs`): `x-authoritative-scope` carries base64 JSON `{payload, signature}` - a fixed-format plain-text claim (`Ephemeral`/`World`/`SceneId`/`Parcel`/`Expiration`) personal-signed by the authoritative server key (`AUTHORITATIVE_SERVER_ADDRESS`; EOA only, EIP-1654 rejected here). The claim's ephemeral address must equal the signed-fetch signer; unknown, missing, or duplicate claim lines fail closed. Minted by `catalyrst-deploy-signer --serve-delegations`, consumed by scene-state's `~system/SignedFetch`;
- federation writes: EIP-712 `Signed<T>` envelopes ([federation.md](./federation.md)); receivers re-run verification, gossip never trusted transitively;
- admin console sign-in: one EIP-191 personal-sign over a SIWE-style message, exchanged for a stateless HMAC session cookie ([operations.md](./operations.md)).
