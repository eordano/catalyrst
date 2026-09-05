{
  description = "sites -- React Router 8 SSR Catalyst Places explorer";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs, ... }:
    let
      # The `sites` server package (a buildNpmPackage whose react-router-serve
      # output is deployed as a Linux systemd unit by the deployment) stays pinned to
      # the Linux systems below. The dev shell, however, evaluates on every
      # default system -- notably aarch64-darwin, so `nix develop ~/one/catalyrst/sites`
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
          # One recipe, defined in catalyrst/nix/sites.nix and shared with
          # catalyrst's own flake. It used to be restated here in full, with the
          # npmDepsHash written out twice while check-npm-deps-hash.sh only
          # maintained one copy. callPackage keeps this flake's own nixpkgs.
          sites = pkgs.callPackage ../nix/sites.nix { };

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
            echo "sites dev shell -- npm run dev | npm run build | npm run test:e2e"
            echo "e2e: pg_tmp (ephemeralpg) provides a throwaway postgres"
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style or pkgs.nixfmt);
    };
}
