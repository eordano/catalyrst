{
  description = "catalyrst — Rust Decentraland catalyst (content + lambdas + write path)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.archipelago = { url = "github:decentraland/archipelago-workers/537def15e2609cf0ecc8ba5bd7ad400702e455c8"; flake = false; };
  inputs.uws-node24 = { url = "github:uNetworking/uWebSockets.js/v20.67.0"; flake = false; };
  inputs.pulse-src = {
    url = "github:decentraland/Pulse/d7b13d7";
    flake = false;
  };

  outputs = { self, nixpkgs, flake-utils, archipelago, uws-node24, pulse-src }:
    let
      # NixOS module is system-independent — expose it at the top level.
      nixosModules.catalyrst = import ./nixos/configuration.nix;
    in
    (flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        nodejs = pkgs.nodejs_24;
      in {
        packages = rec {
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

          pulse = pkgs.buildDotnetModule {
            pname = "dclpulse";
            version = "0.1.0";
            src = pulse-src;
            projectFile = "src/DCLPulse/DCLPulse.csproj";
            executables = [ "DCLPulse" ];
            dotnet-sdk = pkgs.dotnet-sdk_10;
            dotnet-runtime = pkgs.dotnet-runtime_10;
            nugetDeps = ./nixos/pulse-deps.json;
            dotnetFlags = [ "-p:GenerateProto=false" ];
            runtimeDeps = [ pkgs.enet ];
            meta.mainProgram = "DCLPulse";
            postPatch = ''
              substituteInPlace src/DCLPulse/HttpServiceOptions.cs \
                --replace-fail \
                  'public ushort Port { get; set; } = 5000;' \
                  'public ushort Port { get; set; } = 5000;
      public string? Host { get; set; }'
              substituteInPlace src/DCLPulse/HttpService.cs \
                --replace-fail \
                  'string host = OperatingSystem.IsWindows() ? "localhost" : "+";' \
                  'string host = OperatingSystem.IsWindows() ? "localhost" : (string.IsNullOrEmpty(options.Value.Host) ? "+" : options.Value.Host);'
            '';
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
          };

          # Marketplace REST API in front of squid_marketplace (port of
          # decentraland/marketplace-server). Loopback Postgres only —
          # sqlx is built without a TLS feature so no openssl needed.
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

          default = catalyrst;
        };
      }
    )) // {
      # Reusable NixOS module. Operators import it from their own host config
      # and set `services.catalyrst.*` options. See nixos/module-example.nix
      # for a minimal consumer.
      nixosModules = nixosModules // { default = nixosModules.catalyrst; };
    };
}
