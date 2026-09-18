# dcl.social

Standalone Decentraland community chat, friends, voice, events and Worlds, with a React frontend and a Rust API. Foundation remains authoritative for membership, profiles and social permissions; the API stores community chat in SQLite.

## Build and run

From the repository root, using the pinned Rust toolchain and Node.js 26:

```sh
cd social
npm ci --ignore-scripts
npm run build
cd ..
cargo run --locked -p dcl-social-api
```

Open <http://127.0.0.1:5191>. For frontend development, keep the API running and run `npm run dev` in `social`; Vite serves port 5192 and proxies `/api` to port 5191.

| Variable | Default |
| --- | --- |
| `SOCIAL_BIND` | `127.0.0.1:5191` |
| `SOCIAL_DATABASE` | `dcl-social.sqlite` |
| `SOCIAL_ASSETS` | `social/dist`, relative to the API working directory |
| `SOCIAL_UPSTREAM` | `https://social-api.decentraland.org` |
| `RPC_ENDPOINT_ETH` | Unset; needed for smart-contract wallet validation |
| `SOCIAL_TELEMETRY_URL` | `https://interconnected.online/telemetry/api/dcl-social/store/` |
| `SOCIAL_RELEASE` | Release directory name or `dcl-social-<API version>` |

Use a persistent writable database location in production. Production and staging Foundation origins are allowlisted. Loopback fixtures require `SOCIAL_ALLOW_LOCAL_UPSTREAM=1`. Telemetry includes client/server errors with credential patterns and URL queries redacted; set `SOCIAL_TELEMETRY_URL` to your own compatible sink when self-hosting.

## Wallet integration

The browser can connect an injected wallet, or the embedding host can provide an existing signer before loading the app:

```ts
window.dclSocialIdentity = {
  address: existingWalletAddress,
  canSignSilently: true,
  async signRequest(prepared) {
    return existingIdentity.signPayload(prepared.payload)
  }
}
window.dispatchEvent(new Event('dcl:identity-changed'))
```

See `src/api.ts` for the complete interface. Set `canSignSilently` only for an already authorized signer. Hosts should review `prepared.operation`, URL, method and body according to their approval policy. Dispatch the identity-change event on logout or account changes too. Wallet keys remain in the browser; connecting an injected wallet authorizes a 24-hour ephemeral session.

## Validation

From the repository root:

```sh
cargo test --locked -p dcl-social-api
cargo build --locked -p dcl-social-api
cd social
npm ci --ignore-scripts
npm run build
npx playwright install chromium
npm run test:browser
SOCIAL_TEST_PREFIX=/chat npm run test:browser
npm run test:friends
npm run test:discovery
npm run test:activity
```

`CHROMIUM_PATH` selects an installed Chromium instead of Playwright's browser. `SOCIAL_TEST_BINARY` selects an API binary when using a custom Cargo target directory. Tests use local upstream fixtures and disposable wallets; they do not post to production communities.

Community channels, replies, reactions, pins and permissions persist in SQLite. Private friend messages remain in memory in the current tab, without offline delivery. Automatic friend locations cover Genesis City; Worlds destinations are preserved when explicitly shared. Voice tests use fixture media rather than human calls.

See [self-hosting](deploy/README.md) for release packaging, systemd and nginx examples.
