# `lib/auth/thirdweb` - vendored in-app-wallet client

Email/social Sign in (no browser extension) + transparent in-app signing, built on thirdweb's enclave wallet - without the `thirdweb` npm package.

## Why there is no dependency to audit

thirdweb's enclave signs server-side: every private key lives in their AWS Nitro enclave and the browser never holds key material or runs MPC crypto. The entire client surface is plain REST against the public, documented API (`api.thirdweb.com/reference`). This directory is the client, using only `fetch` (zero new deps) - the same shape as the rest of `lib/auth`, which replaced `decentraland-connect` with a small viem layer. Freezing a copy of thirdweb's SDK would mean owning security patches for a wallet.

## Audited surface (`api.ts`)

| Function | Endpoint | Auth |
|---|---|---|
| `initiateEmailLogin` | `POST /v1/auth/initiate` | `x-client-id` |
| `completeEmailLogin` | `POST /v1/auth/complete` -> `{token, walletAddress}` | `x-client-id` |
| `socialLoginUrl` | `GET /v1/auth/social` (302 OAuth) | `clientId` in query |
| `signMessageEnclave` | `POST /v1/wallets/sign-message` -> `{result:{signature}}` | Bearer + `x-client-id` |
| `signTypedDataEnclave` | `POST /v1/wallets/sign-typed-data` (EIP-712) | Bearer + `x-client-id` |
| `getWalletForToken` | `GET /v1/wallets/me` | Bearer + `x-client-id` |

`ClientAuth` = the public `x-client-id`; `BearerAuth` = the per-user JWT.

## How it plugs into ADR-44

The rest of `lib/auth` is signer-agnostic. Login needs exactly one `personalSign(ephemeralMessage)`; `signer.ts#makeInAppSigner` provides it as an enclave round-trip, and `identity.ts#createIdentityWith` builds the same `[SIGNER, ECDSA_EPHEMERAL]` chain the injected-wallet path builds. Everything downstream (ephemeral key, signed-fetch) is unchanged - catalyrst can't tell an enclave login from a MetaMask login. The gasless marketplace meta-tx path uses `signTypedDataEnclave` - "sign transfers from inside the client" (strategy 11.02) with no wallet popup.

## Config

```
THIRDWEB_CLIENT_ID=<your thirdweb project client id>
THIRDWEB_SECRET_KEY=<your thirdweb project secret key>
```

Server code reads `process.env.THIRDWEB_CLIENT_ID`; the browser reads `window.__DCL_PUBLIC__.thirdwebClientId` (injected by `root.tsx` from the same var). Unset -> the in-app Sign in affordance disables itself and explains why (never throws).
`THIRDWEB_SECRET_KEY` is read only by `config.ts#thirdwebSecretKey()`, inside the same-origin sign proxy route `app/routes/internal.thirdweb-sign.tsx` (the target of `signer.ts`'s `SIGN_PROXY`). It never reaches the browser. If unset, that route 503s ("Sign-in is not fully configured on this server") and email/social sign-in (enclave signing) is unusable - live email OTP + enclave signing require both vars set.

## Security notes

- The `clientId` is public and safe to ship. The secret key must never reach the client - it is read only inside the server-side sign proxy (`internal.thirdweb-sign.tsx`), never by browser-reachable code.
- The per-user JWT authorizes enclave signing and is persisted in `localStorage` (`session.ts`) alongside the ADR-44 identity - consistent with the existing threat model, which already stores the ephemeral private key there. Cleared on disconnect.
