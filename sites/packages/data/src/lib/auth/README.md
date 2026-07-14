# Auth / signed-fetch module
Wallet + auth layer for the SSR write/auth-gated surfaces (account settings, builder publish, governance vote, world-storage signed-fetch, credits). Produces a Decentraland ADR-44 auth-chain (signed-fetch) identity that the catalyrst backends verify byte-for-byte (see `catalyrst/crates/catalyrst-worlds/src/auth_chain.rs` and `catalyrst-crypto`). `decentraland-dapps` / Redux are intentionally not used.
## ADR-44 in 3 sentences
1. On connect, generate a throwaway ephemeral secp256k1 keypair and have the user's wallet `personal_sign` a `"Decentraland Login\nEphemeral address: ...\nExpiration: ..."` message - the `ECDSA_EPHEMERAL` link. The persisted identity is `[SIGNER, ECDSA_EPHEMERAL]` plus the ephemeral private key.
2. For each request, append an `ECDSA_SIGNED_ENTITY` link signed locally by the ephemeral key (no wallet prompt) over the payload `method:path:timestamp:metadata` lowercased - catalyrst lowercases the whole string before verifying.
3. The chain + `x-identity-timestamp` + `x-identity-metadata` go out as headers; catalyrst recovers the wallet from the ephemeral link and the ephemeral address from the request link, and checks the timestamp is within 5 minutes.

> The signed path is the public pathname without query string (catalyrst strips the query and, behind nginx, reconstructs from `x-original-path`). Sign the same URL you fetch, including the public service prefix (e.g. `/world-storage/...`) -- that is the default and it holds wherever nginx forwards `x-original-path`.
>
> The exception is a vhost that strips its prefix WITHOUT setting `x-original-path`: the backend then verifies the unprefixed route, so signing the public URL fails with 401. `/comms/*` is such a vhost. Pass `signPath` to `signedFetch`/`postJSON`/`signedGetJSON` to sign the route the backend actually sees while still fetching the public URL -- see `catalyst/admin/user-bans.ts`. When in doubt, compare the 401 against the backend's expected path before assuming the identity is wrong.
## Files
| File | Purpose |
|---|---|
| `types.ts` | `AuthIdentity`, `AuthLink`, metadata types |
| `wallet.ts` | EIP-1193 `window.ethereum` connect / `personal_sign` |
| `identity.ts` | ephemeral key gen + `createIdentity()` (wallet login) + `createIdentityFromPrivateKey()` (headless/in-process EIP-191) |
| `dev-identity.ts` | DEV-ONLY `signInWithPrivateKey()` (burner login, persists via session) + `isDevHost()` gate |
| `signer.ts` | `signRequest()` / `signedFetch()` - per-request signing |
| `session.ts` | localStorage persistence + subscribable singleton store |
| `context.tsx` | `useAuth()` hook (+ optional `AuthProvider`) |
| `index.ts` | public re-exports |

Only external dependency: `viem` (ephemeral key generation + EIP-191 message signing - `viem/accounts`). Wallet connection is raw `window.ethereum` (no client abstraction).
## Consuming from a write surface
No provider wiring required - `useAuth()` binds to a singleton store via `useSyncExternalStore`, so it works in any client component. If `!auth.isConnected`, render `ConnectButton` (`../components/auth/ConnectButton`); `auth.fetch` attaches the signed-fetch headers automatically:

```tsx
import { useAuth } from "../lib/auth/context";
const auth = useAuth();
const res = await auth.fetch("/world-storage/values", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ key, value }),
});
```

Headers only (e.g. to forward through your own fetch wrapper):

```ts
const { headers } = await auth.sign("POST", "/world-storage/values", {});
// { "x-identity-auth-chain-0": "...", ..., "x-identity-timestamp": "...", "x-identity-metadata": "{}" }
```

Outside React, with an explicit identity:

```ts
import { signedFetch } from "../lib/auth/signer";
import { getIdentity } from "../lib/auth/session";
const id = getIdentity();
if (id) await signedFetch(id, "/world-storage/values", { method: "POST", body });
```
## `useAuth()` surface
| Field | Type | Notes |
|---|---|---|
| `identity` | `AuthIdentity \| null` | null when anonymous/expired |
| `address` | `string \| null` | lowercased wallet address |
| `status` | `"anonymous" \| "connecting" \| "connected" \| "expired"` | |
| `isConnected` | `boolean` | |
| `walletAvailable` | `boolean` | is an injected wallet present |
| `error` | `string \| null` | last connect error |
| `connect(opts?)` | `() => Promise<AuthIdentity \| null>` | prompts wallet |
| `devHost` | `boolean` | true only on a local/dev origin |
| `connectWithKey(pk?, opts?)` | `() => Promise<AuthIdentity \| null>` | dev-only burner login; refuses off a dev host |
| `disconnect()` | `() => void` | local sign-out |
| `sign(method, url, metadata?)` | -> `SignedRequest` | headers only |
| `fetch(url, init?)` | `Promise<Response>` | signed fetch |
## Notes / gotchas
- Client-only: the ephemeral private key must live in the browser, so signed-fetch happens client-side. SSR loaders can't sign for the user.
- Session lifetime: ephemeral keys expire (default 7 days); an expired identity reports `status: "expired"` and `isConnected: false` - call `connect()` again. catalyrst independently enforces the expiration and per-request timestamp window.
- Forbidden signer: do not set `metadata.signer = "decentraland-kernel-scene"` - catalyrst rejects it (reserved for in-world scenes).
- Headless / burner sign-in (dev only): `createIdentityFromPrivateKey(pk, opts)` builds the same `[SIGNER, ECDSA_EPHEMERAL]` identity as the wallet path but signs the ephemeral message in-process with viem (no `window.ethereum`); pure + SSR-safe. `signInWithPrivateKey(pk?, opts?)` (dev-identity.ts) mints-or-takes a key, builds the identity, and persists it via `setIdentity` - it refuses to run off a dev host unless given `{ allowNonDev: true }` (the explicit escape hatch for trusted headless Node tooling, e.g. a server action behind a `DCL_DEV_PRIVATE_KEY` flag). Never expose this as a production sign-in path. Verified live: a burner identity's per-request `ECDSA_SIGNED_ENTITY` passes catalyst signature verification (`GET /world-storage/usage/world` reached the handler).
- Demo: `/connect` (`app/routes/connect.tsx`) exercises the whole flow - connect a wallet, inspect the identity auth chain, sign a test `GET /world-storage/usage/world` payload, view the outgoing headers, optionally fire it live.
