# Vendored node_modules

`node_modules.zip` is extracted by `dcl-one-sdk init` so a scaffolded scene builds
and previews with no npm and no network. It contains the scene toolchain the Rust
CLI actually uses: `@dcl/sdk` (with `@dcl/ecs`, `@dcl/react-ecs`, `@dcl/js-runtime`,
`@dcl/ecs-math` and their runtime deps) plus `typescript` for the type check. It is
pure JS — no platform binaries — so one blob serves linux, macOS and Windows.

## Regenerating

1. Empty dir with a `package.json` whose `devDependencies` are copied verbatim from
   `src/templates/init/scene/package.json`.
2. `npm install --ignore-scripts --no-audit --no-fund`
3. Prune the npm-toolchain subtrees the Rust CLI never loads:
   `@dcl/explorer @dcl/inspector @dcl/linker-dapp @dcl/quests-manager
   @dcl/sdk-commands @dcl/quests-client esbuild @esbuild dprint-node ipld-dag-pb
   ipfs-unixfs @segment @opentelemetry @babel archiver archiver-utils
   dcl-catalyst-client @well-known-components node_modules/.bin`
4. Delete symlinks (`find node_modules -type l -delete`) and source maps
   (`find node_modules -name '*.map' -delete`) — symlinks break Windows extraction.
5. Zip deterministically (sorted paths, fixed timestamps, deflate) as
   `node_modules/...` entries at the archive root.
6. Prove it before committing: `dcl-one-sdk init` in an empty dir, then `build`
   (rolldown + type check) with a scene importing `@dcl/sdk/players` and
   `@dcl/sdk/network`, then `start` and probe `/about`.

When bumping the `@dcl/sdk` pin, update the pin in both
`src/templates/init/*/package.json` first — the vendored set must match the
scaffold manifest so a later `npm install` converges instead of fighting it.

Smart items (`@dcl/asset-packs`) and the visual editor are not in the default
path; scenes that want them run `npm install` after adding the dependency.
