{
  config,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  commsPackages = inputs.catalyrst.packages.x86_64-linux;
  indexBin = "${commsPackages.catalyrst-all}/bin/catalyrst-land-authz-index";
in
{
  options.services.catalyrst.landAuthzIndex = {
    enable = lib.mkEnableOption "the on-chain LAND rights indexer (land_authz schema in marketplace_squid)" // {
      description = ''
        Run catalyrst-land-authz-index on a timer, indexing LAND
        Approval/UpdateOperator/Transfer events into the land_authz schema of
        marketplace_squid. Default off, deliberately: the moment the
        indexer's own migrations create land_authz.token_right,
        land_operators.rs answers the LAND lambdas from this index instead
        of the external subgraph -- including while the first backfill is
        still running, when the local answer is incomplete. Enable once the
        deployed catalyrst tag ships the binary and a full first sync can be
        allowed to finish (the unit skips cleanly while the binary is
        absent).
      '';
    };
  };

  config = lib.mkIf (cfg.enable && cfg.landAuthzIndex.enable) {
    systemd.services.catalyrst-land-authz-index = {
      description = "Index LAND Approval/UpdateOperator/Transfer events into land_authz";
      after = [
        "postgresql.service"
        "postgresql-bundles.service"
      ];
      requires = [ "postgresql.service" ];
      serviceConfig = {
        Type = "oneshot";
        User = "catalyrst";
        Group = "catalyrst";
        TimeoutStartSec = "infinity";
      };
      environment = {
        RUST_LOG = "info";
        LAND_AUTHZ_PG_CONNECTION_STRING = "postgres:///marketplace_squid?host=/run/postgresql&port=${toString cfg.pgPort}&user=catalyrst";
        LAND_AUTHZ_RPC_URL = cfg.ethRpcUrl;
      };
      script = ''
        set -euo pipefail
        # v0.16.1 predates the indexer binary; skip cleanly until the
        # catalyrst input is bumped to a tag that ships it.
        if [ ! -x "${indexBin}" ]; then
          echo "catalyrst-all does not ship catalyrst-land-authz-index yet; skipping"
          exit 0
        fi
        exec "${indexBin}"
      '';
    };

    systemd.timers.catalyrst-land-authz-index = {
      description = "Periodic incremental LAND rights sync";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnBootSec = "5m";
        OnUnitInactiveSec = "15m";
        RandomizedDelaySec = "1m";
      };
    };
  };
}
