{
  description = "catalyrst — Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.archipelago = { url = "github:decentraland/archipelago-workers/537def15e2609cf0ecc8ba5bd7ad400702e455c8"; flake = false; };
  inputs.uws-node24 = { url = "github:uNetworking/uWebSockets.js/v20.67.0"; flake = false; };
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  # abgen source-of-truth pin — flip to github:decentraland/abgen once PR #6 merges
  inputs.abgen = { url = "github:eordano/abgen/abgen-registry"; inputs.nixpkgs.follows = "nixpkgs"; };

  outputs = inputs@{ self, nixpkgs, archipelago, uws-node24, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems
        (system: f (import nixpkgs { inherit system; }));

      nixosModules.catalyrst = import ./nixos/configuration.nix;
    in
    {
      packages = forAllSystems (pkgs:
        let
          nodejs = pkgs.nodejs_24;
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

          abgen = inputs.abgen.packages.${pkgs.stdenv.hostPlatform.system}.default;

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
            ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
            postInstall = ''
              mkdir -p "$out/share/catalyrst-server"
              cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
            '';
          };

          abgen-compare = inputs.abgen.packages.${pkgs.stdenv.hostPlatform.system}.abgen-compare;

          default = catalyrst;
        });

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
              pkgs.gnumake
            ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
          };

          ci = pkgs.mkShell {
            hardeningDisable = [ "fortify" ];
            nativeBuildInputs = [
              rust197
              pkgs.pkg-config
              pkgs.protobuf
              pkgs.gnumake
            ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
          };
        });

      nixosModules = nixosModules // { default = nixosModules.catalyrst; };
    };
}
