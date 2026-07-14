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
  # options null — matching bundles.nix and squid.nix, so a profile that
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
      catalyrst-profile-images = mkSingle {
        description = "catalyrst-profile-images (profile picture proxy + cache, port 5161)";
        port = 5161;
        exec = "${bundlesPkg}/bin/catalyrst-profile-images";
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5161";
          PROFILE_IMAGES_ORIGIN_URL = "https://profile-images.decentraland.org";
          PROFILE_IMAGES_CACHE_DIR = "${cfg.stateDir}/profile-images";
        };
        extraServiceConfig = {
          ReadWritePaths = [ "${cfg.stateDir}/profile-images" ];
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
        # unreachable) — localhost must be ALLOWED for the proxy to reach it.
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
