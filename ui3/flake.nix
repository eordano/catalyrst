{
  description = "ui3 -- dcl-react-ui Storybook static component catalog";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      linuxSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      allSystems = linuxSystems ++ [
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forLinuxSystems = f: nixpkgs.lib.genAttrs linuxSystems (system: f (import nixpkgs { inherit system; }));
      forAllSystems = f: nixpkgs.lib.genAttrs allSystems (system: f (import nixpkgs { inherit system; }));
    in
    {
      packages = forLinuxSystems (
        pkgs:
        let
          nodejs = pkgs.nodejs_26;
        in
        rec {
          storybook = pkgs.buildNpmPackage {
            pname = "ui3-storybook";
            version = "0.0.0";
            src = ./.;
            inherit nodejs;
            npmDepsHash = "sha256-b8NMkKpbIQsSoxqsSD6aAiUWKVik9valsiqnZ+Y4AGU=";

            npmBuildScript = "build-storybook";

            nativeBuildInputs = [ pkgs.rsync ];

            installPhase = ''
              runHook preInstall
              mkdir -p $out
              cp -r storybook-static/. $out/
              runHook postInstall
            '';
          };
          default = storybook;
        }
      );

      devShells = forAllSystems (
        pkgs:
        let
          nodejs = pkgs.nodejs_26;
        in
        {
          default = pkgs.mkShell {
            name = "ui3";
            packages = [
              nodejs
              pkgs.pnpm
              pkgs.rsync
              pkgs.jq
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
            shellHook = ''
              echo "ui3 dev shell (dcl-react-ui) -- node $(node --version)"
              echo "  npm ci --ignore-scripts - npm run storybook (:5006) - npm run dev - npm test"
            '';
          };
        }
      );
    };
}
