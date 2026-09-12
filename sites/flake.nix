{
  description = "sites -- React Router 8 SSR Catalyst Places explorer";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs, ... }:
    let
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
