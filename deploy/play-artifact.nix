{
  # Pin record for the /play web client (bevy-explorer wasm bundle).
  # The web client ships with the catalyrst deploy as a PINNED ARTIFACT — the
  # abgen precedent (data/bin/abgen-bvwebgpu-<hash> hardcoded in its unit
  # template), not an in-repo build. render-nginx.sh resolves @BEVY_WEB_DIR@
  # from `dir` below whenever BEVY_WEB_DIR is unset/empty in the environment
  # (env/common.env override wins, and on this instance points at the same dir).
  # Bump recipe: deploy/play-artifact.md.
  dir = "/home/dcl/one/bevy-explorer/deploy/web";

  # sha256 of the entry files actually served (the bundle's provenance.json
  # records the build; these hashes let a bump be verified without trusting
  # mtimes). Bumps here are manual engine rotations.
  entries = {
    "ui.js" = "sha256-e88b97de45b7f9118968e4254c677209590dbeb0958bc00872d33608f1dfe024";
    "pkg/webgpu_build_bg.wasm" = "sha256-4e88aa911ff791d3aaa12fcaf826a4ce6eca27b8600c14e76d60c20ed0eb6b51";
  };

  # ui3-overlay/ is the React HUD, published on its own cadence by
  # catalyrst/ui3/scripts/publish-overlay.mjs, which rewrites this block on every
  # publish — auto-bumped, never edited by hand. `bumped` (not `updated`)
  # so the tooling seds that read the top-level field stay unambiguous.
  overlay = {
    "ui3-overlay/overlay.js" = "sha256-8bd08955b0e72423dbdf8f3a5411c187ad526d6ccabf94caaaa9c7099a512d87";
    "ui3-overlay/overlay.css" = "sha256-cdf89e8d0888d94ddf676bb82884c4bcf3ddb40578a2a73afddb6dd4cb624b85";
    bumped = "2026-08-05";
  };

  # Engine source of the deployed wasm: the vendored bevy-explorer tree in the
  # ~/one monorepo at the rev recorded by the bundle's provenance.json
  # (builtBy "bevy-explorer#web", written 2026-07-31 16:29).
  #
  # This pin was committed at 15:33 on 2026-07-31 and the wasm was rebuilt 56
  # minutes later, so it went stale the same day it was written. That is the
  # normal failure mode here, not an anomaly: nginx serves this directory
  # straight out of the working tree, so a rebuild ships the moment it lands and
  # only the pin lags behind. ui.js was untouched by that rebuild and still
  # matches. Re-check with umbrella-dev-health, which diffs both entries.
  source = "bevy-explorer@ee88cc086415b5c2cd34f5742c0e29e81b1d697c-dirty";
  # Upstream vendor point of that tree (bevy-explorer/UPSTREAM).
  upstream = "decentraland/bevy-explorer@df928e4ef3a2d21a7b62333796dddd07835bd70f";

  updated = "2026-07-31";
}
