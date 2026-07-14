{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  facts = import ./facts.nix;

  inherit (import ./sandbox.nix)
    commsHardening
    ;

  squidPkg =
    if cfg.squidPackage != null then
      cfg.squidPackage
    else
      inputs.catalyrst.packages.${pkgs.stdenv.hostPlatform.system}.squid;

  squidRpcEgress = { };

  mkSquidService =
    {
      description,
      exec,
      socketBindAllow,
      oneshot ? false,
      extraEnvironment ? { },
      requireEnv ? [ ],
    }:
    {
      inherit description;
      after = [
        "postgresql.service"
        "squid-search-path.service"
        "network-online.target"
      ]
      ++ lib.optional (!oneshot) "squid-migrate.service";
      wants = [
        "network-online.target"
      ];
      requires = [
        "squid-search-path.service"
      ]
      ++ lib.optional (!oneshot) "squid-migrate.service";
      wantedBy = [ "multi-user.target" ];
      environment = extraEnvironment;
      serviceConfig =
        commsHardening
        // {
          User = "squid";
          Group = "squid";
          WorkingDirectory = "/var/lib/squid";
          LoadCredential = "squid.env:${cfg.secretsDir}/squid.env";
          ExecStart = pkgs.writeShellScript "squid-launcher" ''
            set -a
            . "$CREDENTIALS_DIRECTORY/squid.env"
            set +a
            ${lib.concatMapStringsSep "\n" (v: ''
              if [ -z "''${${v}:-}" ]; then
                echo "${v} is not set in squid.env; add it there and this service starts by itself on the next restart cycle"
                exit 0
              fi
            '') requireEnv}
            exec ${exec}
          '';
          MemoryHigh = cfg.resources.squidMemoryHigh;
          MemoryMax = cfg.resources.squidMemoryMax;
          TasksMax = 512;
          SocketBindAllow = socketBindAllow;
          SocketBindDeny = "any";
        }
        // (
          if oneshot then
            {
              Type = "oneshot";
              RemainAfterExit = true;
            }
          else
            {
              Restart = "always";
              RestartSec = 30;
            }
        );
    };
in
lib.mkIf (cfg.enable && cfg.subServices.squid) {
  users.users.squid = {
    isSystemUser = true;
    group = "squid";
    home = "/var/lib/squid";
  };
  users.groups.squid = { };
  users.users.squid.extraGroups = [ "postgres" ];

  # ordered after postgresql-setup so marketplace_squid + the squid role exist
  # before the search-path DDL runs (see postgresql.nix).
  systemd.services.squid-search-path = {
    description = "ensure squid processor search_path is set";
    after = [
      "postgresql.service"
      "postgresql-setup.service"
    ];
    wants = [
      "postgresql.service"
      "postgresql-setup.service"
    ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      User = "postgres";
      ExecStart = pkgs.writeShellScript "squid-search-path-fix" ''
        set -euo pipefail

        portArg="${lib.optionalString (cfg.pgPort != 5432) "-p ${toString cfg.pgPort}"}"

        # After= orders starts, not readiness: postgres accepts connections some
        # time after systemd considers it started. Without waiting, every
        # statement below fails, this unit still reports success, and the
        # processors crash-loop against a schema that was never created.
        for _ in $(seq 1 60); do
          if ${pkgs.postgresql_18}/bin/pg_isready $portArg -q; then
            break
          fi
          sleep 1
        done
        ${pkgs.postgresql_18}/bin/pg_isready $portArg -q

        psql() {
          ${pkgs.postgresql_18}/bin/psql $portArg -d marketplace_squid -c "$1"
        }

        # Best effort: the role may not exist on every host.
        psql "ALTER ROLE squid IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;" || true
        psql "ALTER ROLE root  IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;" || true

        # Required: the processors cannot run without these, so fail loudly
        # here rather than let them restart forever.
        psql "CREATE SCHEMA IF NOT EXISTS squid_marketplace AUTHORIZATION squid;"
        psql "CREATE TABLE IF NOT EXISTS public.squids (name text PRIMARY KEY, last_notified bigint);"
        psql "ALTER TABLE public.squids OWNER TO squid;"
        psql "INSERT INTO public.squids (name) VALUES ('marketplace') ON CONFLICT (name) DO NOTHING;"
      '';
    };
  };

  # One-shot TypeORM migration apply; both processors hard-require it so a
  # fresh host can't race the schema.
  systemd.services.squid-migrate = mkSquidService {
    description = "marketplace-squid schema migrations";
    exec = "${squidPkg}/bin/squid-migrate";
    socketBindAllow = [ ];
    oneshot = true;
  };

  # The bin wrappers bake node flags and --chdir into the package share dir
  # (assets/abi resolve relative to it); WorkingDirectory above is only the
  # pre-exec cwd.
  # Module-owned env the processors would otherwise want from squid.env:
  #  - the Prometheus port, pinned to the SocketBindAllow value (they default
  #    it to 0.0.0.0:3000, which the sandbox denies -> EPERM crash loop);
  #  - the chain IDs, fixed constants of the networks this module targets
  #    (mainnet 1 / matic 137). The source already defaults them; setting them
  #    keeps the operator's squid.env to just RPC + DB. Both processors read
  #    both ids (cross-network metadata lookups).
  systemd.services.squid-eth = lib.recursiveUpdate (mkSquidService {
    description = "marketplace-squid eth processor";
    exec = "${squidPkg}/bin/squid-eth";
    socketBindAllow = [ "tcp:${toString facts.units.squid-eth.port}" ];
    extraEnvironment = {
      ETHEREUM_CHAIN_ID = "1";
      POLYGON_CHAIN_ID = "137";
      ETH_PROMETHEUS_PORT = toString facts.units.squid-eth.port;
    };
  }) { serviceConfig = squidRpcEgress; };
  # The public SQD portal caps queries at 256 KiB and the polygon collection
  # filter exceeds that, so the polygon processor only works against the
  # authenticated portal. Gate on the key instead of letting the processor
  # crash-loop with the full filter in every error dump.
  systemd.services.squid-polygon = lib.recursiveUpdate (mkSquidService {
    description = "marketplace-squid polygon processor";
    exec = "${squidPkg}/bin/squid-polygon";
    socketBindAllow = [ "tcp:${toString facts.units.squid-polygon.port}" ];
    requireEnv = [ "SQD_PORTAL_API_KEY" ];
    extraEnvironment = {
      ETHEREUM_CHAIN_ID = "1";
      POLYGON_CHAIN_ID = "137";
      POLYGON_PROMETHEUS_PORT = toString facts.units.squid-polygon.port;
    };
  }) { serviceConfig = squidRpcEgress; };

  systemd.tmpfiles.rules = [
    "d /var/lib/squid              0755 squid     squid     -"
  ];
}
