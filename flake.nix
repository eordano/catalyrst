{
  description = "catalyrst — Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.archipelago = { url = "github:decentraland/archipelago-workers/537def15e2609cf0ecc8ba5bd7ad400702e455c8"; flake = false; };
  inputs.uws-node24 = { url = "github:uNetworking/uWebSockets.js/v20.67.0"; flake = false; };
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };

  outputs = { self, nixpkgs, archipelago, uws-node24, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems
        (system: f (import nixpkgs { inherit system; }));

      nixosModules.catalyrst = import ./nixos/configuration.nix;

      turbojpegIsoFor = pkgs:
        let
          static = pkgs.libjpeg_turbo.overrideAttrs (old: {
            cmakeFlags = (old.cmakeFlags or [ ]) ++ [ "-DENABLE_STATIC=1" ];
            dontDisableStatic = true;
          });
        in
        pkgs.runCommand "turbojpeg-iso" { nativeBuildInputs = [ pkgs.binutils ]; } ''
          mkdir -p $out/lib
          ld -r --whole-archive ${static.out}/lib/libturbojpeg.a --no-whole-archive -o tj-combined.o
          objcopy -w --keep-global-symbol 'tj*' tj-combined.o tj-iso.o
          ar rcs $out/lib/libturbojpeg_iso.a tj-iso.o
        '';
    in
    {
      packages = forAllSystems (pkgs:
        let
          nodejs = pkgs.nodejs_24;
          turbojpegIso = turbojpegIsoFor pkgs;
        in
        rec {
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
            postInstall = ''
              mkdir -p "$out/share/catalyrst-server"
              cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
            '';
          };

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

          catalyrst-map = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-map";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-map" "--bin" "catalyrst-map" ];
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

          catalyrst-camera-reel = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-camera-reel";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-camera-reel" "--bin" "catalyrst-camera-reel" ];
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

          catalyrst-governance = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-governance";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-governance" "--bin" "catalyrst-governance" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-presence = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-presence";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-presence" "--bin" "catalyrst-presence" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-price = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-price";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-price" "--bin" "catalyrst-price" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-notifications = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-notifications";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-notifications" "--bin" "catalyrst-notifications" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-badges = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-badges";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-badges" "--bin" "catalyrst-badges" ];
            doCheck = false;
          };

          catalyrst-economy = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-economy";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-economy" "--bin" "catalyrst-economy" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-media = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-media";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-media" "--bin" "catalyrst-media" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-credits = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-credits";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-credits" "--bin" "catalyrst-credits" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-worlds = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-worlds";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-worlds" "--bin" "catalyrst-worlds" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-builder = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-builder";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-builder" "--bin" "catalyrst-builder" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-registry = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-registry";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-registry" "--bin" "catalyrst-registry" ];
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

          abgen = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-abgen";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [ "-p" "catalyrst-abgen" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf pkgs.cmake ];
            env = {
              ABGEN_TURBOJPEG_STATIC_DIR = "${turbojpegIso}/lib";
            };
          };

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
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
          };

          catalyrst-all = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-all";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            cargoBuildFlags = [
              "-p"
              "catalyrst-server"
              "--bin"
              "catalyrst-live"
              "-p"
              "catalyrst-explore"
              "--bin"
              "catalyrst-explore"
              "-p"
              "catalyrst-create"
              "--bin"
              "catalyrst-create"
              "-p"
              "catalyrst-data"
              "--bin"
              "catalyrst-data"
              "-p"
              "catalyrst-social"
              "--bin"
              "catalyrst-social"
              "-p"
              "catalyrst-social-rpc"
              "--bin"
              "catalyrst-social-rpc"
              "-p"
              "catalyrst-explorer-api"
              "--bin"
              "catalyrst-explorer-api"
              "-p"
              "catalyrst-abgen"
              "--bin"
              "catalyrst-abgen"
              "-p"
              "catalyrst-profile-images"
              "--bin"
              "catalyrst-profile-images"
              "-p"
              "catalyrst-scene-state"
              "--bin"
              "catalyrst-scene-state"
              "-p"
              "catalyrst-signatures"
              "--bin"
              "catalyrst-signatures"
              "-p"
              "catalyrst-telemetry"
              "--bin"
              "catalyrst-telemetry"
              "-p"
              "catalyrst-world-storage"
              "--bin"
              "catalyrst-world-storage"
              "--features"
              "folded-registry"
            ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf pkgs.cmake pkgs.makeWrapper ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
              ABGEN_TURBOJPEG_STATIC_DIR = "${turbojpegIso}/lib";
              ABGEN_GIT_COMMIT = "6f6e4ea8aa26";
            };
            postInstall = ''
              mkdir -p "$out/share/catalyrst-server"
              cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
              # Bundle the sha-pinned template + shader beside the binary and
              # default ABGEN_ROOT there for self-contained deploys (an explicit
              # ABGEN_ROOT still overrides). turbojpeg is statically baked in.
              mkdir -p "$out/share/abgen"
              [ -d crates/catalyrst-abgen/template ] && cp -r crates/catalyrst-abgen/template "$out/share/abgen/template"
              [ -d crates/catalyrst-abgen/shader ]   && cp -r crates/catalyrst-abgen/shader   "$out/share/abgen/shader"
              wrapProgram "$out/bin/catalyrst-abgen" \
                --set-default ABGEN_ROOT "$out/share/abgen" \
                --set-default ABGEN_SHADER_BUNDLE "$out/share/abgen/shader/scene_ignore_windows"
            '';
          };

          default = catalyrst;
        }

        // pkgs.lib.optionalAttrs (builtins.pathExists ./crates/catalyrst-abgen/export-overlay) (
          let

            abgen-standalone-src = pkgs.runCommand "abgen-standalone-src" { } ''
              ${pkgs.bash}/bin/bash ${./scripts/abgen-standalone-assemble.sh} \
                ${./crates/catalyrst-abgen} "$out"
            '';
            pyEnv = pkgs.python3.withPackages (ps: with ps; [ numpy pillow ]);
            libExt = pkgs.stdenv.hostPlatform.extensions.sharedLibrary;
          in
          {
            inherit abgen-standalone-src;

            abgen-compare = pkgs.rustPlatform.buildRustPackage {
              pname = "abgen-compare";
              version = "0.1.0";
              src = abgen-standalone-src;
              cargoLock = { lockFile = ./crates/catalyrst-abgen/export-overlay/Cargo.lock; };
              cargoBuildFlags = [ "--bins" "--examples" ];
              doCheck = false;
              nativeBuildInputs = [ pkgs.cmake pkgs.pkg-config pkgs.git pkgs.makeWrapper ];
              postInstall = ''
                lib=$out/lib/abgen
                mkdir -p $lib/result/bin $lib/crate
                exdir=$(find target -type d -path '*/release/examples' | head -1)
                for t in objdump texdump matdump texcmp texpng; do
                  if [ -f "$exdir/$t" ]; then
                    install -m755 "$exdir/$t" "$lib/result/bin/$t"
                  else
                    echo "missing example tool: $t" >&2; exit 1
                  fi
                done
                ln -s $out/bin/abgen $lib/result/bin/abgen
                cp -r pipeline site template $lib/
                cp -r crate/shader $lib/crate/
                find $lib -type d -name __pycache__ -prune -exec rm -rf {} +
                makeWrapper ${pyEnv}/bin/python3 $out/bin/abgen-compare \
                  --add-flags "$lib/pipeline/abgen-compare" \
                  --set-default TURBOJPEG_LIB ${pkgs.libjpeg_turbo.out}/lib/libturbojpeg${libExt}
              '';
            };
          }
        ));

      # Stateless, sandboxed tests. `nix flake check` (or
      # `nix build .#checks.<system>.catalyrst-server-tests`) builds the
      # catalyrst derivation with its check phase enabled — no devShell, no
      # mutable cargo target dir. Covers the catalyrst-server input-validation
      # unit tests (nul_guard middleware, DatabaseError->AppError mapping,
      # active_entities validator).
      checks = forAllSystems (pkgs: {
        catalyrst-server-tests =
          self.packages.${pkgs.stdenv.hostPlatform.system}.catalyrst.overrideAttrs (old: {
          pname = "catalyrst-server-tests";
          doCheck = true;
          cargoTestFlags = (old.cargoTestFlags or [ ]) ++ [ "-p" "catalyrst-server" ];
        });
      });

      devShells = forAllSystems (pkgs:
        let
          librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };
          turbojpegIso = turbojpegIsoFor pkgs;
          rust197 = (pkgs.extend (import rust-overlay)).rust-bin.stable."1.97.0".default;
        in
        {
          default = pkgs.mkShell {

            hardeningDisable = [ "fortify" ];
            nativeBuildInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.rust-analyzer
              pkgs.pkg-config
              pkgs.protobuf
              pkgs.cmake
              pkgs.gnumake
            ];
            buildInputs = [ pkgs.openssl pkgs.libjpeg_turbo ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
              ABGEN_TURBOJPEG_STATIC_DIR = "${turbojpegIso}/lib";
              TURBOJPEG_LIB = "${pkgs.libjpeg_turbo.out}/lib/libturbojpeg.so";
            };
          };

          ci = pkgs.mkShell {
            hardeningDisable = [ "fortify" ];
            nativeBuildInputs = [
              rust197
              pkgs.pkg-config
              pkgs.protobuf
              pkgs.cmake
              pkgs.gnumake
            ];
            buildInputs = [ pkgs.openssl pkgs.libjpeg_turbo ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
              ABGEN_TURBOJPEG_STATIC_DIR = "${turbojpegIso}/lib";
              TURBOJPEG_LIB = "${pkgs.libjpeg_turbo.out}/lib/libturbojpeg.so";
            };
          };
          gpu = pkgs.mkShell {
            nativeBuildInputs = [ pkgs.cargo pkgs.rustc ];
            env = {
              LD_LIBRARY_PATH = "${pkgs.vulkan-loader}/lib:/run/opengl-driver/lib";
              VK_ICD_FILENAMES = "/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json";
            };
          };
        });

      nixosModules = nixosModules // { default = nixosModules.catalyrst; };
    };
}
