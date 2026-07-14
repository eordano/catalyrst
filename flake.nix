{
  description = "catalyrst — Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  # No nixpkgs follows: abgen's rust-toolchain.toml moves with its own
  # flake.lock (its nix guard refuses a nixpkgs whose rustc mismatches).
  inputs.abgen.url = "github:decentraland/abgen";

  outputs = inputs@{ self, nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems
        (system: f (import nixpkgs { inherit system; }));

      nixosModules.catalyrst = import ./nixos/configuration.nix;
    in
    {
      packages = forAllSystems (pkgs:
        let
          # Shared across every buildRustPackage: the workspace lock plus the
          # git-dependency hashes cargo vendoring needs (web_transport is fetched
          # from git, so buildRustPackage cannot derive its hash from the lock).
          cargoLockShared = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "web_transport-0.1.0" = "sha256-2QwYPooH7gVUenYVXZ24kuB0A19UwO1ICzolkvdo5sI=";
            };
          };
        in
        rec {
          pulse = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-pulse";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-market" "--bin" "catalyrst-market" ];
            doCheck = false;
          };

          catalyrst-map = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-map";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-map" "--bin" "catalyrst-map" ];
            doCheck = false;
          };

          catalyrst-places = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-places";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-places" "--bin" "catalyrst-places" ];
            doCheck = false;
          };

          catalyrst-camera-reel = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-camera-reel";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-camera-reel" "--bin" "catalyrst-camera-reel" ];
            doCheck = false;
          };

          catalyrst-events = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-events";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-events" "--bin" "catalyrst-events" ];
            doCheck = false;
          };

          catalyrst-communities = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-communities";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-social-service" "--bin" "catalyrst-communities" ];
            nativeBuildInputs = [ pkgs.protobuf ];
            doCheck = false;
          };

          catalyrst-explorer-api = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-explorer-api";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-badges" "--bin" "catalyrst-badges" ];
            doCheck = false;
          };

          catalyrst-economy = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-economy";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-archipelago" "--bin" "catalyrst-archipelago" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-bvimposters = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-bvimposters";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-bvimposters" "--bin" "catalyrst-bvimposters" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          catalyrst-preview-tunnel = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-preview-tunnel";
            version = "0.14.1";
            src = ./.;
            cargoLock = cargoLockShared;
            cargoBuildFlags = [ "-p" "catalyrst-preview-tunnel" "--bin" "catalyrst-preview-tunnel" ];
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
            env.OPENSSL_NO_VENDOR = "1";
          };

          abgen = inputs.abgen.packages.${pkgs.stdenv.hostPlatform.system}.default;

          librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };
          catalyrst-scene-state = pkgs.rustPlatform.buildRustPackage {
            pname = "catalyrst-scene-state";
            version = "0.1.0";
            src = ./.;
            cargoLock = cargoLockShared;
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
            cargoLock = cargoLockShared;
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
              "catalyrst-social-service"
              "--features"
              "catalyrst-social-service/rpc"
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
          rust197 = (pkgs.extend (import rust-overlay)).rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
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
