{
  lib,
  buildNpmPackage,
  nodejs_26,
}:
buildNpmPackage {
  pname = "ui3-overlay";
  version = "0.0.0";

  src = ../ui3;
  nodejs = nodejs_26;

  npmDepsHash =
    let stamp = builtins.readFile ../ui3/.npm-deps-stamp;
    in lib.elemAt (lib.splitString " " (lib.head (lib.splitString "\n" stamp))) 1;

  npmBuildScript = "build:overlay";

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
