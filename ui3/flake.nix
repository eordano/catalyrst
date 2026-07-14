{
  description = "ui3 -- dcl-react-ui Storybook static component catalog";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      # The Storybook static-catalog *package* is a buildNpmPackage that only
      # needs to run on the Linux CI/hosts that serve it, so it stays pinned to
      # the Linux systems below. The dev shell, however, evaluates on every
      # default system -- notably aarch64-darwin, so `nix develop` on this tree
      # (and its `.envrc` `use flake`) works on a Mac.
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
            npmDepsHash = "sha256-NEKBhz+AkRR0Cw8uR/QhUJLF785as+mVbEzyQPFnaLQ=";

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
              # nodejs_26 matches the version the storybook package pins; npm
              # ships with it (the repo's Quickstart is `npm ci`), pnpm covers
              # the pnpm-lock.yaml/pnpm-workspace.yaml lane, rsync backs the
              # build:lib / build-storybook scripts, jq for scripting.
              nodejs
              pkgs.pnpm
              pkgs.rsync
              pkgs.jq
            ]
            # No Linux-only libs to translate; libiconv is the standard darwin
            # build dependency to carry so native node-gyp tooling links.
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
