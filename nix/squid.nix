{
  lib,
  buildNpmPackage,
  nodejs_24,
  makeWrapper,
}:
let
  # NOT nodejs_26: node 25 removed buffer.SlowBuffer, which the vendored
  # buffer-equal-constant-time (jwa/jws chain) still touches at import time --
  # squid-eth/polygon crash-loops on `SlowBuffer.prototype` without an LTS
  # that still ships it. Raise the pin only after that dep chain is patched
  # to crypto.timingSafeEqual.
  nodejs = nodejs_24;
  src = ../contracts/squid;
in
buildNpmPackage {
  pname = "marketplace-squid";
  version = "0-unstable-2026-07-24";

  inherit src nodejs;

  npmDepsHash = "sha256-2+2t6KbrpYApd2MpAf+CeuBJX9W+898n3xfKMUlkpBk=";

  nativeBuildInputs = [ makeWrapper ];

  npmBuildScript = "build";

  installPhase = ''
    runHook preInstall

    share=$out/share/squid
    mkdir -p $share

    # compiled output + runtime-resolved source assets
    cp -r lib $share/lib
    cp -r node_modules $share/node_modules
    cp -r assets $share/assets
    cp -r abi $share/abi
    cp package.json $share/package.json
    cp schema.graphql $share/schema.graphql
    cp commands.json $share/commands.json 2>/dev/null || true
    cp squid.yaml $share/squid.yaml 2>/dev/null || true
    # db/migrations are applied by squid-typeorm-migration
    cp -r db $share/db 2>/dev/null || true

    mkdir -p $out/bin
    node=${nodejs}/bin/node

    # ETH L1 processor.
    makeWrapper $node $out/bin/squid-eth \
      --add-flags "--max-old-space-size=4096 $share/lib/eth/main.js" \
      --chdir "$share"

    # Polygon L2 processor.
    makeWrapper $node $out/bin/squid-polygon \
      --add-flags "--max-old-space-size=4096 $share/lib/polygon/main.js" \
      --chdir "$share"

    # TypeORM migration applier (run once before the processors). The package
    # exposes a dedicated `squid-typeorm-migration-apply` bin (it forwards to
    # `squid-typeorm-migration apply`).
    makeWrapper $node $out/bin/squid-migrate \
      --add-flags "$share/node_modules/@subsquid/typeorm-migration/bin/squid-typeorm-migration-apply" \
      --chdir "$share"

    runHook postInstall
  '';

  doCheck = false;

  meta = {
    description = "Decentraland Subsquid marketplace indexer (first-class fork at contracts/squid), populates squid_marketplace";
    homepage = "https://github.com/decentraland/marketplace-squid-core";
    license = lib.licenses.asl20;
    mainProgram = "squid-eth";
  };
}
