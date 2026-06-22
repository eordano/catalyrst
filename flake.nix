{
  description = "catalyrst — Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.archipelago = { url = "github:decentraland/archipelago-workers/537def15e2609cf0ecc8ba5bd7ad400702e455c8"; flake = false; };
  inputs.uws-node24 = { url = "github:uNetworking/uWebSockets.js/v20.67.0"; flake = false; };

  outputs = { self, nixpkgs, flake-utils, archipelago, uws-node24 }:
    let
      # NixOS module is system-independent — expose it at the top level.
      nixosModules.catalyrst = import ./nixos/configuration.nix;
    in
    (flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        nodejs = pkgs.nodejs_24;
      in {
        packages = rec {
          archipelago-workers = pkgs.buildNpmPackage {
            pname = "archipelago-workers";
            version = "0.1.0";
            src = archipelago;
            npmDepsHash = "sha256-zZLuGHkMxpqOcJG4nGRZqLexTCL0O2RojRop8/jchqM=";
            inherit nodejs;
            dontNpmBuild = true;
            nativeBuildInputs = [ pkgs.makeWrapper ];
            postPatch = ''
              cp ${./nixos/archipelago-package-lock.json} package-lock.json
            '';
            preBuild = ''
              chmod -R u+w node_modules/uWebSockets.js
              rm -rf node_modules/uWebSockets.js/*
              cp -r ${uws-node24}/* node_modules/uWebSockets.js/
              chmod -R u+w node_modules/uWebSockets.js
            '';
            buildPhase = ''
              runHook preBuild
              for w in core ws-connector stats; do
                (cd "$w" && ${nodejs}/bin/node ${nodejs}/lib/node_modules/npm/bin/npm-cli.js run build)
              done
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              mkdir -p "$out/lib/archipelago" "$out/bin"
              cp -r core ws-connector stats node_modules "$out/lib/archipelago/"
              for w in core ws-connector stats; do
                makeWrapper ${nodejs}/bin/node "$out/bin/archipelago-$w" \
                  --add-flags "$out/lib/archipelago/$w/dist/index.js"
              done
              runHook postInstall
            '';
          };

          # Pure-rust Pulse: our catalyrst-pulse crate (rusty_enet, prost) instead
          # of the upstream .NET DCLPulse — keeps the stack single-toolchain.
          pulse = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-pulse";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = [ "-p" "catalyrst-pulse" "--bin" "catalyrst-pulse" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
            meta.mainProgram = "catalyrst-pulse";
          };

          catalyrst = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = [ "-p" "catalyrst-server" "--bin" "catalyrst-live" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
            # Ship the content-DB schema so a deploy can create it on a fresh
            # sync replica (catalyrst-server has no in-binary schema bootstrap;
            # see crates/catalyrst-server/migrations/0001_content_schema.sql).
            # Applied by a NixOS one-shot, not sqlx::migrate! (the content DB's
            # _sqlx_migrations table is owned by catalyrst-media).
            postInstall = ''
              mkdir -p "$out/share/catalyrst-server"
              cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
            '';
          };

          # Marketplace REST API in front of squid_marketplace (port of
          # decentraland/marketplace-server). Loopback Postgres only —
          # sqlx is built without a TLS feature so no openssl needed.
          catalyrst-market = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-market";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoBuildFlags = [ "-p" "catalyrst-market" "--bin" "catalyrst-market" ];
            doCheck = false;
          };

          catalyrst-places = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-places";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-places" "--bin" "catalyrst-places" ];
            doCheck = false;
          };

          catalyrst-events = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-events";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-events" "--bin" "catalyrst-events" ];
            doCheck = false;
          };

          catalyrst-communities = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-communities";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-communities" "--bin" "catalyrst-communities" ];
            doCheck = false;
          };

          catalyrst-explorer-api = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-explorer-api";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-explorer-api" "--bin" "catalyrst-explorer-api" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-comms = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-comms";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-comms" "--bin" "catalyrst-comms" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-archipelago = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-archipelago";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-archipelago" "--bin" "catalyrst-archipelago" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          # abgen: the offline asset-bundle CLI tools from catalyrst-abgen for bulk
          # corpus generation — abgen / abgen-corpus / abgen-verify. The serving path
          # (live JIT convert) is in catalyrst-ab-cdn, so the standalone abgen-serve
          # binary is deliberately not built here (Cargo.toml autobins=false, no
          # [[bin]]). cmake builds vendored third_party (draco, crunch); turbojpeg is
          # dlopen'd at runtime via TURBOJPEG_LIB.
          abgen = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-abgen";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-abgen" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf pkgs.cmake pkgs.makeWrapper ];
            buildInputs = [ pkgs.openssl pkgs.libjpeg_turbo ];
            env.OPENSSL_NO_VENDOR = "1";
            postInstall = ''
              for b in abgen abgen-serve abgen-corpus abgen-verify abgen-world; do
                if [ -e "$out/bin/$b" ]; then
                  wrapProgram "$out/bin/$b" \
                    --set-default TURBOJPEG_LIB "${pkgs.libjpeg_turbo.out}/lib/libturbojpeg.so"
                fi
              done
            '';
          };

          # Server-side SDK7 scene-state host (port of scene-state-server). This
          # crate embeds V8 (the `v8`/rusty_v8 crate). rusty_v8's build.rs
          # normally DOWNLOADS a prebuilt librusty_v8 archive, which is
          # impossible in the offline Nix sandbox. We instead fetch that exact
          # archive as a fixed-output derivation (see ./crates/catalyrst-scene-
          # state/nix/librusty_v8.nix) and point the crate at it via
          # RUSTY_V8_ARCHIVE — the rusty_v8 build then links the prebuilt static
          # lib without any network access or a from-source V8 build.
          librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };
          catalyrst-scene-state = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-scene-state";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-scene-state" "--bin" "catalyrst-scene-state" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              # The single env var that makes V8-under-Nix work offline.
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
          };

          # Single-derivation bundle of every binary a multi-service deploy
          # runs directly from the flake (replaces a locally cargo-built
          # bin/catalyrst tree). One cargo invocation compiles the shared
          # dependency graph once and emits all bins. Build inputs are the
          # UNION of the per-service packages above: openssl everywhere,
          # protobuf for the dcl-rpc / prost codegen (social-rpc), and the
          # prebuilt V8 archive for the scene-state crate.
          catalyrst-all = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-all";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [
              "-p" "catalyrst-server"         "--bin" "catalyrst-live"
              "-p" "catalyrst-explore"        "--bin" "catalyrst-explore"
              "-p" "catalyrst-create"         "--bin" "catalyrst-create"
              "-p" "catalyrst-data"           "--bin" "catalyrst-data"
              "-p" "catalyrst-social"         "--bin" "catalyrst-social"
              "-p" "catalyrst-social-rpc"     "--bin" "catalyrst-social-rpc"
              "-p" "catalyrst-explorer-api"   "--bin" "catalyrst-explorer-api"
              "-p" "catalyrst-ab-cdn"         "--bin" "catalyrst-ab-cdn"
              "-p" "catalyrst-profile-images" "--bin" "catalyrst-profile-images"
              "-p" "catalyrst-scene-state"    "--bin" "catalyrst-scene-state"
              "-p" "catalyrst-signatures"     "--bin" "catalyrst-signatures"
              "-p" "catalyrst-telemetry"      "--bin" "catalyrst-telemetry"
              "-p" "catalyrst-world-storage"  "--bin" "catalyrst-world-storage"
            ];
            doCheck = false;
            # cmake builds the vendored C/C++ in catalyrst-abgen/third_party
            # (draco, crunch) which catalyrst-ab-cdn now links via the abgen lib.
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf pkgs.cmake pkgs.makeWrapper ];
            buildInputs = [ pkgs.openssl pkgs.libjpeg_turbo ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
            # Ship the content-DB schema like the catalyrst package does, so a
            # fresh sync replica can bootstrap content_rust.
            postInstall = ''
              mkdir -p "$out/share/catalyrst-server"
              cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
              # catalyrst-ab-cdn folds abgen live-conversion in-process; bake
              # TURBOJPEG_LIB so it dlopen's the 64-bit libturbojpeg with no FHS env.
              # ABGEN_ROOT / ABGEN_SHADER_BUNDLE are host paths set in the unit.
              wrapProgram "$out/bin/catalyrst-ab-cdn" \
                --set-default TURBOJPEG_LIB "${pkgs.libjpeg_turbo.out}/lib/libturbojpeg.so"
            '';
          };

          default = catalyrst;
        };
      }
    )) // {
      # Reusable NixOS module. Operators import it from their own host config
      # and set `services.catalyrst.*` options. See nixos/module-example.nix
      # for a minimal consumer.
      nixosModules = nixosModules // { default = nixosModules.catalyrst; };
    };
}
