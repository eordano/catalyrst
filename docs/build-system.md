# Build system: flake pins and vendored manifests

`flake.nix` builds three packages: `archipelago-workers` (Node), `pulse`
(.NET), and `catalyrst` (Rust). Each carries pin/patch rationale that
isn't self-evident.

## archipelago-workers

- **uWS swap (`uws-node24`):** archipelago's `package.json` pins
  `uWebSockets.js` v20.43, whose prebuilt native binding has max ABI 120
  (Node 21). We run Node 24 (ABI 137), so the postBuild step swaps the
  hoisted `node_modules/uWebSockets.js` for v20.67 (which ships ABI 137
  bindings).
- **Vendored `archipelago-package-lock.json`:** upstream tracks only
  `yarn.lock`, but `buildNpmPackage`'s npm-deps step needs a
  `package-lock.json` to hash. Generated once from a fresh `npm install`
  against the same `package.json` and checked into `nixos/`.
- **Per-workspace build loop:** the root `build` script is
  `yarn workspaces run build`; we shell out to per-workspace `npm run build`
  instead because `npm` doesn't have a workspaces-run-build equivalent at
  the root.

## pulse (.NET)

- **Source rev pinned to `d7b13d7`:** the last upstream commit that ships
  `flake.nix` + `nix/deps.json`. HEAD removed the Nix packaging; server
  behavior is identical.
- **Vendored `nixos/pulse-deps.json`:** upstream's `deps.json` at that rev
  was stale relative to the actual NuGet graph; regenerated locally with
  `nix-build -A fetch-deps`.
- **HttpService bind-address patch:** upstream hardcodes `string host =
  "+"` (wildcard). We `substituteInPlace` to honor a new
  `HttpServiceOptions.Host`, surfaced via `Env: HttpService__Host =
  127.0.0.1` (or `ASPNETCORE_URLS` for Kestrel). Without this the .NET
  HTTP endpoint would bind to all interfaces. `--replace-fail` fails the
  build loudly on upstream rev bumps that drop the matched lines, instead
  of silently producing an unpatched binary.

## catalyrst (Rust)

- **Helios git hash:** all seven `helios-*` 0.11.1 crates come from one
  git revision, so a single `outputHashes` entry covers them.
- `OPENSSL_NO_VENDOR = "1"` — link the system openssl (from `buildInputs`),
  don't compile the vendored copy.

## Comms identity (`commsVersion` / `commsCommitHash`)

Reported via the catalyst `/about` endpoint and consumed by
`catalyst-monitor`'s "Archipelago" tile. The shape must be:

- `commsVersion`  = `<node-version>+pulse-<pulse-short-rev>`
- `commsCommitHash` = `<archipelago-short-rev>+<pulse-short-rev>`

Don't change the separator or order without updating the monitor.
