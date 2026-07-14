{
  description = "catalyrst -- Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  # No nixpkgs follows: abgen's rust-toolchain.toml moves with its own
  # flake.lock (its nix guard refuses a nixpkgs whose rustc mismatches).
  inputs.abgen.url = "github:decentraland/abgen";
  inputs.crane.url = "github:ipetkov/crane/v0.21.0";

  outputs = inputs@{ self, nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      # ONE nixpkgs instantiation and ONE crane scope per system, shared by
      # every output family. Instantiating nixpkgs (a full fixpoint) or the
      # crane scope once per family multiplies eval time and memory -- and the
      # flake is re-evaluated on every deploy/CI run, so eval cost is deploy
      # latency. Output families below must reuse pkgsFor/scopeFor, never
      # `import nixpkgs` or `catalyrstFor` directly.
      pkgsFor = nixpkgs.lib.genAttrs systems (system: import nixpkgs { inherit system; });
      forAllSystems = f: nixpkgs.lib.mapAttrs (_: f) pkgsFor;
      scopeFor = nixpkgs.lib.mapAttrs (_: pkgs: catalyrstFor pkgs) pkgsFor;

      nixosModules.catalyrst = import ./nixos;

      # One crane scope per system, shared by packages + checks so both build
      # against the SAME cargoArtifacts (no drifting second copy of commonArgs).
      catalyrstFor = pkgs:
        let
          # Everything the Rust build genuinely reads, and nothing else. Keeps
          # docs/, sites/, ui3/, nixos/, deploy/, scripts/ and README.md out of
          # the derivation input, so editing them no longer invalidates a Rust
          # build. crates/ carries the per-crate proto/, migrations/ and .sqlx/
          # trees; third_party/ carries the rusty_enet [patch.crates-io] target.
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

          craneLib = inputs.crane.mkLib pkgs;

          librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };

          # The SUPERSET inputs/env, byte-identical between the deps-only layer
          # and every package. This identity is the whole point: any per-package
          # divergence here silently re-forks the dependency graph and restores
          # the ~25x compile. So packages override ONLY pname/version/
          # cargoExtraArgs/postInstall/meta -- never these fields.
          #
          # openssl/protobuf/pkg-config/RUSTY_V8 are harmless supersets for
          # crates that ignore them: protoc/pkg-config sit unused in PATH,
          # openssl is only consulted by openssl-sys' build script, and the two
          # env vars are read only by openssl-sys / rusty_v8 build scripts.
          commonArgs = {
            src = rustSrc;
            strictDeps = true;
            doCheck = false;
            nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
            buildInputs = [ pkgs.openssl ];
            OPENSSL_NO_VENDOR = "1";
            RUST_MIN_STACK = "16777216";
            # web_transport is the workspace's one git dep; pin its vendor hash so
            # eval never falls back to a network fetchGit (breaks restricted eval
            # + binary-cache substitution). Identical on the deps and package args.
            outputHashes = {
              "git+https://github.com/decentraland/rust-web-transport?rev=c5416501f6ffc4a11303980f9811426ae34c77ef#c5416501f6ffc4a11303980f9811426ae34c77ef" =
                "sha256-2QwYPooH7gVUenYVXZ24kuB0A19UwO1ICzolkvdo5sI=";
            };
          }
          # librusty_v8 pins a hash for x86_64-linux only and throws elsewhere;
          # keep it out of the shared args on the other systems so every package
          # (not just scene-state/all) can still evaluate there.
          // pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
            RUSTY_V8_ARCHIVE = "${librusty_v8}";
          };

          # The one shared third-party graph. `--workspace` compiles the deps of
          # every member (self-maintaining: a new crate needs no flake edit); the
          # explicit `catalyrst-social-service/rpc` is the ONLY non-default
          # feature any package enables (catalyrst-all's catalyrst-social-rpc
          # bin), so without it dcl-rpc/prost/tonic would miss the deps layer and
          # recompile in the package build. `nix build .#catalyrst-workspace-deps`
          # warms this once before any package build.
          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            pname = "catalyrst-workspace-deps";
            version = "0.1.0";
            cargoExtraArgs = "--locked --workspace --features catalyrst-social-service/rpc";
          });

          # Overriding cargoExtraArgs replaces crane's default `--locked`, so
          # every package re-adds it.
          mkPkg = args: craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; } // args);

          # A workspace service whose crate, bin and attr name all match.
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

            # No --features rpc: only the catalyrst-social-rpc bin (built inside
            # catalyrst-all) needs it. Adding it here would change what this
            # binary links.
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

            # The prod artifact: every service bin in one derivation. The shared
            # cargoArtifacts already compiled the rpc subtree, so this only links
            # catalyrst-all's own bins.
            catalyrst-all = mkPkg {
              pname = "catalyrst-all";
              version = "0.1.0";
              cargoExtraArgs = "--locked -p catalyrst-server --bin catalyrst-live -p catalyrst-explore --bin catalyrst-explore -p catalyrst-create --bin catalyrst-create -p catalyrst-data --bin catalyrst-data -p catalyrst-social --bin catalyrst-social -p catalyrst-social-service --features catalyrst-social-service/rpc --bin catalyrst-social-rpc -p catalyrst-explorer-api --bin catalyrst-explorer-api -p catalyrst-profile-images --bin catalyrst-profile-images -p catalyrst-scene-state --bin catalyrst-scene-state -p catalyrst-signatures --bin catalyrst-signatures -p catalyrst-telemetry --bin catalyrst-telemetry -p catalyrst-worlds --bin catalyrst-world-storage -p catalyrst-land-authz --bin catalyrst-land-authz-index";
              postInstall = migrationsPostInstall;
            };

            # The marketplace/LAND/ENS indexer -- TypeScript, first-class fork at
            # contracts/squid (not part of the crane/cargo scope above).
            squid = pkgs.callPackage ./nix/squid.nix { };

            # The SSR web tier: operator /server console + realm surfaces.
            # sites/ and ui3/ ride the MANIFEST; the sanitize gate polices them.
            sites = pkgs.callPackage ./nix/sites.nix { };

            # Consumed by bevy-explorer's flake, so its /play bundle can be built
            # from source instead of from build artifacts committed into its tree.
            ui3-overlay = pkgs.callPackage ./nix/ui3-overlay.nix { };

            # The avatar renderer behind catalyrst-profile-images: decentraland's
            # Godot fork (pinned by build SHA) plus a headless Linux export.
            godot-explorer = pkgs.callPackage ./nix/godot-explorer.nix { };

            default = catalyrst;
          }
          # The abgen input doesn't cover every system this flake evaluates for
          # (it has no x86_64-darwin), and it only ships the ab-cdn parity
          # harness on revs that still carry its python pipeline/. Guard both
          # re-exports on what the input actually provides, so the attrs are
          # genuinely ABSENT (not present-but-throwing) where unavailable:
          # downstream `or` fallbacks work, `nix flake show` stays green, and
          # the attrs return automatically when the input provides them.
          # Deployments that still run the harness pin an older catalyrst whose
          # abgen lock predates the removal (the hive's `catalyrst-compare`).
          // (let abgenPkgs = inputs.abgen.packages.${pkgs.stdenv.hostPlatform.system} or { }; in
            pkgs.lib.optionalAttrs (abgenPkgs ? default) { abgen = abgenPkgs.default; }
            // pkgs.lib.optionalAttrs (abgenPkgs ? abgen-compare) { abgen-compare = abgenPkgs.abgen-compare; });
        in
        { inherit craneLib commonArgs cargoArtifacts packages; };
    in
    {
      packages = nixpkgs.lib.mapAttrs (_: scope: scope.packages) scopeFor;

      # Deliberately NOT reached through `packages`. That attrset is finished with
      # `// (optionalAttrs (abgenPkgs ? default) ...)`, and `//` forces its right
      # operand, so selecting ANY attribute from it evaluates the abgen flake and
      # its own nixpkgs. bevy-explorer needs only the overlay and should not take
      # a hard dependency on abgen being fetchable to build /play.
      webPackages = nixpkgs.lib.mapAttrs
        (_: pkgs: { ui3-overlay = pkgs.callPackage ./nix/ui3-overlay.nix { }; })
        pkgsFor;

      # Stateless, sandboxed tests. `nix flake check` (or
      # `nix build .#checks.<system>.catalyrst-server-tests`) runs the
      # catalyrst-server input-validation unit tests (nul_guard middleware,
      # DatabaseError->AppError mapping, active_entities validator) against the
      # SAME shared cargoArtifacts -- deps already compiled, so the test build
      # links only catalyrst-server. The crates use plain `cargo test`.
      checks = forAllSystems (pkgs:
        let c = scopeFor.${pkgs.stdenv.hostPlatform.system}; in {
          catalyrst-server-tests = c.craneLib.cargoTest (c.commonArgs // {
            pname = "catalyrst-server-tests";
            cargoArtifacts = c.cargoArtifacts;
            # commonArgs carries doCheck=false (right for buildPackage); crane's
            # cargoTest runs the suite in checkPhase, so it must be re-enabled or
            # the tests silently never run.
            doCheck = true;
            # The test output is a pass/fail gate, not a reusable artifact -- don't
            # install the whole release target dir (gigabytes) as $out.
            doInstallCargoArtifacts = false;
            cargoTestExtraArgs = "-p catalyrst-server";
            # Two hermetic-sandbox enablers, matching how the suite runs without
            # infra: the content-encoding tests build a reqwest::Client whose
            # native-roots TLS loader panics with no system trust store (point it
            # at nixpkgs' bundle; the requests hit a localhost mock), and the
            # catalyrst-testgate DB integration tests (land_publish, schema,
            # sync_*) skip instead of failing when no Postgres is reachable.
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            ALLOW_SKIPPED_INTEGRATION = "1";
          });
        }
        # The module's runtime behaviour -- unit ordering, secret generation,
        # sandbox, the rendered nginx config -- is invisible to drvPath eval, so
        # boot it. VM tests are x86_64-linux only.
        // pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
          module-first-boot = import ./nixos/tests/first-boot.nix { inherit pkgs self; };
        });

      devShells = forAllSystems (pkgs:
        let
          librusty_v8 = scopeFor.${pkgs.stdenv.hostPlatform.system}.packages.librusty_v8;
          rust197 = (pkgs.extend (import rust-overlay)).rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          # sqlx-cli must match the workspace's sqlx minor (Cargo.lock: 0.9.0) or
          # it writes .sqlx entries the query! macros cannot read; nixpkgs still
          # ships 0.8.x, so build 0.9.0 from the crates.io package with the same
          # feature set CI's `cargo install sqlx-cli` uses.
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
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
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

      # `nix run .#init` -- scaffold a starter host config (nixos/scaffold/init.sh).
      apps = forAllSystems (pkgs: {
        init = {
          type = "app";
          program = "${pkgs.writeShellScriptBin "catalyrst-init"
            (builtins.readFile ./nixos/scaffold/init.sh)}/bin/catalyrst-init";
        };
      });
    };
}
