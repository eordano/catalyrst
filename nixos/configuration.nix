{ config, lib, pkgs, ... }:
let
  cfg = config.services.catalyrst;

  landingRoot = pkgs.runCommand "catalyrst-landing" { } ''
    mkdir -p "$out"
    cp ${./landing/index.html} "$out/index.html"
  '';

  baseSandbox = {
    NoNewPrivileges = true;
    ProtectSystem = "strict";
    PrivateTmp = true;
    PrivateDevices = true;
    KeyringMode = "private";
    # PrivateUsers OMITTED: postgres SO_PEERCRED can't see UIDs from a child
    # userns, so catalyrst-sync + squid-* would fail peer auth. Added back on
    # non-postgres services via `noPgSandbox` below.
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectKernelLogs = true;
    ProtectControlGroups = true;
    ProtectClock = true;
    ProtectHostname = true;
    RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" "AF_NETLINK" ];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    LockPersonality = true;
    ProtectProc = "invisible";
    ProcSubset = "pid";
    CapabilityBoundingSet = "";
    AmbientCapabilities = "";
    SystemCallArchitectures = "native";
    # ~@resources dropped: .NET 10 (pulse) needs mbind/set_mempolicy/sched_setattr.
    SystemCallFilter = [ "@system-service" "~@privileged" ];
    UMask = "0077";
    DevicePolicy = "closed";
    # RestrictFileSystems disabled: needs BPF LSM hook, NixOS 25.11 kernel
    # doesn't enable it (services exit 244). Revisit when nixpkgs ships bpf-lsm.
  };

  commsHardening = baseSandbox // { ProtectHome = true; };
  noPgSandbox = commsHardening // { PrivateUsers = true; };
  # MDWE excludes pulse (.NET RyuJIT) and archipelago-* (V8) — both SIGTRAP.
  noJitHardening = noPgSandbox // { MemoryDenyWriteExecute = true; };

  # No IP-level egress allowlist for squid-eth/polygon: operators may switch
  # RPC providers, brittle to pin.
  squidRpcEgress = {};

  mkSquidService = { description, exec, socketBindAllow, extraEnvironment ? {} }: {
    inherit description;
    after = [ "postgresql.service" "squid-search-path.service" "network-online.target" ];
    wants = [ "network-online.target" "squid-search-path.service" ];
    wantedBy = [ "multi-user.target" ];
    environment = extraEnvironment;
    serviceConfig = commsHardening // {
      User = "squid";
      Group = "squid";
      WorkingDirectory = "/var/lib/squid";
      LoadCredential = "squid.env:/var/lib/secrets/squid.env";
      ExecStart = pkgs.writeShellScript "squid-launcher" ''
        set -a
        . "$CREDENTIALS_DIRECTORY/squid.env"
        set +a
        exec ${exec}
      '';
      Restart = "always";
      RestartSec = 30;
      MemoryHigh = "4G";
      MemoryMax = "5G";
      TasksMax = 512;
      SocketBindAllow = socketBindAllow;
      SocketBindDeny = "any";
    };
  };

  commsEnabled = cfg.enableComms && cfg.commsPackages != null;
in
{
  options.services.catalyrst = {
    enable = lib.mkEnableOption "the catalyrst Decentraland catalyst (content + lambdas + sync)";

    domain = lib.mkOption {
      type = lib.types.str;
      default = "example.com";
      description = "Public DNS domain for this realm. Drives ACME certs, nginx vhosts, and PUBLIC_URL.";
    };

    acmeEmail = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Email passed to Let's Encrypt via security.acme.defaults.email. If null,
        the operator must set security.acme.defaults.email themselves.
      '';
    };

    realmName = lib.mkOption {
      type = lib.types.str;
      default = "your-realm-name";
      description = "REALM_NAME exposed by /about. Cosmetic, but must be unique in your realm directory.";
    };

    publicUrl = lib.mkOption {
      type = lib.types.str;
      default = "https://${cfg.domain}";
      defaultText = lib.literalExpression ''"https://''${cfg.domain}"'';
      description = "PUBLIC_URL the realm advertises. Defaults to https://<domain>.";
    };

    package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = ''
        The catalyrst Rust server package (provides /bin/catalyrst-live).
        Required when services.catalyrst.enable = true. Typically set to
        inputs.catalyrst.packages.''${pkgs.system}.catalyrst.
      '';
    };

    commsPackages = lib.mkOption {
      type = lib.types.nullOr (lib.types.attrsOf lib.types.package);
      default = null;
      description = ''
        Optional attrset of comms-related packages: { archipelago-workers, pulse, catalyrst }.
        When enableComms = true, must provide archipelago-workers and pulse.
        Typically set to inputs.catalyrst.packages.''${pkgs.system}.
      '';
    };

    commitHash = lib.mkOption {
      type = lib.types.str;
      default = "unknown";
      description = "Git commit hash of the catalyrst build (surfaced via /about as COMMIT_HASH).";
    };

    commsVersion = lib.mkOption {
      type = lib.types.str;
      default = "unknown";
      description = "Version string for the comms stack (archipelago + pulse).";
    };

    commsCommitHash = lib.mkOption {
      type = lib.types.str;
      default = "unknown";
      description = "Commit identifier for the comms stack (archipelago + pulse).";
    };

    contentStorageRoot = lib.mkOption {
      type = lib.types.str;
      default = "/srv/catalyrst/content";
      description = "STORAGE_ROOT_FOLDER — legacy content-server layout root.";
    };

    syncStorageRoot = lib.mkOption {
      type = lib.types.str;
      default = "/srv/catalyrst/content_rust";
      description = "SYNC_STORAGE_ROOT — rust-side sync output root.";
    };

    syncEnabled = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "If true, catalyrst pulls deployments from peers in syncSource.";
    };

    enableDeployments = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        If true, accept deployments on this catalyst (write path). Default is
        false — most operators want a read-only mirror.
      '';
    };

    syncSource = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [
        "https://peer.decentraland.org/content"
        "https://peer-eu1.decentraland.org/content"
        "https://peer.dclnodes.io/content"
        "https://peer.uadevops.com/content"
        "https://peer.melonwave.com/content"
      ];
      description = "Peer URLs catalyrst pulls deployments from. Joined with ',' into SYNC_SOURCE.";
    };

    cloudflareFronted = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        If true, nginx 80/443 only accepts traffic from published Cloudflare
        IP ranges, and CF real-IP / CF-Connecting-IP is honored. Disable if
        you're not fronting with Cloudflare.
      '';
    };

    enableComms = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        If true, run the comms stack alongside the content server:
        archipelago-{core,ws-connector,stats}, NATS, LiveKit SFU, Pulse.
        Requires commsPackages with archipelago-workers + pulse.
      '';
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      assertions = [
        {
          assertion = cfg.package != null;
          message = "services.catalyrst.enable = true requires services.catalyrst.package to be set.";
        }
        {
          assertion = !cfg.enableComms || (cfg.commsPackages != null
            && cfg.commsPackages ? archipelago-workers
            && cfg.commsPackages ? pulse);
          message = ''
            services.catalyrst.enableComms = true requires services.catalyrst.commsPackages
            to provide both `archipelago-workers` and `pulse` packages.
          '';
        }
      ];

      services.logrotate.checkConfig = false;
      boot.tmp.cleanOnBoot = true;
      zramSwap.enable = true;
      networking.domain = lib.mkDefault "";

      # PerSourcePenalties off: avoids locking out trusted automation sources.
      services.openssh = {
        enable = lib.mkDefault true;
        settings = {
          PasswordAuthentication = lib.mkDefault false;
          KbdInteractiveAuthentication = lib.mkDefault false;
          PermitRootLogin = lib.mkDefault "prohibit-password";
          X11Forwarding = lib.mkDefault false;
          PerSourcePenalties = lib.mkDefault "no";
        };
      };
      security.pam.loginLimits = [
        { domain = "*"; type = "soft"; item = "nofile"; value = "1048576"; }
        { domain = "*"; type = "hard"; item = "nofile"; value = "1048576"; }
      ];

      networking.nftables.enable = true;
      networking.firewall = {
        enable = lib.mkDefault true;
        allowedTCPPorts = [ 22 ] ++ lib.optionals cfg.enableComms [ 7881 ];
        allowedUDPPorts = lib.optionals cfg.enableComms [ 7777 7882 ];
      };
      services.fail2ban = { enable = lib.mkDefault true; maxretry = 8; bantime = "1h"; banaction = "nftables-multiport"; };

      services.nginx = {
        enable = true;
        recommendedTlsSettings = true;
        recommendedProxySettings = true;
        recommendedOptimisation = true;
        recommendedGzipSettings = true;
        serverTokens = false;
        commonHttpConfig = ''
          ${lib.optionalString cfg.cloudflareFronted ''
            include /var/lib/cloudflare/nginx-real-ip.conf;
            real_ip_header CF-Connecting-IP;
            real_ip_recursive on;
          ''}
          limit_req_zone  $binary_remote_addr zone=catread:10m   rate=30r/s;
          limit_req_zone  $binary_remote_addr zone=catdeploy:10m rate=2r/s;
          limit_conn_zone $binary_remote_addr zone=catws:10m;
          limit_req_status 429;
          limit_conn_status 429;
        '';
        virtualHosts.${cfg.domain} = {
          serverAliases = [ "www.${cfg.domain}" ];
          forceSSL = true;
          useACMEHost = cfg.domain;
          extraConfig = ''
            add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
            add_header X-Frame-Options "SAMEORIGIN" always;
            add_header X-Content-Type-Options "nosniff" always;
            add_header Referrer-Policy "strict-origin-when-cross-origin" always;
            add_header Permissions-Policy "interest-cohort=()" always;
            add_header Content-Security-Policy "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'self'; base-uri 'self'" always;
            add_header Cross-Origin-Opener-Policy "same-origin" always;
            add_header Cross-Origin-Resource-Policy "same-origin" always;
            client_max_body_size 1m;
            limit_req zone=catread burst=60 nodelay;
          '';
          locations."= /" = { root = "${landingRoot}"; extraConfig = "try_files /index.html =404;"; };
          # Block leakage of internal endpoints via the catch-all proxy.
          locations."= /metrics" = { extraConfig = "return 404;"; };
          locations."= /admin"   = { extraConfig = "return 404;"; };
          locations."= /debug"   = { extraConfig = "return 404;"; };
          locations."/ws" = lib.mkIf cfg.enableComms {
            proxyPass = "http://127.0.0.1:5001";
            proxyWebsockets = true;
            extraConfig = ''
              proxy_read_timeout 3600s;
              limit_conn catws 8;
            '';
          };
          locations."= /content/entities" = {
            proxyPass = "http://127.0.0.1:5141";
            extraConfig = ''
              proxy_read_timeout 600s;
              proxy_buffering off;
              client_max_body_size 200m;
              client_body_timeout 300s;
              limit_req zone=catdeploy burst=4 nodelay;
            '';
          };
          locations."/" = {
            proxyPass = "http://127.0.0.1:5141";
            extraConfig = ''
              proxy_read_timeout 600s;
              proxy_buffering off;
            '';
          };
          # PERF-B: internal target for X-Accel-Redirect zero-copy on
          # /content/contents/{hash} (+ entity thumbnail/image). `internal;`
          # blocks direct external access — only nginx-internal redirects
          # from catalyrst-live's response header can reach it.
          locations."/__protected_storage/" = {
            extraConfig = ''
              internal;
              alias ${cfg.contentStorageRoot}/contents/;
              add_header Cache-Control "public, max-age=31536000, immutable" always;
              add_header X-Content-Type-Options "nosniff" always;
              sendfile on;
              tcp_nopush on;
              aio threads;
              output_buffers 1 256k;
            '';
          };
        };
      };

      users.users.catalyrst = { isSystemUser = true; group = "catalyrst"; };
      users.groups.catalyrst = {};
      users.users.squid = { isSystemUser = true; group = "squid"; home = "/var/lib/squid"; };
      users.groups.squid = {};

      # DigitalOcean's monitoring agent is intentionally NOT enabled here. The
      # example config is provider-neutral; operators on DigitalOcean can opt in
      # by setting `services.do-agent.enable = true;` from their own host config.

      nix.gc = { automatic = true; dates = "weekly"; options = "--delete-older-than 14d"; };
      nix.settings.auto-optimise-store = true;

      # Peer auth, socket-only. catalyrst+squid are non-superuser; per-DB grants
      # are applied by postgresql-ownership.service.
      services.postgresql = {
        enable = true;
        package = pkgs.postgresql_18;
        ensureDatabases = [ "content" "marketplace_squid" ];
        ensureUsers = [
          { name = "root"; ensureClauses.superuser = true; }
          { name = "catalyrst"; ensureClauses.login = true; }
          { name = "squid"; ensureClauses.login = true; }
        ];
        authentication = lib.mkForce ''
          local all         all peer
          local replication all peer
        '';
        settings = {
          listen_addresses = lib.mkForce "";
          unix_socket_permissions = "0770";
          shared_buffers = "3GB";
          effective_cache_size = "8GB";
          work_mem = "32MB";
          maintenance_work_mem = "512MB";
          max_connections = 300;
          random_page_cost = 1.1;
          effective_io_concurrency = 200;
          # Single-node: no replication, cut WAL volume.
          wal_level = "minimal";
          max_wal_senders = 0;
          # TODO: pgaudit — not in nixpkgs postgresql_18; needs withPackages build.
          log_connections = true;
          log_disconnections = true;
          log_line_prefix = "%m [%p] %q%u@%d/%a ";
          log_min_duration_statement = 1000;
          log_checkpoints = true;
          log_lock_waits = true;
          log_temp_files = 0;
        };
      };
      # Reach the 0770 socket dir.
      users.users.catalyrst.extraGroups = [ "postgres" ];
      users.users.squid.extraGroups = [ "postgres" ];

      # Idempotent least-priv grants at boot. NOSUPERUSER strips prior emergency grants.
      systemd.services.postgresql-ownership = {
        description = "least-priv DB ownership + grants for catalyrst / squid";
        after = [ "postgresql.service" "squid-search-path.service" ];
        wants = [ "postgresql.service" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = { Type = "oneshot"; RemainAfterExit = true; User = "postgres"; };
        script = ''
          set -e
          PSQL=${pkgs.postgresql_18}/bin/psql

          $PSQL -d postgres -c "ALTER ROLE catalyrst NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT 120;"
          $PSQL -d postgres -c "ALTER ROLE squid     NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT  60;"

          $PSQL -d postgres -c "ALTER DATABASE content OWNER TO catalyrst;"
          $PSQL -d content  -c "REASSIGN OWNED BY postgres TO catalyrst;" || true
          $PSQL -d content  -c "REASSIGN OWNED BY root     TO catalyrst;" || true
          $PSQL -d content  -c "GRANT ALL ON SCHEMA public TO catalyrst;"
          $PSQL -d content  -c "GRANT ALL ON ALL TABLES    IN SCHEMA public TO catalyrst;"
          $PSQL -d content  -c "GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO catalyrst;"
          $PSQL -d content  -c "ALTER DEFAULT PRIVILEGES FOR ROLE catalyrst IN SCHEMA public GRANT ALL ON TABLES    TO catalyrst;"
          $PSQL -d content  -c "ALTER DEFAULT PRIVILEGES FOR ROLE catalyrst IN SCHEMA public GRANT ALL ON SEQUENCES TO catalyrst;"

          $PSQL -d postgres -c "ALTER DATABASE marketplace_squid OWNER TO squid;"
          $PSQL -d marketplace_squid -c "REASSIGN OWNED BY postgres TO squid;" || true
          $PSQL -d marketplace_squid -c "REASSIGN OWNED BY root     TO squid;" || true
          $PSQL -d marketplace_squid -c "GRANT ALL ON SCHEMA squid_marketplace TO squid;"
          $PSQL -d marketplace_squid -c "GRANT ALL ON ALL TABLES    IN SCHEMA squid_marketplace TO squid;"
          $PSQL -d marketplace_squid -c "GRANT ALL ON ALL SEQUENCES IN SCHEMA squid_marketplace TO squid;"
          $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT ALL ON TABLES    TO squid;"
          $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT ALL ON SEQUENCES TO squid;"

          # catalyrst RO into marketplace_squid for third-party Merkle roots + ownership.
          $PSQL -d marketplace_squid -c "GRANT CONNECT ON DATABASE marketplace_squid TO catalyrst;"
          $PSQL -d marketplace_squid -c "GRANT USAGE  ON SCHEMA squid_marketplace TO catalyrst;"
          $PSQL -d marketplace_squid -c "GRANT SELECT ON ALL TABLES IN SCHEMA squid_marketplace TO catalyrst;"
          $PSQL -d marketplace_squid -c "ALTER DEFAULT PRIVILEGES FOR ROLE squid IN SCHEMA squid_marketplace GRANT SELECT ON TABLES TO catalyrst;"

          $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE content           FROM PUBLIC;"
          $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE marketplace_squid FROM PUBLIC;"

        '';
      };

      systemd.services.catalyrst-sync = {
        description = "catalyrst (content + lambdas + sync)";
        after = [ "postgresql.service" "network-online.target" ];
        wants = [ "network-online.target" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = baseSandbox // {
          ExecStart = "${cfg.package}/bin/catalyrst-live";
          Restart = "on-failure";
          RestartSec = 5;
          LimitNOFILE = 1048576;
          User = "catalyrst";
          Group = "catalyrst";
          ProtectHome = true;
          ReadWritePaths = [ "/srv/catalyrst" "/run/postgresql" ];
          MemoryHigh = "12G";
          MemoryMax = "14G";
          TasksMax = 4096;
          SocketBindAllow = [ "tcp:5141" ];
          SocketBindDeny = "any";
          # No IPAddress filter: SYNC_SOURCE pool includes non-CF peers
          # (some peers in the SYNC_SOURCE pool are not behind Cloudflare) and
          # operators may rotate the pool.
        };
        environment = {
          RUST_LOG = "info";
          COMMIT_HASH = cfg.commitHash;
          HTTP_SERVER_HOST = "127.0.0.1";
          CATALYRST_PORT = "5141";
          PUBLIC_URL = cfg.publicUrl;
          COMMS_PROTOCOL = "v3";
          COMMS_FIXED_ADAPTER = "archipelago:archipelago:wss://${cfg.domain}/ws";
          COMMS_VERSION = cfg.commsVersion;
          COMMS_COMMIT_HASH = cfg.commsCommitHash;
          REALM_NAME = cfg.realmName;
          SQUID_DB_NAME = "marketplace_squid";
          SQUID_DB_USER = "catalyrst";
          POSTGRES_HOST = "/run/postgresql";
          POSTGRES_PORT = "5432";
          POSTGRES_CONTENT_USER = "catalyrst";
          # Peer auth: value unused, but the binary panics if unset.
          POSTGRES_CONTENT_PASSWORD = "x";
          POSTGRES_CONTENT_DB = "content";
          SYNC_DB_NAME = "content";
          STORAGE_ROOT_FOLDER = cfg.contentStorageRoot;
          SYNC_STORAGE_ROOT = cfg.syncStorageRoot;
          # PERF-B: hand /content/contents/{hash} byte transfer to nginx via
          # X-Accel-Redirect. Must match the `locations."/__protected_storage/"`
          # block above. Unsetting reverts to Rust-side streaming.
          STORAGE_X_ACCEL_BASE = "/__protected_storage";
          SYNC_ENABLED = lib.boolToString cfg.syncEnabled;
          ENABLE_DEPLOYMENTS = lib.boolToString cfg.enableDeployments;
          THIRD_PARTY_ROOT_SOURCE = "squid";
          IGNORE_BLOCKCHAIN_ACCESS_CHECKS = "false";
          ETH_RPC_URL = "https://rpc.decentraland.org/mainnet";
          CONCURRENT_SYNC_DOWNLOADS = "1500";
          SYNC_SOURCE = lib.concatStringsSep "," cfg.syncSource;
        };
      };

      # Per-role search_path state is dropped by pg_upgrade/restore; re-apply on boot.
      systemd.services.squid-search-path = {
        description = "ensure squid processor search_path is set";
        after = [ "postgresql.service" ]; wants = [ "postgresql.service" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          Type = "oneshot"; RemainAfterExit = true; User = "postgres";
          ExecStart = pkgs.writeShellScript "squid-search-path-fix" ''
            ${pkgs.postgresql_18}/bin/psql -d marketplace_squid \
              -c "ALTER ROLE squid IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;" || true
            ${pkgs.postgresql_18}/bin/psql -d marketplace_squid \
              -c "ALTER ROLE root  IN DATABASE marketplace_squid SET search_path = squid_marketplace, public;" || true
          '';
        };
      };

      systemd.services.squid-eth = lib.recursiveUpdate (mkSquidService {
        description = "marketplace-squid eth processor";
        exec = "${pkgs.nodejs_24}/bin/node --max-old-space-size=4096 /var/lib/squid/lib/eth/main.js";
        socketBindAllow = [ "tcp:5131" ];
      }) { serviceConfig = squidRpcEgress; };
      systemd.services.squid-polygon = lib.recursiveUpdate (mkSquidService {
        description = "marketplace-squid polygon processor";
        exec = "${pkgs.nodejs_24}/bin/node --max-old-space-size=4096 /var/lib/squid/lib/polygon/main.js";
        socketBindAllow = [ "tcp:5132" ];
      }) { serviceConfig = squidRpcEgress; };
      systemd.services.squid-api = lib.recursiveUpdate (mkSquidService {
        description = "marketplace-squid GraphQL API";
        exec = "${pkgs.nodejs_24}/bin/node /var/lib/squid/node_modules/@subsquid/graphql-server/bin/run.js --dialect thegraph";
        socketBindAllow = [ "tcp:5130" ];
        extraEnvironment = { GQL_LISTEN_ADDRESS = "127.0.0.1"; };
      }) {
        serviceConfig = {
          IPAddressAllow = [ "localhost" ];
          IPAddressDeny = "any";
        };
      };

      # Prometheus + exporters are loopback; query via SSH tunnel to :9090.
      # TODO: Alertmanager delivery.
      services.prometheus.exporters.node = {
        enable = true;
        listenAddress = "127.0.0.1";
        port = 9100;
        enabledCollectors = [ "systemd" "textfile" ];
        extraFlags = [ "--collector.textfile.directory=/var/lib/node-exporter-textfile" ];
      };
      services.prometheus.exporters.blackbox = {
        enable = true;
        listenAddress = "127.0.0.1";
        port = 9115;
        configFile = pkgs.writeText "blackbox.yml" ''
          modules:
            about_comms_healthy:
              prober: http
              timeout: 10s
              http:
                valid_status_codes: [200]
                fail_if_body_not_matches_regexp:
                  - '"comms":\{"healthy":true'
                preferred_ip_protocol: ip4
        '';
      };
      services.prometheus = {
        enable = true;
        listenAddress = "127.0.0.1";
        port = 9090;
        globalConfig.scrape_interval = "30s";
        scrapeConfigs = [
          { job_name = "node"; static_configs = [{ targets = [ "127.0.0.1:9100" ]; }]; }
          { job_name = "catalyrst"; static_configs = [{ targets = [ "127.0.0.1:5141" ]; }]; }
          { job_name = "blackbox_about";
            metrics_path = "/probe";
            params.module = [ "about_comms_healthy" ];
            static_configs = [{ targets = [ "${cfg.publicUrl}/content/about" ]; }];
            relabel_configs = [
              { source_labels = [ "__address__" ]; target_label = "__param_target"; }
              { source_labels = [ "__param_target" ]; target_label = "instance"; }
              { target_label = "__address__"; replacement = "127.0.0.1:9115"; }
            ];
          }
        ] ++ lib.optionals cfg.enableComms [
          { job_name = "archipelago"; static_configs = [{ targets = [ "127.0.0.1:5000" "127.0.0.1:5001" "127.0.0.1:5002" ]; }]; }
          { job_name = "pulse"; static_configs = [{ targets = [ "127.0.0.1:5005" ]; }]; }
        ];
        rules = [ (builtins.toJSON {
          groups = [{
            name = "realm";
            rules = [
              { alert = "AboutDownOrCommsUnhealthy";
                expr = "probe_success{job=\"blackbox_about\"} == 0";
                for = "3m"; labels.severity = "critical";
                annotations.summary = "${cfg.domain} /about is down or comms.healthy is false"; }
              { alert = "CertExpiringSoon";
                expr = "probe_ssl_earliest_cert_expiry{job=\"blackbox_about\"} - time() < 1209600";
                for = "1h"; labels.severity = "warning";
                annotations.summary = "TLS cert (as seen at the edge) expires in < 14 days"; }
              { alert = "ServiceDown";
                expr = "up{job=~\"catalyrst|archipelago|pulse|node\"} == 0";
                for = "3m"; labels.severity = "critical";
                annotations.summary = "{{ $labels.job }} {{ $labels.instance }} scrape target is down"; }
              { alert = "LiveKitKeyStale";
                expr = "time() - livekit_rotation_timestamp_seconds > 100*86400";
                for = "1h"; labels.severity = "warning";
                annotations.summary = "LiveKit API key has not been rotated in >100 days"; }
              { alert = "CloudflareIpsStale";
                expr = "time() - cloudflare_ips_refresh_timestamp_seconds > 7*86400";
                for = "1h"; labels.severity = "warning";
                annotations.summary = "Cloudflare IP ranges not refreshed in >7 days"; }
              { alert = "DiskAlmostFull";
                expr = "node_filesystem_avail_bytes{mountpoint=\"/\",fstype!~\"tmpfs|overlay|squashfs|ramfs\"} / node_filesystem_size_bytes{mountpoint=\"/\",fstype!~\"tmpfs|overlay|squashfs|ramfs\"} < 0.10";
                for = "15m"; labels.severity = "warning";
                annotations.summary = "Root filesystem < 10% free"; }
              { alert = "DiskCritical";
                expr = "node_filesystem_avail_bytes{mountpoint=\"/\"} / node_filesystem_size_bytes{mountpoint=\"/\"} < 0.05";
                for = "5m"; labels.severity = "critical";
                annotations.summary = "Root filesystem < 5% free — content sync may stall"; }
            ];
          }];
        }) ];
      };

      systemd.tmpfiles.rules = [
        "d /srv/catalyrst              0755 catalyrst catalyrst -"
        "d ${cfg.contentStorageRoot}   0755 catalyrst catalyrst -"
        "d ${cfg.syncStorageRoot}      0755 catalyrst catalyrst -"
        "d /var/lib/squid              0755 squid     squid     -"
        "d /var/lib/node-exporter-textfile 0755 root root -"
        "d /var/lib/cloudflare         0755 root root -"
      ];

      environment.systemPackages = with pkgs; [ git tmux htop curl jq nodejs_24 postgresql_18 ];
    }

    # ACME: only configure if the operator opted in via acmeEmail. Otherwise
    # the operator sets security.acme.defaults.email themselves.
    (lib.mkIf (cfg.acmeEmail != null) {
      security.acme = {
        acceptTerms = true;
        defaults.email = cfg.acmeEmail;
      };
    })

    {
      # ACME apex+wildcard via Cloudflare DNS-01 (independent of acmeEmail —
      # the per-cert config still needs to exist for nginx).
      security.acme = {
        acceptTerms = true;
        certs.${cfg.domain} = {
          dnsProvider = "cloudflare";
          environmentFile = "/var/lib/secrets/cloudflare-dns.env";
          extraDomainNames = [ "*.${cfg.domain}" ];
          webroot = null;
          group = "nginx";
          postRun = "systemctl reload nginx.service || true";
        };
      };
    }

    # Cloudflare-fronted firewall + nginx real-IP wiring.
    (lib.mkIf cfg.cloudflareFronted {
      # 80/443 accept only from Cloudflare ranges. UDP media/game bypasses CF.
      networking.firewall.extraInputRules = ''
        ip saddr { 173.245.48.0/20, 103.21.244.0/22, 103.22.200.0/22, 103.31.4.0/22, 141.101.64.0/18, 108.162.192.0/18, 190.93.240.0/20, 188.114.96.0/20, 197.234.240.0/22, 198.41.128.0/17, 162.158.0.0/15, 104.16.0.0/13, 104.24.0.0/14, 172.64.0.0/13, 131.0.72.0/22 } tcp dport { 80, 443 } accept
        ip6 saddr { 2400:cb00::/32, 2606:4700::/32, 2803:f800::/32, 2405:b500::/32, 2405:8100::/32, 2a06:98c0::/29, 2c0f:f248::/32 } tcp dport { 80, 443 } accept
      '';

      # CF real-IP seed for first boot before cloudflare-ips-refresh.service runs.
      environment.etc."cf-nginx-real-ip-seed.conf".text = ''
        set_real_ip_from 173.245.48.0/20;
        set_real_ip_from 103.21.244.0/22;
        set_real_ip_from 103.22.200.0/22;
        set_real_ip_from 103.31.4.0/22;
        set_real_ip_from 141.101.64.0/18;
        set_real_ip_from 108.162.192.0/18;
        set_real_ip_from 190.93.240.0/20;
        set_real_ip_from 188.114.96.0/20;
        set_real_ip_from 197.234.240.0/22;
        set_real_ip_from 198.41.128.0/17;
        set_real_ip_from 162.158.0.0/15;
        set_real_ip_from 104.16.0.0/13;
        set_real_ip_from 104.24.0.0/14;
        set_real_ip_from 172.64.0.0/13;
        set_real_ip_from 131.0.72.0/22;
        set_real_ip_from 2400:cb00::/32;
        set_real_ip_from 2606:4700::/32;
        set_real_ip_from 2803:f800::/32;
        set_real_ip_from 2405:b500::/32;
        set_real_ip_from 2405:8100::/32;
        set_real_ip_from 2a06:98c0::/29;
        set_real_ip_from 2c0f:f248::/32;
      '';
      systemd.services.cloudflare-ips-seed = {
        description = "Seed the nginx real-ip include on first boot";
        wantedBy = [ "multi-user.target" ];
        before = [ "nginx.service" ];
        serviceConfig = { Type = "oneshot"; User = "root"; };
        script = ''
          ${pkgs.coreutils}/bin/cp -n /etc/cf-nginx-real-ip-seed.conf /var/lib/cloudflare/nginx-real-ip.conf || true
          ${pkgs.coreutils}/bin/chmod 0644 /var/lib/cloudflare/nginx-real-ip.conf
        '';
      };

      # Fail-soft: on any HTTP error the previous snapshot stays intact (never an
      # empty list, never a hard fail).
      systemd.services.cloudflare-ips-refresh = {
        description = "Refresh Cloudflare edge IP ranges (nginx real-ip include)";
        after = [ "network-online.target" "cloudflare-ips-seed.service" ];
        wants = [ "network-online.target" ];
        serviceConfig = {
          Type = "oneshot";
          User = "root";
          # Hardened oneshot: needs root only to rewrite the nginx include +
          # reload nginx; everything else stripped.
          ProtectSystem = "strict";
          ProtectHome = true;
          PrivateTmp = true;
          PrivateDevices = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectControlGroups = true;
          ReadWritePaths = [
            "/var/lib/cloudflare"
            "/var/lib/node-exporter-textfile"
          ];
          NoNewPrivileges = true;
          RestrictNamespaces = true;
          RestrictRealtime = true;
          RestrictSUIDSGID = true;
          LockPersonality = true;
          CapabilityBoundingSet = "";
          AmbientCapabilities = "";
          SystemCallArchitectures = "native";
          SystemCallFilter = [ "@system-service" "~@privileged" ];
        };
        script = ''
          set -euo pipefail
          umask 022
          DIR=/var/lib/cloudflare
          METRIC=/var/lib/node-exporter-textfile/cloudflare_ips_refresh.prom

          v4=$(${pkgs.coreutils}/bin/mktemp "$DIR/.ips-v4.XXXXXX")
          v6=$(${pkgs.coreutils}/bin/mktemp "$DIR/.ips-v6.XXXXXX")
          trap 'rm -f "$v4" "$v6"' EXIT

          if ! ${pkgs.curl}/bin/curl -sf --max-time 30 https://www.cloudflare.com/ips-v4 -o "$v4"; then
            ${pkgs.util-linux}/bin/logger -t cloudflare-ips "fetch v4 failed; keeping previous"
            exit 0
          fi
          if ! ${pkgs.curl}/bin/curl -sf --max-time 30 https://www.cloudflare.com/ips-v6 -o "$v6"; then
            ${pkgs.util-linux}/bin/logger -t cloudflare-ips "fetch v6 failed; keeping previous"
            exit 0
          fi
          if ! ${pkgs.gnugrep}/bin/grep -Eq '^[0-9].*/[0-9]+$' "$v4" \
             || ! ${pkgs.gnugrep}/bin/grep -Eq '^[0-9a-fA-F:].*/[0-9]+$' "$v6"; then
            ${pkgs.util-linux}/bin/logger -t cloudflare-ips "sanity check failed; keeping previous"
            exit 0
          fi

          ngx=$(${pkgs.coreutils}/bin/mktemp "$DIR/.nginx-real-ip.XXXXXX")
          trap 'rm -f "$v4" "$v6" "$ngx"' EXIT
          ${pkgs.gawk}/bin/awk '{printf "set_real_ip_from %s;\n", $0}' "$v4" "$v6" > "$ngx"
          ${pkgs.coreutils}/bin/chmod 0644 "$ngx"
          ${pkgs.coreutils}/bin/mv "$ngx" "$DIR/nginx-real-ip.conf"
          trap - EXIT

          systemctl reload nginx.service || true

          ${pkgs.coreutils}/bin/mkdir -p "$(dirname "$METRIC")"
          printf '# HELP cloudflare_ips_refresh_timestamp_seconds Unix time of last CF IP refresh\n# TYPE cloudflare_ips_refresh_timestamp_seconds gauge\ncloudflare_ips_refresh_timestamp_seconds %d\n' "$(${pkgs.coreutils}/bin/date +%s)" > "$METRIC"
        '';
      };
      systemd.timers.cloudflare-ips-refresh = {
        description = "Daily Cloudflare edge IP refresh";
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnCalendar = "daily";
          RandomizedDelaySec = "1h";
          Persistent = true;
        };
      };
    })

    # Comms stack — archipelago + livekit + pulse + nats. Opt-in.
    (lib.mkIf commsEnabled {
      services.nginx.virtualHosts."livekit.${cfg.domain}" = {
        onlySSL = true;
        useACMEHost = cfg.domain;
        extraConfig = ''
          add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
        '';
        # /rtc only — explicit `/` → 404 keeps the Twirp admin API off the internet
        # even if a default location is added later.
        locations."/rtc" = {
          proxyPass = "http://127.0.0.1:5880";
          proxyWebsockets = true;
          extraConfig = "proxy_read_timeout 3600s;\nproxy_send_timeout 3600s;";
        };
        locations."/" = { extraConfig = "return 404;"; };
      };

      systemd.services.nats = {
        description = "NATS message bus (archipelago)";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        serviceConfig = noJitHardening // {
          ExecStart = "${pkgs.nats-server}/bin/nats-server -a 127.0.0.1 -p 4222 -m 8222";
          Restart = "always"; RestartSec = 5; DynamicUser = true;
          MemoryMax = "512M";
          TasksMax = 128;
          SocketBindAllow = [ "tcp:5222" "tcp:5223" ];
          SocketBindDeny = "any";
          IPAddressAllow = [ "localhost" ];
          IPAddressDeny = "any";
        };
      };

      # Atomic rotation: write new files, restart livekit, health-check; on failure
      # restore .prev and abort.
      systemd.services.livekit-rotate = {
        description = "Rotate LiveKit API key + secret";
        serviceConfig = {
          Type = "oneshot";
          User = "root";
          # Hardened oneshot: still runs as root because it must rewrite
          # /var/lib/secrets/* and restart livekit + archipelago, but stripped of
          # everything else.
          ProtectSystem = "strict";
          ProtectHome = true;
          PrivateTmp = true;
          PrivateDevices = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectControlGroups = true;
          ReadWritePaths = [
            "/var/lib/secrets"
            "/var/lib/node-exporter-textfile"
          ];
          NoNewPrivileges = true;
          RestrictNamespaces = true;
          RestrictRealtime = true;
          RestrictSUIDSGID = true;
          LockPersonality = true;
          CapabilityBoundingSet = "";
          AmbientCapabilities = "";
          SystemCallArchitectures = "native";
          SystemCallFilter = [ "@system-service" "~@privileged" ];
        };
        script = ''
          set -euo pipefail
          umask 077

          YAML=/var/lib/secrets/livekit.yaml
          ENV=/var/lib/secrets/livekit-api.env
          METRIC=/var/lib/node-exporter-textfile/livekit_rotation.prom

          cp -a "$YAML" "$YAML.prev"
          cp -a "$ENV"  "$ENV.prev"

          KEY="API$(${pkgs.openssl}/bin/openssl rand -hex 6)"
          SECRET="$(${pkgs.openssl}/bin/openssl rand -base64 36 | tr -d '\n')"

          ytmp=$(mktemp "$YAML.XXXXXX")
          ${pkgs.gawk}/bin/awk -v k="$KEY" -v s="$SECRET" '
            BEGIN { in_keys=0 }
            /^keys:[[:space:]]*$/ { print "keys:"; print "  " k ": " s; in_keys=1; next }
            in_keys && /^[[:space:]]/ { next }
            { in_keys=0; print }
          ' "$YAML" > "$ytmp"
          chmod 600 "$ytmp"; chown root:root "$ytmp"
          mv "$ytmp" "$YAML"

          etmp=$(mktemp "$ENV.XXXXXX")
          printf 'LIVEKIT_API_KEY=%s\nLIVEKIT_API_SECRET=%s\n' "$KEY" "$SECRET" > "$etmp"
          chmod 600 "$etmp"; chown root:root "$etmp"
          mv "$etmp" "$ENV"

          systemctl restart livekit.service
          sleep 5

          if ! systemctl is-active --quiet livekit.service; then
            ${pkgs.util-linux}/bin/logger -t livekit-rotate "ROLLBACK: livekit failed to come up with new key"
            mv "$YAML.prev" "$YAML"
            mv "$ENV.prev"  "$ENV"
            systemctl restart livekit.service archipelago-core.service
            exit 1
          fi

          systemctl restart archipelago-core.service

          mkdir -p "$(dirname "$METRIC")"
          printf '# HELP livekit_rotation_timestamp_seconds Unix time of last successful LiveKit key rotation\n# TYPE livekit_rotation_timestamp_seconds gauge\nlivekit_rotation_timestamp_seconds %d\n' "$(date +%s)" > "$METRIC"

          ${pkgs.util-linux}/bin/logger -t livekit-rotate "rotated LiveKit API key (kid=$KEY)"
        '';
      };
      systemd.timers.livekit-rotate = {
        description = "Quarterly LiveKit key rotation";
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnCalendar = "*-01,04,07,10-01 03:00:00";
          Persistent = true;
          RandomizedDelaySec = "1h";
        };
      };

      systemd.services.livekit = {
        description = "LiveKit SFU (comms media)";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ]; wants = [ "network-online.target" ];
        serviceConfig = noJitHardening // {
          LoadCredential = "livekit.yaml:/var/lib/secrets/livekit.yaml";
          ExecStart = "${pkgs.livekit}/bin/livekit-server --config %d/livekit.yaml";
          DynamicUser = true;
          Restart = "always"; RestartSec = 5;
          MemoryMax = "2G";
          TasksMax = 1024;
          SocketBindAllow = [ "tcp:5880" "tcp:5881" "udp:7882" ];
          SocketBindDeny = "any";
          # NO IPAddress filter: LiveKit ICE/STUN candidates are arbitrary; an
          # allowlist would break media.
        };
      };

      systemd.services.archipelago-core = {
        description = "archipelago core (island clustering, mints LiveKit tokens)";
        wantedBy = [ "multi-user.target" ];
        after = [ "nats.service" "livekit.service" ]; wants = [ "nats.service" "livekit.service" ];
        environment = {
          HTTP_SERVER_PORT = "5000"; HTTP_SERVER_HOST = "127.0.0.1";
          NATS_URL = "nats://127.0.0.1:5222";
          ARCHIPELAGO_FLUSH_FREQUENCY = "2.0";
          ARCHIPELAGO_JOIN_DISTANCE = "64";
          ARCHIPELAGO_LEAVE_DISTANCE = "80";
          CHECK_HEARTBEAT_INTERVAL = "60000";
          LIVEKIT_HOST = "wss://livekit.${cfg.domain}";
          LIVEKIT_ISLAND_SIZE = "50";
          COMMS_GATEKEEPER_URL = "https://comms-gatekeeper.decentraland.org";
        };
        serviceConfig = noPgSandbox // {
          LoadCredential = "livekit-env:/var/lib/secrets/livekit-api.env";
          ExecStart = pkgs.writeShellScript "archipelago-core-launcher" ''
            set -a
            . "$CREDENTIALS_DIRECTORY/livekit-env"
            set +a
            exec ${cfg.commsPackages.archipelago-workers}/bin/archipelago-core
          '';
          DynamicUser = true;
          Restart = "always"; RestartSec = 10;
          MemoryMax = "1G";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:5000" ];
          SocketBindDeny = "any";
          # CF ranges only — comms-gatekeeper.decentraland.org is CF-fronted.
          IPAddressAllow = [ "localhost" "104.16.0.0/13" "172.64.0.0/13" ];
          IPAddressDeny = "any";
        };
      };
      systemd.services.archipelago-ws-connector = {
        description = "archipelago ws-connector (client comms WebSocket)";
        wantedBy = [ "multi-user.target" ];
        after = [ "nats.service" ]; wants = [ "nats.service" ];
        environment = {
          HTTP_SERVER_PORT = "5001"; HTTP_SERVER_HOST = "127.0.0.1";
          NATS_URL = "nats://127.0.0.1:5222";
          ETH_NETWORK = "mainnet";
          COMMS_GATEKEEPER_URL = "https://comms-gatekeeper.decentraland.org";
        };
        serviceConfig = noPgSandbox // {
          ExecStart = "${cfg.commsPackages.archipelago-workers}/bin/archipelago-ws-connector";
          DynamicUser = true;
          Restart = "always"; RestartSec = 10;
          MemoryMax = "1G";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:5001" ];
          SocketBindDeny = "any";
          IPAddressAllow = [ "localhost" "104.16.0.0/13" "172.64.0.0/13" ];
          IPAddressDeny = "any";
        };
      };
      systemd.services.archipelago-stats = {
        description = "archipelago stats (monitoring REST)";
        wantedBy = [ "multi-user.target" ];
        after = [ "nats.service" ]; wants = [ "nats.service" ];
        environment = {
          HTTP_SERVER_PORT = "5002"; HTTP_SERVER_HOST = "127.0.0.1";
          NATS_URL = "nats://127.0.0.1:5222";
          CONTENT_URL = "${cfg.publicUrl}/content/";
        };
        serviceConfig = noPgSandbox // {
          ExecStart = "${cfg.commsPackages.archipelago-workers}/bin/archipelago-stats";
          DynamicUser = true;
          Restart = "always"; RestartSec = 10;
          MemoryMax = "1G";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:5002" ];
          SocketBindDeny = "any";
          IPAddressAllow = [ "localhost" "104.16.0.0/13" "172.64.0.0/13" ];
          IPAddressDeny = "any";
        };
      };

      systemd.services.pulse = {
        description = "Pulse authoritative comms server (.NET, ENet/UDP)";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ]; wants = [ "network-online.target" ];
        environment = {
          DOTNET_SYSTEM_GLOBALIZATION_INVARIANT = "1";
          ENV = "prd";
          Transport__Port = "7777";
          HttpService__Port = "5005";
          HttpService__Host = "127.0.0.1";
          # Kestrel (.NET ASP.NET) ignores HttpService__Host; ASPNETCORE_URLS wins.
          ASPNETCORE_URLS = "http://127.0.0.1:5005";
          Metrics__Type = "Prometheus";
          Peers__MaxWorkerThreads = "2";
          Transport__MaxConcurrentConnections = "1024";
          Transport__MaxPeers = "1200";
        };
        # noPgSandbox (not noJitHardening): .NET RyuJIT needs W+X pages.
        # No IPAddress filter — public ENet/UDP game server.
        serviceConfig = noPgSandbox // {
          ExecStart = "${cfg.commsPackages.pulse}/bin/DCLPulse";
          # WorkingDirectory pins .NET content-root so appsettings.json loads.
          WorkingDirectory = "${cfg.commsPackages.pulse}/lib/dclpulse";
          Restart = "always"; RestartSec = 10; DynamicUser = true;
          MemoryHigh = "4G";
          MemoryMax = "6G";
          TasksMax = 512;
          SocketBindAllow = [ "udp:7777" "tcp:5005" ];
          SocketBindDeny = "any";
        };
      };
    })
  ]);

  meta.maintainers = [ ];
}
