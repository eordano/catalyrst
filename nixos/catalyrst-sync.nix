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
  contentPkg = if cfg.contentPackage != null then cfg.contentPackage else commsPackages.catalyrst;
  # Tied to inputs.catalyrst (the source the binaries come from), never to the
  # consumer flake's own rev: COMMIT_HASH sits in the generated unit text, so a
  # whole-repo rev would make restartIfChanged bounce this service on every
  # unrelated commit the consumer applies (a ~9-minute content re-bootstrap).
  commitHash = inputs.catalyrst.shortRev or inputs.catalyrst.dirtyShortRev or "dirty";
  commsVersion = "${pkgs.nodejs_24.version}+pulse-${inputs.catalyrst.shortRev or "unknown"}";
  commsCommitHash = "${inputs.catalyrst.inputs.archipelago.shortRev or "unknown"}+${
    inputs.catalyrst.shortRev or "unknown"
  }";

  inherit (import ./sandbox.nix)
    baseSandbox
    rootOneshotSandbox
    ;
in
lib.mkIf cfg.enable {
  users.users.catalyrst = {
    isSystemUser = true;
    group = "catalyrst";
  };
  users.groups.catalyrst = { };
  users.users.catalyrst.extraGroups = [ "postgres" ];

  systemd.services.catalyrst-sync = {
    description = "catalyrst (content + lambdas + sync)";
    after = [
      "postgresql.service"
      "postgresql-setup.service"
      "catalyrst-content-migrate.service"
      "catalyrst-admin-secret.service"
      "network-online.target"
    ];
    wants = [
      "network-online.target"
      "postgresql-setup.service"
      "catalyrst-admin-secret.service"
    ];
    wantedBy = [ "multi-user.target" ];
    unitConfig.RequiresMountsFor = [ cfg.stateDir ];
    serviceConfig = baseSandbox // {
      ExecStart = pkgs.writeShellScript "catalyrst-live-launcher" ''
        set -a
        . "$CREDENTIALS_DIRECTORY/admin-env"
        set +a
        ${lib.optionalString (cfg.telemetryAdminTokenFile != null) ''
          export CATALYRST_TELEMETRY_ADMIN_TOKEN="$(cat "$CREDENTIALS_DIRECTORY/telemetry-admin-token")"
        ''}
        exec ${contentPkg}/bin/catalyrst-live
      '';
      LoadCredential = [
        "admin-env:${cfg.secretsDir}/catalyrst-admin.env"
      ]
      ++ lib.optional (
        cfg.telemetryAdminTokenFile != null
      ) "telemetry-admin-token:${cfg.telemetryAdminTokenFile}";
      UMask = "0022";
      Restart = "on-failure";
      RestartSec = 5;
      LimitNOFILE = 1048576;
      User = "catalyrst";
      Group = "catalyrst";
      ProtectHome = true;
      ReadWritePaths = [
        cfg.stateDir
        "/run/postgresql"
      ];
      MemoryHigh = cfg.resources.syncMemoryHigh;
      MemoryMax = cfg.resources.syncMemoryMax;
      TasksMax = 4096;
      SocketBindAllow = [ "tcp:5141" ];
      SocketBindDeny = "any";
    };
    environment = {
      RUST_LOG = "info";
      COMMIT_HASH = commitHash;
      HTTP_SERVER_HOST = "127.0.0.1";
      CATALYRST_PORT = "5141";
      PUBLIC_URL = d.publicUrl;
      COMMS_PROTOCOL = "v3";
      COMMS_FIXED_ADAPTER = "archipelago:archipelago:${d.wsScheme}://${cfg.domain}/ws";
      COMMS_VERSION = commsVersion;
      COMMS_COMMIT_HASH = commsCommitHash;
      COMMS_WS_CONNECTOR_URL = "http://127.0.0.1:5139";
      COMMS_STATS_URL = "http://127.0.0.1:5139";
      REALM_NAME = cfg.realm;

      ADMIN_ADDRESSES = lib.concatStringsSep "," cfg.adminAddresses;
      ADMIN_SESSION_TTL_SECS = "43200";
      PROFILE_CDN_BASE_URL = "https://profile-images.decentraland.org";
      MAP_SATELLITE_BASE_URL = "https://genesis.city/map/latest";
      MAP_PARCEL_VIEW_URL = "https://api.decentraland.org/v1/minimap.png";
      LAND_IMAGE_BASE_URL = "https://api.decentraland.org";
      CATALYRST_SERVICE_URLS = "content=http://127.0.0.1:5141,explore=http://127.0.0.1:5143,create=http://127.0.0.1:5144,social=http://127.0.0.1:5145,data=http://127.0.0.1:5146,ab-cdn=http://127.0.0.1:5147,social-rpc=http://127.0.0.1:5148,explorer-api=http://127.0.0.1:5137";
      STORAGE_X_ACCEL_BASE = "/__protected_storage";

      SQUID_DB_NAME = "marketplace_squid";
      SQUID_DB_USER = "catalyrst";
      POSTGRES_HOST = "/run/postgresql";
      POSTGRES_PORT = toString cfg.pgPort;
      POSTGRES_CONTENT_USER = "catalyrst";
      POSTGRES_CONTENT_PASSWORD = "x";
      POSTGRES_CONTENT_DB = "content";
      SYNC_DB_NAME = "content";
      STORAGE_ROOT_FOLDER = "${cfg.stateDir}/content_rust";
      SYNC_STORAGE_ROOT = "${cfg.stateDir}/content_rust";
      SYNC_ENABLED = if cfg.sync.enable then "true" else "false";
      ENABLE_DEPLOYMENTS = "true";
      THIRD_PARTY_ROOT_SOURCE = "squid";
      IGNORE_BLOCKCHAIN_ACCESS_CHECKS = "false";
      ETH_RPC_URL = cfg.ethRpcUrl;
      RPC_ENDPOINT_ETH = cfg.ethRpcUrl;
      RPC_ENDPOINT_POLYGON = "https://rpc.decentraland.org/polygon";

      ETH_COLLECTIONS_SUBGRAPH_URL = "https://subgraph.decentraland.org/collections-ethereum-mainnet";
      MATIC_COLLECTIONS_SUBGRAPH_URL = "https://subgraph.decentraland.org/collections-matic-mainnet";
      THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL = "https://subgraph.decentraland.org/tpr-matic-mainnet";
      LAND_SUBGRAPH_URL = "https://subgraph.decentraland.org/land-manager";
      CONCURRENT_SYNC_DOWNLOADS = toString cfg.sync.concurrency;
      SYNC_SOURCE = lib.concatStringsSep "," cfg.sync.sources;
    };
  };

  systemd.services.catalyrst-admin-secret = {
    description = "Generate the catalyrst admin-console SESSION_SECRET";
    wantedBy = [ "multi-user.target" ];
    before = [ "catalyrst-sync.service" ];
    serviceConfig = rootOneshotSandbox // {
      Type = "oneshot";
      RemainAfterExit = true;
      User = "root";
      ReadWritePaths = [ cfg.secretsDir ];
    };
    script = ''
      set -euo pipefail
      umask 077
      ENV=${cfg.secretsDir}/catalyrst-admin.env
      if [ ! -s "$ENV" ]; then
        printf 'SESSION_SECRET=%s\n' "$(${pkgs.openssl}/bin/openssl rand -base64 48 | tr -d '\n')" > "$ENV"
        chmod 600 "$ENV"
      fi
    '';
  };

  systemd.tmpfiles.rules = [
    "d ${cfg.secretsDir}           0700 root root -"
    "d ${cfg.stateDir}              0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/content      0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/content_rust 0755 catalyrst catalyrst -"
  ];
}
