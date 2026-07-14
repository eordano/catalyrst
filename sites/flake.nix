{
  description = "sites — React Router 8 SSR Catalyst Places explorer";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  inputs.ui3 = { url = "path:../ui3"; flake = false; };

  outputs = { self, nixpkgs, ui3 }:
    let
      # The `sites` server package (a buildNpmPackage whose react-router-serve
      # output is deployed as a Linux systemd unit by the deployment) stays pinned to
      # the Linux systems below. The dev shell, however, evaluates on every
      # default system — notably aarch64-darwin, so `nix develop ~/one/catalyrst/sites`
      # works on this Mac.
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      allSystems = linuxSystems ++ [ "x86_64-darwin" "aarch64-darwin" ];
      forSystems = systems: f: nixpkgs.lib.genAttrs systems
        (system: f (import nixpkgs { inherit system; }));
      forLinuxSystems = forSystems linuxSystems;
      forAllSystems = forSystems allSystems;
    in
    {
      packages = forLinuxSystems (pkgs:
        let
          lib = pkgs.lib;
          nodejs = pkgs.nodejs_26;
        in
        rec {
          sites = pkgs.buildNpmPackage {
            pname = "sites";
            version = "0.0.0";

            src = ./.;
            inherit nodejs;

            # Regenerate whenever package-lock.json changes:
            #   nix run nixpkgs#prefetch-npm-deps -- catalyrst/sites/package-lock.json
            # A stale value fails the build with "npmDepsHash is out of date", and
            # because `nix profile upgrade` is all-or-nothing that aborts the whole
            # the deployment profile transaction — so the entire prod stack silently stops
            # tracking source. That is half of how prod ended up 407 commits behind
            # by 2026-07-27; the other half was a retired catalyrst-registry entry
            # still installed in the profile.
            npmDepsHash = "sha256-KnUjLXymHfrgAWSGq3sLtFMxd9AdjTqcA7EkVbnrQIE=";

            nativeBuildInputs = [ pkgs.makeWrapper ];

            npmBuildScript = "build";

            preBuild = ''
              mkdir -p ../ui3
              cp -r ${ui3}/src ../ui3/src
              cp -r ${ui3}/public ../ui3/public
              chmod -R u+w ../ui3
              # sites/public is gitignored (regenerated from ui3/public by the
              # predev/prebuild `sync:assets` hook). .npmrc ignore-scripts=true
              # skips that lifecycle hook during the build, so run the sync
              # explicitly here — otherwise react-router build ships build/client/
              # with NO public assets and prod (react-router-serve) 404s
              # /favicon.ico + /assets/* site-wide.
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
              makeWrapper ${nodejs}/bin/node $out/bin/sites-server \
                --add-flags "$out/node_modules/.bin/react-router-serve $out/build/server/index.js" \
                --chdir "$out"

              runHook postInstall
            '';

            doCheck = false;

            meta = {
              description = "sites — React Router 8 SSR Catalyst Places explorer";
              mainProgram = "sites-server";
            };
          };

          default = sites;
        });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          name = "sites";
          packages = [
            pkgs.nodejs_26
            pkgs.postgresql_18
            pkgs.ephemeralpg
            pkgs.jq
          ]
          # No Linux-only libs in this shell to translate; libiconv is the
          # standard darwin build dependency to carry so native tooling links.
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
          shellHook = ''
            echo "sites dev shell — npm run dev | npm run build | npm run test:e2e"
            echo "e2e: pg_tmp (ephemeralpg) provides a throwaway postgres"
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style or pkgs.nixfmt);
    };
}
