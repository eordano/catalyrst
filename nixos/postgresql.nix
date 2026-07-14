{
  config,
  pkgs,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
in
lib.mkIf cfg.enable {
  services.postgresql = {
    enable = true;
    package = pkgs.postgresql_18;
    ensureDatabases = [
      "content"
      "marketplace_squid"
      "places"
      "places_events"
      "worlds"
      "builder"
      "camera_reel"
      "ab_registry"
      "communities"
      "comms"
      "notifications"
      "badges"
      "media"
      "price"
      "credits"
      "social_rpc"
      "signatures"
      "governance"
      "presence"
      "catalyrst"
    ];
    ensureUsers = [
      {
        name = "root";
        ensureClauses.superuser = true;
      }
      {
        name = "catalyrst";
        ensureClauses.login = true;
      }
      {
        name = "squid";
        ensureClauses.login = true;
      }
    ];
    authentication = lib.mkForce ''
      local all         all peer
      local replication all peer
    '';
    settings = {
      listen_addresses = lib.mkForce "";
      port = cfg.pgPort;
      unix_socket_permissions = "0770";
      shared_buffers = cfg.resources.pgSharedBuffers;
      effective_cache_size = cfg.resources.pgEffectiveCacheSize;
      work_mem = "32MB";
      maintenance_work_mem = "512MB";
      max_connections = 300;
      random_page_cost = 1.1;
      effective_io_concurrency = 200;
      wal_level = "minimal";
      max_wal_senders = 0;
      log_connections = true;
      log_disconnections = true;
      log_line_prefix = "%m [%p] %q%u@%d/%a ";
      log_min_duration_statement = 1000;
      log_checkpoints = true;
      log_lock_waits = true;
      log_temp_files = 0;
    };
  };

  systemd.services.postgresql-ownership = {
    description = "least-priv DB ownership + grants for catalyrst / squid";
    # postgresql-setup.service runs ensureDatabases/ensureUsers; ordering only
    # after postgresql.service races DB+role creation and this DDL fails.
    after = [
      "postgresql.service"
      "postgresql-setup.service"
      "squid-search-path.service"
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
    };
    script = ''
      set -e
      ${
        lib.optionalString (cfg.pgPort != 5432) "export PGPORT=${toString cfg.pgPort}\n      "
      }PSQL=${pkgs.postgresql_18}/bin/psql

      $PSQL -d postgres -c "ALTER ROLE catalyrst NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT 120;"
      $PSQL -d postgres -c "ALTER ROLE squid     NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT  60;"

      $PSQL -d postgres -c "ALTER DATABASE content OWNER TO catalyrst;"
      $PSQL -d content  -c "REASSIGN OWNED BY postgres TO catalyrst;" || true
      $PSQL -d content  -c "REASSIGN OWNED BY root     TO catalyrst;" || true
      # REASSIGN OWNED BY postgres above is a guaranteed no-op (PG refuses to
      # reassign from the bootstrap role wholesale; the || true swallows it),
      # and a DB restored as postgres -- or as any third role -- leaves its
      # objects out of catalyrst's ownership. The migrate service then fails
      # its unguarded "ALTER SEQUENCE ... OWNED BY" ("must be owner of
      # sequence deployments_id_seq" on a DB whose objects were restored under
      # a non-catalyrst role), because ALTER needs
      # ownership, not grants. Sweep every relation individually instead:
      # per-object ALTER ... OWNER TO is allowed to a superuser regardless of
      # the current owner.
      $PSQL -v ON_ERROR_STOP=1 -d content <<'EOSQL'
      DO $$
      DECLARE r record;
      BEGIN
        -- A sequence linked to a table column (serial / OWNED BY) cannot be
        -- re-owned directly ("cannot change owner of sequence ... is linked
        -- to table"); it follows its table's owner, so skip those and let
        -- the table ALTER carry them.
        FOR r IN
          SELECT format('ALTER %s public.%I OWNER TO catalyrst',
                        CASE c.relkind
                          WHEN 'S' THEN 'SEQUENCE'
                          WHEN 'v' THEN 'VIEW'
                          WHEN 'm' THEN 'MATERIALIZED VIEW'
                          ELSE 'TABLE'
                        END,
                        c.relname) AS stmt
          FROM pg_class c
          WHERE c.relnamespace = 'public'::regnamespace
            AND c.relkind IN ('r', 'p', 'S', 'v', 'm')
            AND c.relowner <> 'catalyrst'::regrole
            AND NOT (c.relkind = 'S' AND EXISTS (
                  SELECT 1 FROM pg_depend d
                  WHERE d.classid    = 'pg_class'::regclass
                    AND d.objid      = c.oid
                    AND d.refclassid = 'pg_class'::regclass
                    AND d.deptype    IN ('a', 'i')))
        LOOP
          EXECUTE r.stmt;
        END LOOP;
      END $$;
      EOSQL
      $PSQL -d content  -c "GRANT ALL ON SCHEMA public TO catalyrst;"
      $PSQL -d content  -c "GRANT ALL ON ALL TABLES    IN SCHEMA public TO catalyrst;"
      $PSQL -d content  -c "GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO catalyrst;"
      $PSQL -d content  -c "ALTER DEFAULT PRIVILEGES FOR ROLE catalyrst IN SCHEMA public GRANT ALL ON TABLES    TO catalyrst;"
      $PSQL -d content  -c "ALTER DEFAULT PRIVILEGES FOR ROLE catalyrst IN SCHEMA public GRANT ALL ON SEQUENCES TO catalyrst;"

      $PSQL -d postgres -c "ALTER DATABASE marketplace_squid OWNER TO squid;"
      $PSQL -d marketplace_squid -c "REASSIGN OWNED BY postgres TO squid;" || true
      $PSQL -d marketplace_squid -c "REASSIGN OWNED BY root     TO squid;" || true
      $PSQL -d marketplace_squid -c "GRANT CONNECT ON DATABASE marketplace_squid TO catalyrst;"

      # The squid_marketplace schema only exists once the squid indexer has run
      # (needs squid.env + on-box code). Skip its grants until then so a host
      # without squid provisioned doesn't fail the whole pass.
      if [ "$($PSQL -d marketplace_squid -tAc "select exists (select 1 from information_schema.schemata where schema_name = 'squid_marketplace')")" = t ]; then
        $PSQL -d marketplace_squid -c "GRANT ALL ON SCHEMA squid_marketplace TO squid;"
        $PSQL -d marketplace_squid -c "GRANT ALL ON ALL TABLES    IN SCHEMA squid_marketplace TO squid;"
        $PSQL -d marketplace_squid -c "GRANT ALL ON ALL SEQUENCES IN SCHEMA squid_marketplace TO squid;"
        $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT ALL ON TABLES    TO squid;"
        $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT ALL ON SEQUENCES TO squid;"
        $PSQL -d marketplace_squid -c "GRANT USAGE  ON SCHEMA squid_marketplace TO catalyrst;"
        $PSQL -d marketplace_squid -c "GRANT SELECT ON ALL TABLES IN SCHEMA squid_marketplace TO catalyrst;"
        $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT SELECT ON TABLES TO catalyrst;"
      fi

      $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE content           FROM PUBLIC;"
      $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE marketplace_squid FROM PUBLIC;"

    '';
  };

  systemd.services.postgresql-bundles = {
    description = "DB ownership + grants for the catalyrst v3 bundles";
    after = [
      "postgresql.service"
      "postgresql-setup.service"
      "postgresql-ownership.service"
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
    };
    script = ''
      set -e
      ${
        lib.optionalString (cfg.pgPort != 5432) "export PGPORT=${toString cfg.pgPort}\n      "
      }PSQL=${pkgs.postgresql_18}/bin/psql

      for db in places places_events worlds builder camera_reel ab_registry \
                communities comms notifications badges media \
                price credits social_rpc signatures governance presence catalyrst; do
        $PSQL -d postgres -c "ALTER DATABASE $db OWNER TO catalyrst;"
        $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE $db FROM PUBLIC;"
        $PSQL -d "$db" -c "REASSIGN OWNED BY postgres TO catalyrst;" || true
        $PSQL -d "$db" -c "GRANT ALL ON SCHEMA public TO catalyrst;"
      done

      # catalyrst-telemetry writes into the telemetry schema of the catalyrst DB.
      $PSQL -d catalyrst -c "CREATE SCHEMA IF NOT EXISTS telemetry AUTHORIZATION catalyrst;"

      # market/economy (marketplace-server views) + favorites live as extra
      # schemas inside the existing marketplace_squid DB, alongside the squid
      # indexer's squid_marketplace schema. catalyrst owns them; the squid role
      # keeps squid_marketplace.
      $PSQL -d marketplace_squid -c "GRANT CREATE ON DATABASE marketplace_squid TO catalyrst;"
      $PSQL -d marketplace_squid -c "CREATE SCHEMA IF NOT EXISTS marketplace AUTHORIZATION catalyrst;"
      $PSQL -d marketplace_squid -c "CREATE SCHEMA IF NOT EXISTS favorites   AUTHORIZATION catalyrst;"
      # land_authz stays table-less until catalyrst-land-authz-index runs its
      # own migrations: land_operators.rs gates on to_regclass('land_authz.
      # token_right'), so a bare schema keeps the external-subgraph fallback
      # serving while the indexer is absent or still backfilling.
      $PSQL -d marketplace_squid -c "CREATE SCHEMA IF NOT EXISTS land_authz  AUTHORIZATION catalyrst;"
      # catalyrst-market's sqlx migration bookkeeping lands in `public`; PG15+
      # no longer grants CREATE there by default.
      $PSQL -d marketplace_squid -c "GRANT ALL ON SCHEMA public TO catalyrst;"
    '';
  };
}
