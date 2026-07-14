{
  lib,
  buildNpmPackage,
  nodejs_26,
  makeWrapper,
}:
buildNpmPackage {
  pname = "sites";
  version = "0.0.0";

  src = ../sites;
  nodejs = nodejs_26;

  # Regenerate whenever sites/package-lock.json changes:
  #   nix run nixpkgs#prefetch-npm-deps -- catalyrst/sites/package-lock.json
  # Keep in lockstep with sites/flake.nix (the dev-shell twin of this build).
  npmDepsHash = "sha256-/zf98PzO6OMH+2c+Cf7IbWGNxDalcSD3dIMIA/uOhvw=";

  nativeBuildInputs = [ makeWrapper ];

  npmBuildScript = "build";

  preBuild = ''
    mkdir -p ../ui3
    cp -r ${../ui3}/src ../ui3/src
    cp -r ${../ui3}/public ../ui3/public
    cp ${../ui3}/vite.validate.js ../ui3/
    chmod -R u+w ../ui3
    ln -sfn "$PWD/node_modules" ../node_modules
    node scripts/sync-ui3-public.mts
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out
    cp -r build $out/build
    cp -r node_modules $out/node_modules
    cp package.json $out/package.json

    mkdir -p $out/packages/data/src/fixtures
    cp -r packages/data/src/fixtures/. $out/packages/data/src/fixtures/
    ( cd packages/features/src && find stories -name '*.md' -exec cp --parents {} "$out/packages/features/src/" \; )

    mkdir -p $out/bin
    makeWrapper ${nodejs_26}/bin/node $out/bin/sites-server \
      --add-flags "$out/node_modules/.bin/react-router-serve $out/build/server/index.js" \
      --chdir "$out"

    runHook postInstall
  '';

  doCheck = false;

  meta = {
    description = "sites -- the SSR web tier (operator /server console + realm surfaces)";
    mainProgram = "sites-server";
  };
}
