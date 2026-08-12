# /play pinned artifact — refresh recipe

The bevy-explorer web client ships with the catalyrst deploy as a pinned
artifact, not an in-repo build. `deploy/play-artifact.nix` is the pin record;
`umbrella/scripts/render-nginx.sh` resolves `@BEVY_WEB_DIR@` (locations
`/play/`, `/_play/` in `deploy/nginx/01-catalyst.conf`) from the environment
first (`umbrella/env/common.env` sets `BEVY_WEB_DIR` on this instance) and
falls back to the pin's `dir` when the env leaves it unset/empty.

This mirrors the abgen-bvwebgpu precedent: an experimental artifact pinned
by content identity, tracked in the repo, old versions kept side by side. There
the artifact is a hash-named binary (`umbrella/data/bin/abgen-bvwebgpu-42b0685`)
hardcoded in the unit template
(`deploy/systemd/umbrella-catalyrst-bvwebgpu.service`); a bump drops a new
`abgen-bvwebgpu-<shorthash>` next to the old one, edits the template's
`ExecStart`, re-renders units, and requests a restart. Here the artifact is the
bundle dir; the pin records its entry-file hashes and engine source rev, and
previous versions live beside it as `*.prev-YYYYMMDD` rotations.

Note: colmena's `play.dcl.one` vhost serves a separate copy from
`/persist/web/play.dcl.one` — that deploy is not governed by this pin.

## 1. Build a new bundle

Two lanes (the live bundle was last produced by lane B — see the bundle's
`provenance.json`):

A. Hermetic flake bundle — `umbrella/scripts/build-bevy-web.sh` runs
`nix build .#web` inside `bevy-explorer/` (in-repo so the workspace resolves
`../third-party` without copying the untracked tree) and maintains the stable
gcroot symlink `umbrella/data/bevy-web`. Point the pin's `dir` (and/or
`BEVY_WEB_DIR` in `common.env`) at that symlink once; later bumps are just a
re-run — the symlink swaps atomically.

B. In-tree docroot rotation (current live layout,
`dir = bevy-explorer/deploy/web`): build the engine core only
(`nix build .#web-core` in `bevy-explorer/`), then rotate into the live
docroot, keeping the previous version:

```bash
cd /home/dcl/one/bevy-explorer
nix build .#web-core --out-link /tmp/web-core
d=deploy/web; stamp=$(date +%Y%m%d)
mv $d/pkg $d/pkg.prev-$stamp
cp -r --no-preserve=mode,ownership /tmp/web-core/pkg $d/pkg
# assets_bundle.bin(.br) and provenance.json rotate the same way when changed
```

Write `deploy/web/provenance.json` with the ~/one rev the build came from
(`git -C /home/dcl/one rev-parse HEAD`, `-dirty` suffix if the tree was), the
web-core store path, and a `builtBy` note — same shape as the current file.

## 2. Verify

```bash
# entry files present and hashed — MUST match `entries` in play-artifact.nix
# (umbrella-dev-health's "/play pin" section runs this comparison automatically,
# on-disk AND against the bytes nginx serves; a mismatch there means the bundle
# was swapped without a pin bump)
cd /home/dcl/one/bevy-explorer/deploy/web
sha256sum ui.js pkg/webgpu_build_bg.wasm
# render still valid (temp state dir — never overwrite live renders untested)
UMBRELLA_STATE=$(mktemp -d) bash umbrella/scripts/render-nginx.sh
# live surface after reload
curl -sI https://catalyst.dcl.one/play/ | head -3
curl -sI -H 'Accept-Encoding: br' https://catalyst.dcl.one/play/pkg/webgpu_build_bg.wasm | head -3
```

For a real smoke test load `https://catalyst.dcl.one/play/` on shade (WebGPU,
CDP capture — see the shade-webgpu-headless-capture notes); the sandbox has
no GPU browser.

## 3. Bump the pin

Edit `deploy/play-artifact.nix`:

- `dir` — only if the docroot moved (e.g. switching to the lane-A symlink)
- `entries` — new `sha256sum` of `ui.js` and `pkg/webgpu_build_bg.wasm`
- `source` — `bevy-explorer@<rev>` from the new `provenance.json`
- `upstream` — only when `bevy-explorer/UPSTREAM` is re-vendored
- `updated` — today

Then re-render (`umbrella/scripts/render-nginx.sh`) — a no-op for nginx unless
`dir` changed — and reload nginx. From an agent sandbox, request the reload via
`~/lore/data/restart-requests/` instead of `systemctl --user`. Commit the pin
bump and any rotated `provenance.json` atomically with explicit pathspecs (the
background auto-committer will otherwise sweep them).

## The HUD (`overlay` block)

`ui3-overlay/` is covered separately: `catalyrst/ui3/scripts/publish-overlay.mts` rewrites
the pin's `overlay` block on every publish (one-shot and watch syncs alike), so
its hashes track the deployed HUD with no manual step. A `WARN served HUD does
not match the pin` from `umbrella-dev-health` therefore means a swap or a
partial publish — never "someone forgot the bump".
