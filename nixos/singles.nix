{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  d = import ./helpers.nix cfg;

  commsPackages = inputs.catalyrst.packages.x86_64-linux;

  # Fall back to the flake's own builds when an operator leaves the package
  # options null -- matching bundles.nix and squid.nix, so a profile that
  # enables these singles is self-contained (options.nix's profile contract)
  # instead of requiring the operator to wire three packages by hand.
  bundlesPkg = if cfg.bundlesPackage != null then cfg.bundlesPackage else commsPackages.catalyrst-all;
  governancePkg =
    if cfg.governancePackage != null then cfg.governancePackage else commsPackages.catalyrst-governance;
  presencePkg =
    if cfg.presencePackage != null then cfg.presencePackage else commsPackages.catalyrst-presence;

  inherit (import ./sandbox.nix)
    baseSandbox
    rootOneshotSandbox
    ;

  conn = db: "postgresql:///${db}?host=/run/postgresql&user=catalyrst${d.pgPortQuery}";

  telemetryAdminToken = cfg.telemetryAdminTokenFile != null;

  mkSingle =
    {
      description,
      port,
      exec,
      environment,
      extraServiceConfig ? { },
      afterExtra ? [ ],
      wantsExtra ? [ ],
    }:
    {
      inherit description environment;
      after = [
        "postgresql.service"
        "postgresql-bundles.service"
        "network-online.target"
      ]
      ++ afterExtra;
      wants = [
        "network-online.target"
        "postgresql-bundles.service"
      ]
      ++ wantsExtra;
      wantedBy = [ "multi-user.target" ];
      serviceConfig =
        baseSandbox
        // {
          ExecStart = exec;
          Restart = "always";
          RestartSec = 10;
          User = "catalyrst";
          Group = "catalyrst";
          ProtectHome = true;
          ReadWritePaths = [ "/run/postgresql" ];
          MemoryHigh = "512M";
          MemoryMax = "512M";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:${toString port}" ];
          SocketBindDeny = "any";
        }
        // extraServiceConfig;
    };
in
lib.mkIf cfg.enable {

  systemd.tmpfiles.rules = lib.optionals cfg.subServices.profileImages [
    "d ${cfg.stateDir}/profile-images      0755 catalyrst catalyrst -"
    # Godot needs a writable HOME for its config and shader cache; ProtectHome
    # blocks the real one, so it gets a directory inside the cache tree.
    "d ${cfg.stateDir}/profile-images/.godot-home 0700 catalyrst catalyrst -"
  ];

  systemd.services =
    lib.optionalAttrs cfg.subServices.telemetry {
      catalyrst-telemetry = mkSingle {
        description = "catalyrst-telemetry (event sink + dashboard, port 5150)";
        port = 5150;
        exec =
          if telemetryAdminToken then
            pkgs.writeShellScript "catalyrst-telemetry-launcher" ''
              export CATALYRST_TELEMETRY_ADMIN_TOKEN="$(cat "$CREDENTIALS_DIRECTORY/telemetry-admin-token")"
              exec ${bundlesPkg}/bin/catalyrst-telemetry
            ''
          else
            "${bundlesPkg}/bin/catalyrst-telemetry";
        extraServiceConfig = lib.optionalAttrs telemetryAdminToken {
          LoadCredential = "telemetry-admin-token:${cfg.telemetryAdminTokenFile}";
        };
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5150";
          TELEMETRY_PG_CONNECTION_STRING = "postgresql:///catalyrst?host=/run/postgresql&user=catalyrst${d.pgPortQuery}&options=-c%%20search_path%%3Dtelemetry";
          TELEMETRY_BASE_PATH = "/telemetry";
          FLAGS_URL = "http://127.0.0.1:5137/explorer.json";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.worldStorage {
      catalyrst-world-storage = mkSingle {
        description = "catalyrst-world-storage (world env/player key-value store + ACLs, port 5154)";
        port = 5154;
        exec = pkgs.writeShellScript "catalyrst-world-storage-launcher" ''
          set -a
          . "$CREDENTIALS_DIRECTORY/world-storage-env"
          set +a
          exec ${bundlesPkg}/bin/catalyrst-world-storage
        '';
        afterExtra = [ "catalyrst-world-storage-secret.service" ];
        wantsExtra = [ "catalyrst-world-storage-secret.service" ];
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5154";
          WORLD_STORAGE_PG_CONNECTION_STRING = conn "worlds";
          LAMBDAS_URL = "http://127.0.0.1:5141/lambdas";
          WORLDS_CONTENT_SERVER_URL = "http://127.0.0.1:5143";
          PLACES_URL = "http://127.0.0.1:5143";
          RPC_ENDPOINT_ETH = cfg.ethRpcUrl;
          CORS_ALLOWED_ORIGIN_SUFFIXES = "decentraland.org,decentraland.zone,decentraland.today,${cfg.domain}";
        };
        extraServiceConfig = {
          LoadCredential = "world-storage-env:${cfg.secretsDir}/catalyrst-world-storage.env";
        };
      };
      catalyrst-world-storage-secret = {
        description = "Generate the catalyrst-world-storage ENCRYPTION_KEY";
        wantedBy = [ "multi-user.target" ];
        before = [ "catalyrst-world-storage.service" ];
        serviceConfig = rootOneshotSandbox // {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "root";
          ReadWritePaths = [ cfg.secretsDir ];
        };
        script = ''
          set -euo pipefail
          umask 077
          ENV=${cfg.secretsDir}/catalyrst-world-storage.env
          if [ ! -s "$ENV" ]; then
            printf 'ENCRYPTION_KEY=%s\n' "$(${pkgs.openssl}/bin/openssl rand -hex 32)" > "$ENV"
            chmod 600 "$ENV"
          fi
        '';
      };
    }
    // lib.optionalAttrs cfg.subServices.profileImages {
      catalyrst-profile-images =
        let
          r = cfg.profileImagesRender;
          godotPkg = if r.package != null then r.package else commsPackages.godot-explorer;
          cacheDir = "${cfg.stateDir}/profile-images";
          # Godot writes its own config and shader cache under HOME/XDG. The
          # sandbox has ProtectHome, so it needs a writable directory of its own
          # or it fails on first render rather than at start.
          godotHome = "${cacheDir}/.godot-home";
        in
        mkSingle {
          description =
            if r.enable then
              "catalyrst-profile-images (local avatar renderer + cache, port 5161)"
            else
              "catalyrst-profile-images (profile picture proxy + cache, port 5161)";
          port = 5161;
          exec = "${bundlesPkg}/bin/catalyrst-profile-images";
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5161";
            PROFILE_IMAGES_CACHE_DIR = cacheDir;
            PROFILE_IMAGES_CACHE_MAX_BYTES = toString r.cacheMaxBytes;
          }
          // (
            if r.enable then
              {
                PROFILE_IMAGES_BACKEND = "render";
                # The -xvfb variant, not the bare binary. Godot's --headless
                # flag selects its dummy rendering server, where
                # async_get_viewport_image() returns null and every render dies
                # on `Parameter "t" is null`. Rendering needs a real GL context,
                # so each invocation gets a throwaway X display of its own.
                PROFILE_IMAGES_GODOT_BIN =
                  "${godotPkg}/bin/decentraland-godot-client-xvfb";
                # Resolve profiles from this node's own content core, which is
                # what makes the renderer independent of Decentraland's servers.
                PROFILE_IMAGES_CONTENT_URL = "http://127.0.0.1:5141/content";
                PROFILE_IMAGES_MAX_CONCURRENT = toString r.maxConcurrentRenders;
                # baseSandbox sets PrivateDevices, so there is no /dev/dri and no
                # GPU. Software GL is also the honest default for the target
                # deployment: a small VPS has no GPU either. A node with a GPU
                # can drop PrivateDevices and unset this.
                LIBGL_ALWAYS_SOFTWARE = "1";
                HOME = godotHome;
                XDG_DATA_HOME = "${godotHome}/.local/share";
                XDG_CONFIG_HOME = "${godotHome}/.config";
                XDG_CACHE_HOME = "${godotHome}/.cache";
              }
            else
              { PROFILE_IMAGES_ORIGIN_URL = "https://profile-images.decentraland.org"; }
          );
          extraServiceConfig = {
            ReadWritePaths = [ cacheDir ];
          }
          // lib.optionalAttrs r.enable {
            # Two narrow additions to the shared filter, each measured against a
            # unit that otherwise starts healthy and then 502s every render:
            # the X server calls capset() to drop privileges, and capset lives in
            # @privileged, so the shared `~@privileged` kills it before it can
            # create a display; godot's V8 wants memory protection keys, and
            # @pkey is in no other set here. Everything else in baseSandbox is
            # confirmed harmless to the renderer.
            #
            # This must stay ONE ordered string. A second SystemCallFilter= line
            # re-adding capset does not restore it -- only re-adding it after the
            # subtraction inside a single list does.
            SystemCallFilter = "@system-service @pkey ~@privileged capset";
            # An engine process blows straight through the 512M shared default.
            MemoryHigh = r.memoryMax;
            MemoryMax = r.memoryMax;
            # One engine process per render, each with its own thread pool.
            TasksMax = 4096;
            # Renders are seconds, not milliseconds: give a slow first render
            # room before systemd calls the unit dead.
            TimeoutStartSec = 180;
          };
        };
    }
    // lib.optionalAttrs cfg.subServices.signatures {
      catalyrst-signatures = mkSingle {
        description = "catalyrst-signatures (auth-chain signature index, port 5159)";
        port = 5159;
        exec = "${bundlesPkg}/bin/catalyrst-signatures";
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5159";
          SIGNATURES_PG_CONNECTION_STRING = conn "signatures";
          DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "marketplace_squid";
          DAPPS_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
          CHAIN_NAME = "ETHEREUM_MAINNET";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.governance {
      catalyrst-governance = mkSingle {
        description = "catalyrst-governance (governance mirror + read API, port 5151)";
        port = 5151;
        exec = "${governancePkg}/bin/catalyrst-governance";
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5151";
          GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "governance";
          GOVERNANCE_API_URL = "https://governance.decentraland.org/api";
          GOVERNANCE_POLL_ENABLED = "true";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.presence {
      catalyrst-presence = mkSingle {
        description = "catalyrst-presence (user-count history, port 5152)";
        port = 5152;
        exec = "${presencePkg}/bin/catalyrst-presence run";
        afterExtra = [ "catalyrst-archipelago.service" ];
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5152";
          PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "presence";
          ARCHIPELAGO_URL = "http://127.0.0.1:5139";
          COMMS_URL = "http://127.0.0.1:5145";
          WORLDS_SERVER_URL = "http://127.0.0.1:5143";
        };
      };
    }
    // lib.optionalAttrs cfg.gateway.enable {
      catalyrst-opensea-resolver = mkSingle {
        description = "catalyrst-opensea-resolver (NFT metadata: squid + on-chain, port 5162)";
        port = 5162;
        exec = "${pkgs.nodejs_24}/bin/node ${./opensea-resolver.mjs}";
        environment = {
          PORT = "5162";
          DOMAIN = cfg.domain;
          PSQL = "${pkgs.postgresql_18}/bin/psql";
          PG_CONN = conn "marketplace_squid";
          RPC_MAINNET = "https://rpc.decentraland.org/mainnet";
          RPC_POLYGON = "https://rpc.decentraland.org/polygon";
        };
        # SSRF containment. systemd IP filters are bidirectional, so denying
        # loopback would also drop nginx's ingress to :5162 (listener up but
        # unreachable) -- localhost must be ALLOWED for the proxy to reach it.
        # The network layer still denies link-local (incl. the cloud metadata
        # endpoint) and RFC1918 egress; the resolver's own URL guard
        # (opensea-resolver.mjs) refuses loopback/private targets per hop, so
        # allowing localhost here does not re-open the SSRF. Postgres rides the
        # AF_UNIX socket, unaffected by IPAddress* rules.
        extraServiceConfig.IPAddressAllow = [ "localhost" ];
        extraServiceConfig.IPAddressDeny = [
          "link-local"
          "multicast"
          "10.0.0.0/8"
          "172.16.0.0/12"
          "192.168.0.0/16"
          "fc00::/7"
        ];
      };
    };
}
