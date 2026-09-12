{
  description = "catalyrst -- Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  inputs.abgen.url = "github:decentraland/abgen/v0.17.10";
  inputs.crane.url = "github:ipetkov/crane/v0.21.0";

  outputs = inputs@{ self, nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      pkgsFor = nixpkgs.lib.genAttrs systems (system: import nixpkgs { inherit system; });
      forAllSystems = f: nixpkgs.lib.mapAttrs (_: f) pkgsFor;
      scopeFor = nixpkgs.lib.mapAttrs (_: pkgs: catalyrstFor pkgs) pkgsFor;

      nixosModules.catalyrst = import ./nixos;

      catalyrstFor = pkgs:
        let
          rustSrc = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./clippy.toml
              ./rust-toolchain.toml
              ./.cargo
              ./crates
              ./third_party
            ];
          };

          pinnedRustToolchain = (pkgs.extend (import rust-overlay)).rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain pinnedRustToolchain;

          librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };

          commonArgs = {
            src = rustSrc;
            strictDeps = true;
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            OPENSSL_NO_VENDOR = "1";
            RUST_MIN_STACK = "16777216";
            outputHashes = {
              "git+https://github.com/decentraland/rust-web-transport?rev=c5416501f6ffc4a11303980f9811426ae34c77ef#c5416501f6ffc4a11303980f9811426ae34c77ef" =
                "sha256-2QwYPooH7gVUenYVXZ24kuB0A19UwO1ICzolkvdo5sI=";
            };
          }
          // pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
            RUSTY_V8_ARCHIVE = "${librusty_v8}";
          };

          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            pname = "catalyrst-workspace-deps";
            version = "0.1.0";
            cargoExtraArgs = "--locked --workspace --features catalyrst-social-service/rpc";
          });

          mkPkg = args: craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; } // args);

          svc = name: mkPkg {
            pname = name;
            version = "0.1.0";
            cargoExtraArgs = "--locked -p ${name} --bin ${name}";
          };

          migrationsPostInstall = ''
            mkdir -p "$out/share/catalyrst-server"
            cp -r crates/catalyrst-server/migrations "$out/share/catalyrst-server/migrations"
          '';

          packages = rec {
            inherit librusty_v8;

            catalyrst-workspace-deps = cargoArtifacts;

            pulse = mkPkg {
              pname = "catalyrst-pulse";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-pulse --bin catalyrst-pulse";
              meta.mainProgram = "catalyrst-pulse";
            };

            catalyrst = mkPkg {
              pname = "catalyrst";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-server --bin catalyrst-live";
              postInstall = migrationsPostInstall;
            };

            catalyrst-market = svc "catalyrst-market";

            catalyrst-map = svc "catalyrst-map";

            catalyrst-places = svc "catalyrst-places";

            catalyrst-camera-reel = svc "catalyrst-camera-reel";

            catalyrst-events = svc "catalyrst-events";

            catalyrst-communities = mkPkg {
              pname = "catalyrst-communities";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-social-service --bin catalyrst-communities";
            };

            catalyrst-explorer-api = svc "catalyrst-explorer-api";

            catalyrst-governance = svc "catalyrst-governance";

            catalyrst-presence = svc "catalyrst-presence";

            catalyrst-price = svc "catalyrst-price";

            catalyrst-notifications = svc "catalyrst-notifications";

            catalyrst-badges = svc "catalyrst-badges";

            catalyrst-economy = svc "catalyrst-economy";

            catalyrst-media = svc "catalyrst-media";

            catalyrst-rpc = svc "catalyrst-rpc";

            catalyrst-credits = svc "catalyrst-credits";

            catalyrst-worlds = mkPkg {
              pname = "catalyrst-worlds";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-worlds --bin catalyrst-worlds --bin worlds-mirror";
            };

            catalyrst-builder = svc "catalyrst-builder";

            catalyrst-comms = svc "catalyrst-comms";

            catalyrst-archipelago = svc "catalyrst-archipelago";

            catalyrst-bvimposters = svc "catalyrst-bvimposters";

            catalyrst-preview-tunnel = mkPkg {
              pname = "catalyrst-preview-tunnel";
              version = "0.14.1";
              cargoExtraArgs = "--locked -p catalyrst-preview-tunnel --bin catalyrst-preview-tunnel";
            };

            catalyrst-scene-state = svc "catalyrst-scene-state";

            catalyrst-all = mkPkg {
              pname = "catalyrst-all";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-server --bin catalyrst-live -p catalyrst-explore --bin catalyrst-explore -p catalyrst-create --bin catalyrst-create -p catalyrst-data --bin catalyrst-data -p catalyrst-social --bin catalyrst-social -p catalyrst-social-service --features catalyrst-social-service/rpc --bin catalyrst-social-rpc -p catalyrst-explorer-api --bin catalyrst-explorer-api -p catalyrst-profile-images --bin catalyrst-profile-images -p catalyrst-scene-state --bin catalyrst-scene-state -p catalyrst-signatures --bin catalyrst-signatures -p catalyrst-telemetry --bin catalyrst-telemetry -p catalyrst-worlds --bin catalyrst-world-storage -p catalyrst-land-authz --bin catalyrst-land-authz-index";
              postInstall = migrationsPostInstall;
            };

            squid = pkgs.callPackage ./nix/squid.nix { };

            sites = pkgs.callPackage ./nix/sites.nix { };

            ui3-overlay = pkgs.callPackage ./nix/ui3-overlay.nix { };

            godot-explorer = pkgs.callPackage ./nix/godot-explorer.nix { };

            default = catalyrst;
          }
          // (let abgenPkgs = inputs.abgen.packages.${pkgs.stdenv.hostPlatform.system} or { }; in
            pkgs.lib.optionalAttrs (abgenPkgs ? default) { abgen = abgenPkgs.default; }
            // pkgs.lib.optionalAttrs (abgenPkgs ? abgen-compare) { abgen-compare = abgenPkgs.abgen-compare; });
        in
        { inherit craneLib commonArgs cargoArtifacts packages; };
    in
    {
      packages = nixpkgs.lib.mapAttrs (_: scope: scope.packages) scopeFor;

      webPackages = nixpkgs.lib.mapAttrs
        (_: pkgs: { ui3-overlay = pkgs.callPackage ./nix/ui3-overlay.nix { }; })
        pkgsFor;

      checks = forAllSystems (pkgs:
        let c = scopeFor.${pkgs.stdenv.hostPlatform.system}; in {
          catalyrst-server-tests = c.craneLib.cargoTest (c.commonArgs // {
            pname = "catalyrst-server-tests";
            cargoArtifacts = c.cargoArtifacts;
            doCheck = true;
            doInstallCargoArtifacts = false;
            cargoTestExtraArgs = "-p catalyrst-server";
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            ALLOW_SKIPPED_INTEGRATION = "1";
          });
        }
        // pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
          module-first-boot = import ./nixos/tests/first-boot.nix { inherit pkgs self; };
        });

      devShells = forAllSystems (pkgs:
        let
          librusty_v8 = scopeFor.${pkgs.stdenv.hostPlatform.system}.packages.librusty_v8;
          rust197 = (pkgs.extend (import rust-overlay)).rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          sqlx-cli-090 = pkgs.rustPlatform.buildRustPackage {
            pname = "sqlx-cli";
            version = "0.9.0";
            src = pkgs.fetchCrate {
              pname = "sqlx-cli";
              version = "0.9.0";
              hash = "sha256-XariusjsCgn0Qai0XWtr7EzSzDDTp1cCzjff1kJNO9Y=";
            };
            cargoHash = "sha256-pHaMKuB9v3fjbgeVyLyRtfoQ9BkE6z+TjDfdBaVdbXM=";
            buildNoDefaultFeatures = true;
            buildFeatures = [ "postgres" ];
            doCheck = false;
          };
        in
        {
          default = pkgs.mkShell {

            hardeningDisable = [ "fortify" ];
            nativeBuildInputs = [
              rust197
              pkgs.rust-analyzer
              pkgs.pkg-config
              pkgs.protobuf
              pkgs.gnumake
              sqlx-cli-090
              pkgs.postgresql
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
              sqlx-cli-090
              pkgs.postgresql
            ];
            buildInputs = [ pkgs.openssl ];
            env = {
              OPENSSL_NO_VENDOR = "1";
              RUSTY_V8_ARCHIVE = "${librusty_v8}";
            };
          };
        });

      nixosModules = nixosModules // { default = nixosModules.catalyrst; };

      apps = forAllSystems (pkgs: {
        init = {
          type = "app";
          program = "${pkgs.writeShellScriptBin "catalyrst-init"
            (builtins.readFile ./nixos/scaffold/init.sh)}/bin/catalyrst-init";
        };
      });
    };
}
