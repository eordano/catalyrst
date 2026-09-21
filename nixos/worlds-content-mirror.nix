{
  config,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  d = import ./helpers.nix cfg;
  mirror = cfg.worldsContentMirror;
  inherit (import ./sandbox.nix) baseSandbox;
  bundles =
    if cfg.bundlesPackage != null then
      cfg.bundlesPackage
    else
      inputs.catalyrst.packages.x86_64-linux.catalyrst-all;
  mirrorBin = "${bundles}/bin/worlds-mirror";
in
{
  options.services.catalyrst.worldsContentMirror = {
    enable = lib.mkEnableOption "the scheduled copy of upstream world scenes and files" // {
      description = ''
        Run worlds-mirror on a timer, copying every world the upstream worlds
        server lists (scene entities plus their files) into this node's
        worlds store, so a world picked from the catalogue opens from this
        node. upstream.mirrorWorlds only lists the public worlds; without
        this copy a listed world answers 404 here. Default off: the full
        public set is tens of gigabytes. A world published to this node is
        never overwritten, and files already on disk are not fetched again.
      '';
    };
    upstreamUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://worlds-content-server.decentraland.org";
      description = "Worlds server the scenes and files are copied from.";
    };
    jobs = lib.mkOption {
      type = lib.types.ints.positive;
      default = 8;
      description = "Worlds copied at the same time.";
    };
    interval = lib.mkOption {
      type = lib.types.str;
      default = "6h";
      description = "Pause between the end of one run and the start of the next (systemd time span).";
    };
  };

  config = lib.mkIf (cfg.enable && cfg.subServices.explore && mirror.enable) {
    systemd.services.catalyrst-worlds-content-mirror = {
      description = "Copy upstream world scenes and files into this node's worlds store";
      after = [
        "postgresql.service"
        "postgresql-bundles.service"
        "network-online.target"
      ];
      wants = [ "network-online.target" ];
      requires = [ "postgresql.service" ];
      unitConfig.RequiresMountsFor = [ cfg.stateDir ];
      environment = {
        RUST_LOG = "info";
        NETWORK_ID = "1";
        WORLDS_PG_CONNECTION_STRING = "postgresql:///worlds?host=/run/postgresql&user=catalyrst${d.pgPortQuery}";
        WORLDS_CONTENT_DIR = "${cfg.stateDir}/worlds/contents";
        CONTENT_PUBLIC_URL = "${d.publicUrl}/content";
        LAMBDAS_PUBLIC_URL = "${d.publicUrl}/lambdas";
        HTTP_BASE_URL = d.publicUrl;
        CONTENTS_UPSTREAM_URL = mirror.upstreamUrl;
        LIVEKIT_ALLOW_DEV_CREDS = "1";
      };
      serviceConfig = baseSandbox // {
        Type = "oneshot";
        User = "catalyrst";
        Group = "catalyrst";
        ProtectHome = true;
        ReadWritePaths = [
          cfg.stateDir
          "/run/postgresql"
        ];
        TimeoutStartSec = "infinity";
        Nice = 10;
        IOSchedulingClass = "idle";
        MemoryMax = cfg.resources.bundleMemoryMax;
        SocketBindDeny = "any";
      };
      script = ''
        set -euo pipefail
        if [ ! -x "${mirrorBin}" ]; then
          echo "this catalyrst package does not ship worlds-mirror; skipping"
          exit 0
        fi
        exec "${mirrorBin}" -j ${toString mirror.jobs}
      '';
    };

    systemd.timers.catalyrst-worlds-content-mirror = {
      description = "Periodic copy of upstream world scenes and files";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnBootSec = "10m";
        OnUnitInactiveSec = mirror.interval;
        RandomizedDelaySec = "5m";
      };
    };
  };
}
