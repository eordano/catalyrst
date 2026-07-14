# scripts/drive -- UI drive + screenshot toolkit

Consolidation of the raw-CDP driver patterns (P3 of
`docs/design/ui-iteration-harness.md`) into reusable node tooling. No deps --
node built-ins + a headless chromium the tools launch themselves on an
ephemeral port (never attaches to anyone else's browser; never `:9222`).

## ui-shot

```bash
npm run shot -- /create /marketplace/shop     # shoot + pixel-diff vs baselines
npm run shot:update -- /create                # (re)write baselines
node scripts/drive/shot.mts --base http://127.0.0.1:5189 --burner /account
```

- Diff runs *inside* chromium (canvas compare, >12/channel = changed pixel);
  over-threshold shots get a red-overlay `.diff.png` in `out/`.
- `--burner` seeds a real key-backed session via `window.__DCL_DEV__` first
  (dev server only).
- Every run writes `out/manifest.json` stamped with `{commit, dirty}` -- under
  up-dev the target serves the **uncommitted working tree**, so unstamped
  evidence is unreproducible (see `docs/design/evidence-under-dev-mode.md`).
- Baselines live in `baselines/` and are committed; exit 1 when any diff
  exceeds `--threshold` (default 0.5%).

### Storybook canvases (`story:` targets)

`story:<id>` shoots the bare story canvas (`iframe.html`) from storybook-dev.
KNOWN STATE: canvases need the dev server's DIRECT origin
(default `--sb-base http://localhost:5006`) -- under the `/ui` path prefix the preview
iframe cannot load (vite dev emits root-absolute `/@vite/`+`/@id/` module URLs
that escape the prefix; see the comment in `catalyrst/ui3/.storybook/main.ts`). Story
ids come from `<sb-base>/index.json`.

## smoke (npm run smoke)

Headless degradation sweep (P4): every route in `smoke-routes.mts` x auth
state -> main-document status, console errors/exceptions via CDP EVENTS
(worker+iframe errors included), broken images, brand-font presence, empty
body. Green = exit 0; failures list evidence per route. Report git-stamped at
`out/smoke.json`. First runs caught: the missing origin favicon (404 on every
fresh session, all surfaces incl. /docs) and a wrong route guess (/account ->
/marketplace/account). Keep `allowConsole` entries NARROW and commented -- a
blanket allow defeats the sweep.

Scheduled: `deploy-smoke.timer` (systemd user, hourly at :06 via OnCalendar,
Persistent) -- a non-zero exit leaves the unit failed, which the manager's
supervisor ticks and the hourly `lore-drift-check` catch. (Was OnUnitActiveSec=1h: a failed oneshot never re-arms that, so one red sweep silently
stopped the hourly cadence.)

DEV GOTCHA (proven by the timer's first catch): the vite dev
server does NOT hot-detect files ADDED to `public/` after it starts -- new
public assets 404 until `deploy-dev-sites` restarts. (Related but distinct
from the boot-race fixed in sync-ui3-public: that killed ALL statics when the
dir was missing at boot; this is per-file for later additions.) If you add a
public asset in dev, ask the manager for a dev-sites bounce -- smoke will page
otherwise.

## cdp.mts

`freePort()`, `launchChromium({port, profileDir})`, `Tab.open(port)` with
`cmd/ev/navigate/setViewport/screenshotB64` -- enough to script any drive
(see `/tmp`-style one-offs in the repo history, or `shot.mts` as the example).
Chromium resolution: `DCL_SHOT_CHROMIUM` env -> `chromium` on PATH ->
`nix run nixpkgs#chromium`.

## Related dev-mode tooling

- `window.__DCL_DEV__` (P5): `signInBurner()`, `seedIdentity(addr)`,
  `openSignIn()`, `preview('Button', {children:'Hi'})`, `clearAll()` --
  installed automatically under the vite dev server.
- `/dev/preview[/<Component>]?props=<json>&wrap=ui2&bg=dark` (P2):
  URL-addressable component states, index at `/dev/preview` (~285 ui3
  components). Dev server only; tree-shaken out of prod builds.
