{
  lib,
  buildNpmPackage,
  nodejs_26,
}:
# The HUD overlay bundle bevy-explorer serves at /play.
#
# It used to reach the explorer only as ~142 build artifacts force-added to
# bevy-explorer/deploy/web/ui3-overlay, against that tree's own .gitignore --
# tracked purely because a nix flake build sees only tracked files and had no
# other way to obtain it. This package is that other way.
buildNpmPackage {
  pname = "ui3-overlay";
  version = "0.0.0";

  src = ../ui3;
  nodejs = nodejs_26;

  # Read from the stamp rather than pinned here. the deploy tree's scripts/{check,update}-npm-deps-hash.sh
  # only ever rewrite <dir>/flake.nix, so a third hardcoded copy would keep a stale
  # hash after a lockfile bump while the gate reported green -- the exact deploy-time
  # break that gate exists to prevent.
  npmDepsHash =
    let stamp = builtins.readFile ../ui3/.npm-deps-stamp;
    in lib.elemAt (lib.splitString " " (lib.head (lib.splitString "\n" stamp))) 1;

  # `build:overlay` runs typecheck first, which needs the whole ui3 project;
  # vite writes the bundle to dist-overlay.
  npmBuildScript = "build:overlay";

  # The same build-integrity checks the publish path runs. Presence of the entry
  # file proves nothing about the ~119 hashed chunks it imports: a dangling chunk
  # reference already shipped a production 404 once, which is why
  # checkReferencedChunks exists. Packaging the bundle instead of publishing it
  # must not skip them.
  postBuild = ''
    node scripts/publish-overlay.mts --verify-build
  '';

  installPhase = ''
    runHook preInstall
    for f in overlay.js overlay.css; do
      if [ ! -f "dist-overlay/$f" ]; then
        echo "ui3-overlay: dist-overlay/$f missing -- did build:overlay change its output names?" >&2
        exit 1
      fi
    done
    mkdir -p $out
    cp -r dist-overlay/. $out/
    runHook postInstall
  '';

  doCheck = false;

  meta = {
    description = "ui3-overlay -- the DOM HUD bevy-explorer serves alongside the wasm engine";
  };
}
