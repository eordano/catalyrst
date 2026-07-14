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
    SystemCallFilter = [ "@system-service" "~@privileged" ];
    UMask = "0077";
    DevicePolicy = "closed";
  };

  commsHardening = baseSandbox // { ProtectHome = true; };
  noPgSandbox = commsHardening // { PrivateUsers = true; };
  noJitHardening = noPgSandbox // { MemoryDenyWriteExecute = true; };

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

  subOpt = default: lib.mkOption {
    type = lib.types.bool;
    inherit default;
  };

  anyAllPackageService = with cfg.subServices;
    explore || create || social || data || socialRpc || explorerApi
    || sceneState || telemetry || worldStorage || profileImages || signatures;

  anySubService = anyAllPackageService || cfg.subServices.governance || cfg.subServices.presence;

  connBundle = db: "postgresql:///${db}?host=/run/postgresql&user=catalyrst";
  # systemd Environment= lines expand % specifiers, so URL-encoded values need
  # %% here to reach the binary as a single %.
  connBundleAuth = db: "postgres://catalyrst@%%2Frun%%2Fpostgresql/${db}";

  bundleCommonEnv = {
    RUST_LOG = "info";
    HTTP_SERVER_HOST = "127.0.0.1";
    COMMIT_HASH = cfg.commitHash;
    ETH_NETWORK = "mainnet";
    NETWORK_ID = "1";

    CONTENT_PG_CONNECTION_STRING = connBundle "content";
    DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "marketplace_squid";
    DAPPS_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
    DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "marketplace_squid";
    SQUID_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
    PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "places";
    PLACES_EVENTS_PG_CONNECTION_STRING = connBundle "places_events";
    WORLDS_PG_CONNECTION_STRING = connBundle "worlds";
    WORLD_STORAGE_PG_CONNECTION_STRING = connBundle "worlds";
    BUILDER_PG_CONNECTION_STRING = connBundle "builder";
    CAMERA_REEL_PG_CONNECTION_STRING = connBundle "camera_reel";
    AB_REGISTRY_PG_CONNECTION_STRING = connBundle "ab_registry";
    COMMUNITIES_PG_CONNECTION_STRING = connBundle "communities";
    MUTES_PG_CONNECTION_STRING = connBundle "communities";
    COMMS_PG_CONNECTION_STRING = connBundle "comms";
    NOTIFICATIONS_PG_CONNECTION_STRING = connBundle "notifications";
    BADGES_PG_CONNECTION_STRING = connBundle "badges";
    MEDIA_PG_CONNECTION_STRING = connBundle "media";
    PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "price";
    CREDITS_PG_CONNECTION_STRING = connBundle "credits";
    SIGNATURES_PG_CONNECTION_STRING = connBundle "signatures";

    CONTENT_SERVER_ADDRESS = "${cfg.publicUrl}/content";
    CONTENT_URL = "${cfg.publicUrl}/content/";
    CONTENT_BASE_URL = "${cfg.publicUrl}/content";
    CONTENT_PUBLIC_URL = "${cfg.publicUrl}/content";
    LAMBDAS_PUBLIC_URL = "${cfg.publicUrl}/lambdas";
    CATALYST_URL = "http://127.0.0.1:5141";
    PLACES_API_URL = "http://127.0.0.1:5143";
    COMMS_GATEKEEPER_URL = "http://127.0.0.1:5145";
    PROFILE_IMAGES_URL = "https://profile-images.decentraland.org";
    PROFILE_CDN_BASE_URL = "https://profile-images.decentraland.org";

    LIVEKIT_HOST = "livekit.${cfg.domain}";
    LIVEKIT_WS_URL = "wss://livekit.${cfg.domain}";

    WORLDS_CONTENT_DIR = "/srv/catalyrst/worlds/contents";
    CONTENT_STORAGE_DIR = "/srv/catalyrst/camera-reel";
    COMMUNITIES_CONTENT_DIR = "/srv/catalyrst/communities/content";
  };

  bundleRwDirs = [ "/srv/catalyrst" "/run/postgresql" ];

  mkBundle =
    { description
    , bin
    , port
    , extraEnv ? { }
    , needsLivekit ? false
    , afterExtra ? [ ]
    }:
    let
      exe = "${cfg.bundlesPackage}/bin/${bin}";
      livekitEnv = needsLivekit && commsEnabled;
    in
    {
      inherit description;
      after = [ "postgresql.service" "postgresql-bundles.service" "network-online.target" ]
        ++ lib.optional livekitEnv "livekit.service"
        ++ afterExtra;
      wants = [ "network-online.target" "postgresql-bundles.service" ]
        ++ lib.optional livekitEnv "livekit.service";
      wantedBy = [ "multi-user.target" ];
      environment = bundleCommonEnv // extraEnv;
      serviceConfig = baseSandbox // {
        ExecStart =
          if livekitEnv then
            pkgs.writeShellScript "${bin}-launcher" ''
              set -a
              . "$CREDENTIALS_DIRECTORY/livekit-env"
              set +a
              exec ${exe}
            ''
          else
            exe;
        Restart = "always";
        RestartSec = 10;
        LimitNOFILE = 1048576;
        User = "catalyrst";
        Group = "catalyrst";
        ProtectHome = true;
        ReadWritePaths = bundleRwDirs;
        MemoryHigh = "1500M";
        MemoryMax = "1500M";
        TasksMax = 1024;
        SocketBindAllow = [ "tcp:${toString port}" ];
        SocketBindDeny = "any";
      } // lib.optionalAttrs livekitEnv {
        LoadCredential = "livekit-env:/var/lib/secrets/livekit-api.env";
      };
    };

  bproxy = port: { proxyPass = "http://127.0.0.1:${toString port}"; };
  exploreLoc = bproxy 5143;
  createLoc = bproxy 5144;
  socialLoc = bproxy 5145;
  dataLoc = bproxy 5146;

  bundleProxyLocations =
    lib.optionalAttrs cfg.subServices.explore {
      "/api/places" = exploreLoc;
      "/api/destinations" = exploreLoc;
      "/api/map" = exploreLoc;
      "/api/report" = exploreLoc;
      "/places" = exploreLoc;
      "/world_names" = exploreLoc;
      "/worlds" = exploreLoc;
      "/categories" = exploreLoc;
      "/pois" = exploreLoc;
      "/api/events" = exploreLoc;
      "/api/schedules" = exploreLoc;
      "/api/poster" = exploreLoc;
      "/api/poster-vertical" = exploreLoc;
      "/api/profiles/settings" = exploreLoc;
      "/events/" = exploreLoc;
      "/v1/map.png" = exploreLoc;
      "/v1/minimap.png" = exploreLoc;
      "/v1/estatemap.png" = exploreLoc;
      "/v1/tiles" = exploreLoc;
      "/v2/" = exploreLoc;
      "/world/" = exploreLoc;
      "/wallet/" = exploreLoc;
    }
    // lib.optionalAttrs cfg.subServices.create {
      "/v1/newsletter" = createLoc;
      "/v1/collections/" = createLoc;
      "/v1/storage/" = createLoc;
      "/images" = createLoc;
      "/users" = createLoc;
      "/profiles" = createLoc;
      "/registry" = createLoc;
      "/denylist" = createLoc;
      "/queues/" = createLoc;
      "/flush-cache" = createLoc;
      "~ ^/places/[^/]+/images" = createLoc;
    }
    // lib.optionalAttrs cfg.subServices.social {
      "/v1/communities" = socialLoc;
      "/v1/members" = socialLoc;
      "/v1/community-voice-chats" = socialLoc;
      "/v1/moderation/" = socialLoc;
      "/federation/communities" = socialLoc;
      "/social/communities" = socialLoc;
      "/get-scene-adapter" = socialLoc;
      "/get-server-scene-adapter" = socialLoc;
      "/scene-participants" = socialLoc;
      "/scene-bans/" = socialLoc;
      "/private-messages/token" = socialLoc;
      "/community-voice-chat" = socialLoc;
      "/cast/" = socialLoc;
      "= /bans" = socialLoc;
      "= /livekit-webhook" = socialLoc;
      "/notifications" = socialLoc;
      "/subscription" = socialLoc;
      "/set-email" = socialLoc;
      "/confirm-email" = socialLoc;
      "/badges/" = socialLoc;
      "/translate" = socialLoc;
    }
    // lib.optionalAttrs cfg.subServices.data {
      "/v1/catalog" = dataLoc;
      "/v2/catalog" = dataLoc;
      "/v1/items" = dataLoc;
      "/v1/nfts" = dataLoc;
      "/v1/orders" = dataLoc;
      "/v1/bids" = dataLoc;
      "/v1/sales" = dataLoc;
      "/v1/trades" = dataLoc;
      "/v1/accounts" = dataLoc;
      "/v1/activity" = dataLoc;
      "/v1/contracts" = dataLoc;
      "/v1/owners" = dataLoc;
      "/v1/prices" = dataLoc;
      "/v1/trendings" = dataLoc;
      "/v1/volume" = dataLoc;
      "/v1/transactions" = dataLoc;
      "/users/" = dataLoc;
      "/seasons" = dataLoc;
      "= /captcha" = dataLoc;
      "/api/v3/simple/price" = dataLoc;
      "~ ^/(mainnet|sepolia|polygon|amoy|ethereum)$" = dataLoc;
    }
    // lib.optionalAttrs cfg.subServices.socialRpc {
      "/social-rpc" = {
        proxyPass = "http://127.0.0.1:5148";
        proxyWebsockets = true;
        extraConfig = ''
          rewrite ^/social-rpc/?(.*)$ /$1 break;
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
    }
    // lib.optionalAttrs cfg.subServices.sceneState {
      "/scene-state/admin" = { extraConfig = "return 404;"; };
      "/scene-state" = {
        proxyPass = "http://127.0.0.1:5209";
        proxyWebsockets = true;
        extraConfig = ''
          rewrite ^/scene-state/?(.*)$ /$1 break;
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
    }
    // lib.optionalAttrs cfg.subServices.worldStorage {
      # x-original-path: the trailing-slash proxy_pass strips /world-storage/,
      # so signed-fetch verification needs the path the caller actually signed.
      "/world-storage/" = {
        proxyPass = "http://127.0.0.1:5149/";
        extraConfig = "proxy_set_header x-original-path $request_uri;";
      };
    }
    // lib.optionalAttrs cfg.subServices.telemetry {
      "/telemetry/" = { proxyPass = "http://127.0.0.1:5150/"; };
    }
    // lib.optionalAttrs cfg.subServices.governance {
      "/governance-api/" = { proxyPass = "http://127.0.0.1:5151/"; };
    }
    // lib.optionalAttrs cfg.subServices.presence {
      "/presence/" = { proxyPass = "http://127.0.0.1:5152/"; };
    };
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
        Optional attrset of comms-related packages: { catalyrst-archipelago, pulse }.
        When enableComms = true, must provide catalyrst-archipelago and pulse.
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

    ethRpcUrl = lib.mkOption {
      type = lib.types.str;
      example = "http://127.0.0.1:8545";
      description = ''
        HTTPS RPC endpoint used for EIP-1654 write validation. No default:
        the previous one was Decentraland's production RPC gateway.
      '';
    };

    commsGatekeeperUrl = lib.mkOption {
      type = lib.types.str;
      example = "http://127.0.0.1:5138";
      description = ''
        Comms gatekeeper base URL for catalyrst-archipelago. No default:
        the previous one was the production gatekeeper.
      '';
    };

    syncSource = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = ''
        Peer URLs catalyrst pulls deployments from, joined with ',' into
        SYNC_SOURCE. Empty by default: this list used to name the public
        Genesis City peers, so a stock deployment synced from production.
      '';
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
        catalyrst-archipelago (clustering + ws-connector + stats in one Rust
        binary on :5139), LiveKit SFU, Pulse.
        Requires commsPackages with catalyrst-archipelago + pulse.
      '';
    };

    bundlesPackage = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = ''
        The catalyrst-all multi-binary package: the explore/create/social/data
        bundles plus catalyrst-social-rpc, catalyrst-explorer-api,
        catalyrst-scene-state, catalyrst-telemetry, catalyrst-world-storage,
        catalyrst-profile-images and catalyrst-signatures. Required when any
        services.catalyrst.subServices flag except governance/presence is
        enabled. Typically set to
        inputs.catalyrst.packages.''${pkgs.system}.catalyrst-all.
      '';
    };

    governancePackage = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = ''
        The catalyrst-governance package. Required when
        services.catalyrst.subServices.governance is enabled. Typically set to
        inputs.catalyrst.packages.''${pkgs.system}.catalyrst-governance.
      '';
    };

    presencePackage = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      description = ''
        The catalyrst-presence package. Required when
        services.catalyrst.subServices.presence is enabled. Typically set to
        inputs.catalyrst.packages.''${pkgs.system}.catalyrst-presence.
      '';
    };

    subServices = lib.mkOption {
      description = ''
        Which sibling services to run beside the content server. Together they
        stand up the full realm service surface: places/events/worlds/map on
        the explore bundle (:5143), builder/camera-reel/registry on create
        (:5144), communities/comms/notifications/badges/media on social
        (:5145), market/economy/price/credits on data (:5146), plus the
        standalone singles (social-rpc :5148, explorer-api :5137, scene-state
        :5209, telemetry :5150, world-storage :5149, governance :5151,
        presence :5152, signatures :5159, profile-images :5161).
      '';
      default = { };
      type = lib.types.submodule {
        options = {
          explore = subOpt true;
          create = subOpt true;
          social = subOpt true;
          data = subOpt true;
          socialRpc = subOpt true;
          explorerApi = subOpt true;
          sceneState = subOpt true;
          telemetry = subOpt true;
          worldStorage = subOpt true;
          profileImages = subOpt true;
          signatures = subOpt true;
          governance = subOpt true;
          presence = subOpt true;
        };
      };
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
            && cfg.commsPackages ? catalyrst-archipelago
            && cfg.commsPackages ? pulse);
          message = ''
            services.catalyrst.enableComms = true requires services.catalyrst.commsPackages
            to provide both `catalyrst-archipelago` and `pulse` packages.
          '';
        }
      ];

      services.logrotate.checkConfig = false;
      boot.tmp.cleanOnBoot = true;
      zramSwap.enable = true;
      networking.domain = lib.mkDefault "";

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
          locations."= /metrics" = { extraConfig = "return 404;"; };
          locations."= /admin"   = { extraConfig = "return 404;"; };
          locations."= /debug"   = { extraConfig = "return 404;"; };
          locations."/ws" = lib.mkIf cfg.enableComms {
            proxyPass = "http://127.0.0.1:5139";
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
          locations."/__protected_storage/" = {
            extraConfig = ''
              internal;
              alias ${cfg.contentStorageRoot}/contents/;
              # X-Accel-Redirect drops the upstream response headers and nginx's static
              # module would generate its default mtime-size ETag, breaking parity with
              # the TS catalyst whose ETag is the quoted content CID. Disable the auto
              # ETag and re-emit the app's headers (kept in $upstream_http_* across the
              # internal redirect).
              etag off;
              add_header ETag $upstream_http_etag always;
              add_header Access-Control-Expose-Headers $upstream_http_access_control_expose_headers always;
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

      nix.gc = { automatic = true; dates = "weekly"; options = "--delete-older-than 14d"; };
      nix.settings.auto-optimise-store = true;

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
      users.users.catalyrst.extraGroups = [ "postgres" ];
      users.users.squid.extraGroups = [ "postgres" ];

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
        };
        environment = {
          RUST_LOG = "info";
          COMMIT_HASH = cfg.commitHash;
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5141";
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
          POSTGRES_CONTENT_PASSWORD = "x";
          POSTGRES_CONTENT_DB = "content";
          SYNC_DB_NAME = "content";
          STORAGE_ROOT_FOLDER = cfg.contentStorageRoot;
          SYNC_STORAGE_ROOT = cfg.syncStorageRoot;
          STORAGE_X_ACCEL_BASE = "/__protected_storage";
          SYNC_ENABLED = lib.boolToString cfg.syncEnabled;
          ENABLE_DEPLOYMENTS = lib.boolToString cfg.enableDeployments;
          THIRD_PARTY_ROOT_SOURCE = "squid";
          IGNORE_BLOCKCHAIN_ACCESS_CHECKS = "false";
          ETH_RPC_URL = cfg.ethRpcUrl;
          CONCURRENT_SYNC_DOWNLOADS = "1500";
          SYNC_SOURCE = lib.concatStringsSep "," cfg.syncSource;
        };
      };

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
          { job_name = "archipelago"; static_configs = [{ targets = [ "127.0.0.1:5139" ]; }]; }
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
              { alert = "SyncHeartbeatStale";
                expr = "time() - catalyrst_sync_heartbeat_timestamp_seconds > 900";
                for = "5m"; labels.severity = "critical";
                annotations.summary = "catalyrst sync loop has not heartbeat in >15 min — live forward sync is dead or every upstream stream is failing"; }
              { alert = "SyncIngestSilent";
                expr = "increase(catalyrst_sync_deployments_total[2h]) == 0";
                for = "30m"; labels.severity = "warning";
                annotations.summary = "No deployments ingested in >2h — upstream network idle (unlikely on mainnet) or ingest is broken while the loop still beats"; }
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

    (lib.mkIf anySubService {
      assertions = [
        {
          assertion = !anyAllPackageService || cfg.bundlesPackage != null;
          message = ''
            services.catalyrst.subServices requires services.catalyrst.bundlesPackage
            (the catalyrst-all package), or disable every subServices flag that
            it backs.
          '';
        }
        {
          assertion = !cfg.subServices.governance || cfg.governancePackage != null;
          message = "services.catalyrst.subServices.governance requires services.catalyrst.governancePackage.";
        }
        {
          assertion = !cfg.subServices.presence || cfg.presencePackage != null;
          message = "services.catalyrst.subServices.presence requires services.catalyrst.presencePackage.";
        }
      ];

      services.postgresql.ensureDatabases = [
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
        "signatures"
        "governance"
        "presence"
        "catalyrst"
      ];

      services.nginx.virtualHosts.${cfg.domain}.locations = bundleProxyLocations;

      environment.etc = lib.mkIf cfg.subServices.explorerApi {
        "catalyrst/feature-flags.json".text = ''{"flags":{},"variants":{}}'';
        "catalyrst/denylist.json".text = ''{"users":[],"names":[],"coordinates":[]}'';
      };

      systemd.tmpfiles.rules = [
        "d /var/lib/secrets                   0700 root      root      -"
        "d /srv/catalyrst/worlds              0755 catalyrst catalyrst -"
        "d /srv/catalyrst/worlds/contents     0755 catalyrst catalyrst -"
        "d /srv/catalyrst/camera-reel         0755 catalyrst catalyrst -"
        "d /srv/catalyrst/communities         0755 catalyrst catalyrst -"
        "d /srv/catalyrst/communities/content 0755 catalyrst catalyrst -"
        "d /srv/catalyrst/profile-images      0755 catalyrst catalyrst -"
      ];

      systemd.services = {
        postgresql-bundles = {
          description = "DB ownership + grants for the catalyrst sub-services";
          after = [ "postgresql.service" "postgresql-ownership.service" ];
          wants = [ "postgresql.service" ];
          wantedBy = [ "multi-user.target" ];
          serviceConfig = { Type = "oneshot"; RemainAfterExit = true; User = "postgres"; };
          script = ''
            set -e
            PSQL=${pkgs.postgresql_18}/bin/psql

            for db in places places_events worlds builder camera_reel ab_registry \
                      communities comms notifications badges media price credits \
                      signatures governance presence catalyrst; do
              $PSQL -d postgres -c "ALTER DATABASE $db OWNER TO catalyrst;"
              $PSQL -d postgres -c "REVOKE CONNECT ON DATABASE $db FROM PUBLIC;"
              $PSQL -d "$db" -c "REASSIGN OWNED BY postgres TO catalyrst;" || true
              $PSQL -d "$db" -c "GRANT ALL ON SCHEMA public TO catalyrst;"
            done

            $PSQL -d catalyrst -c "CREATE SCHEMA IF NOT EXISTS telemetry AUTHORIZATION catalyrst;"

            # market/economy views + favorites live as extra schemas inside the
            # existing marketplace_squid DB, beside the squid indexer's
            # squid_marketplace schema. catalyrst owns them; squid keeps its own.
            $PSQL -d marketplace_squid -c "GRANT CREATE ON DATABASE marketplace_squid TO catalyrst;"
            $PSQL -d marketplace_squid -c "CREATE SCHEMA IF NOT EXISTS marketplace AUTHORIZATION catalyrst;"
            $PSQL -d marketplace_squid -c "CREATE SCHEMA IF NOT EXISTS favorites   AUTHORIZATION catalyrst;"
            $PSQL -d marketplace_squid -c "GRANT ALL ON SCHEMA public TO catalyrst;"
          '';
        };
      }
      // lib.optionalAttrs cfg.subServices.explore {
        catalyrst-explore = mkBundle {
          description = "catalyrst explore bundle (places, events, archipelago, worlds, map, lists; port 5143)";
          bin = "catalyrst-explore";
          port = 5143;
          needsLivekit = true;
          extraEnv = { BUNDLE_HTTP_PORT = "5143"; };
        };
      }
      // lib.optionalAttrs cfg.subServices.create {
        catalyrst-create = mkBundle {
          description = "catalyrst create bundle (builder, camera-reel, registry; port 5144)";
          bin = "catalyrst-create";
          port = 5144;
          extraEnv = {
            BUNDLE_HTTP_PORT = "5144";
            API_URL = cfg.publicUrl;
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.social {
        catalyrst-social = mkBundle {
          description = "catalyrst social bundle (communities, comms, notifications, badges, media; port 5145)";
          bin = "catalyrst-social";
          port = 5145;
          needsLivekit = true;
          extraEnv = { BUNDLE_HTTP_PORT = "5145"; };
        };
      }
      // lib.optionalAttrs cfg.subServices.data {
        catalyrst-data = mkBundle {
          description = "catalyrst data bundle (market, economy, price, credits; port 5146)";
          bin = "catalyrst-data";
          port = 5146;
          extraEnv = {
            BUNDLE_HTTP_PORT = "5146";
            DAPPS_PG_COMPONENT_PSQL_SCHEMA = "marketplace";
            DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA = "marketplace";
            DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundleAuth "marketplace_squid";
            DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundleAuth "marketplace_squid";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.socialRpc {
        catalyrst-social-rpc = mkBundle {
          description = "catalyrst-social-rpc (dcl-rpc WebSocket: friends/presence/voice; port 5148)";
          bin = "catalyrst-social-rpc";
          port = 5148;
          afterExtra = lib.optional cfg.subServices.social "catalyrst-social.service";
          extraEnv = {
            HTTP_SERVER_PORT = "5148";
            DATABASE_URL = connBundle "communities";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.explorerApi {
        catalyrst-explorer-api = {
          description = "catalyrst-explorer-api (realm provider + auth + feature flags, port 5137)";
          after = [ "network-online.target" ];
          wants = [ "network-online.target" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5137";
            REALM_NAME = cfg.realmName;
            ENV_NAME = "prd";
            NETWORK_ID = "1";
            CATALYST_URL = "http://127.0.0.1:5141";
            LAMBDAS_URL = "${cfg.publicUrl}/lambdas";
            PUBLIC_REALM_URL = cfg.publicUrl;
            HOT_SCENES_URL = "http://127.0.0.1:5143/hot-scenes";
            FEATURE_FLAGS_CONFIG_PATH = "/etc/catalyrst/feature-flags.json";
            BLOCKLIST_PATH = "/etc/catalyrst/denylist.json";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.bundlesPackage}/bin/catalyrst-explorer-api";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5137" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.sceneState {
        catalyrst-scene-state = {
          description = "catalyrst-scene-state (authoritative SDK7 scene state, port 5209)";
          after = [ "network-online.target" ];
          wants = [ "network-online.target" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5209";
            REALM_NAME = cfg.realmName;
            WORLD_SERVER_URL = cfg.publicUrl;
            STORAGE_URL = cfg.publicUrl;
            RUST_LOG = "catalyrst_scene_state=info";
          };
          serviceConfig = noJitHardening // {
            ExecStart = "${cfg.bundlesPackage}/bin/catalyrst-scene-state";
            Restart = "always";
            RestartSec = 10;
            DynamicUser = true;
            MemoryHigh = "1536M";
            MemoryMax = "2G";
            TasksMax = 512;
            SocketBindAllow = [ "tcp:5209" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.telemetry {
        catalyrst-telemetry = {
          description = "catalyrst-telemetry (event sink + dashboard, port 5150)";
          after = [ "postgresql.service" "postgresql-bundles.service" "network-online.target" ];
          wants = [ "network-online.target" "postgresql-bundles.service" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5150";
            TELEMETRY_PG_CONNECTION_STRING = "postgresql:///catalyrst?host=/run/postgresql&user=catalyrst&options=-c%%20search_path%%3Dtelemetry";
            TELEMETRY_BASE_PATH = "/telemetry";
            FLAGS_URL = "http://127.0.0.1:5137/explorer.json";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.bundlesPackage}/bin/catalyrst-telemetry";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/run/postgresql" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5150" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.worldStorage {
        catalyrst-world-storage = {
          description = "catalyrst-world-storage (world env/player key-value store + ACLs, port 5149)";
          after = [
            "postgresql.service"
            "postgresql-bundles.service"
            "catalyrst-world-storage-secret.service"
            "network-online.target"
          ];
          wants = [
            "network-online.target"
            "postgresql-bundles.service"
            "catalyrst-world-storage-secret.service"
          ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5149";
            WORLD_STORAGE_PG_CONNECTION_STRING = connBundle "worlds";
            LAMBDAS_URL = "http://127.0.0.1:5141/lambdas";
            WORLDS_CONTENT_SERVER_URL = "http://127.0.0.1:5143";
            PLACES_URL = "http://127.0.0.1:5143";
            RPC_ENDPOINT_ETH = cfg.ethRpcUrl;
            CORS_ALLOWED_ORIGIN_SUFFIXES = "decentraland.org,decentraland.zone,decentraland.today,${cfg.domain}";
          };
          serviceConfig = baseSandbox // {
            LoadCredential = "world-storage-env:/var/lib/secrets/catalyrst-world-storage.env";
            ExecStart = pkgs.writeShellScript "catalyrst-world-storage-launcher" ''
              set -a
              . "$CREDENTIALS_DIRECTORY/world-storage-env"
              set +a
              exec ${cfg.bundlesPackage}/bin/catalyrst-world-storage
            '';
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/run/postgresql" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5149" ];
            SocketBindDeny = "any";
          };
        };
        catalyrst-world-storage-secret = {
          description = "Generate the catalyrst-world-storage ENCRYPTION_KEY";
          wantedBy = [ "multi-user.target" ];
          before = [ "catalyrst-world-storage.service" ];
          serviceConfig = { Type = "oneshot"; RemainAfterExit = true; User = "root"; };
          script = ''
            set -euo pipefail
            umask 077
            mkdir -p /var/lib/secrets
            ENV=/var/lib/secrets/catalyrst-world-storage.env
            if [ ! -s "$ENV" ]; then
              printf 'ENCRYPTION_KEY=%s\n' "$(${pkgs.openssl}/bin/openssl rand -hex 32)" > "$ENV"
              chmod 600 "$ENV"
            fi
          '';
        };
      }
      // lib.optionalAttrs cfg.subServices.profileImages {
        catalyrst-profile-images = {
          description = "catalyrst-profile-images (profile picture proxy + cache, port 5161)";
          after = [ "network-online.target" ];
          wants = [ "network-online.target" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5161";
            PROFILE_IMAGES_ORIGIN_URL = "https://profile-images.decentraland.org";
            PROFILE_IMAGES_CACHE_DIR = "/srv/catalyrst/profile-images";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.bundlesPackage}/bin/catalyrst-profile-images";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/srv/catalyrst/profile-images" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5161" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.signatures {
        catalyrst-signatures = {
          description = "catalyrst-signatures (auth-chain signature index, port 5159)";
          after = [ "postgresql.service" "postgresql-bundles.service" "network-online.target" ];
          wants = [ "network-online.target" "postgresql-bundles.service" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5159";
            SIGNATURES_PG_CONNECTION_STRING = connBundle "signatures";
            DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "marketplace_squid";
            DAPPS_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
            CHAIN_NAME = "ETHEREUM_MAINNET";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.bundlesPackage}/bin/catalyrst-signatures";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/run/postgresql" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5159" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.governance {
        catalyrst-governance = {
          description = "catalyrst-governance (governance mirror + read API, port 5151)";
          after = [ "postgresql.service" "postgresql-bundles.service" "network-online.target" ];
          wants = [ "network-online.target" "postgresql-bundles.service" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5151";
            GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "governance";
            GOVERNANCE_API_URL = "https://governance.decentraland.org/api";
            # Standalone deployments have no local governance archive writers,
            # so the crate's own upstream poller is the only ingestion path.
            GOVERNANCE_POLL_ENABLED = "true";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.governancePackage}/bin/catalyrst-governance";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/run/postgresql" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5151" ];
            SocketBindDeny = "any";
          };
        };
      }
      // lib.optionalAttrs cfg.subServices.presence {
        catalyrst-presence = {
          description = "catalyrst-presence (user-count history, port 5152)";
          after = [ "postgresql.service" "postgresql-bundles.service" "network-online.target" ]
            ++ lib.optional commsEnabled "catalyrst-archipelago.service";
          wants = [ "network-online.target" "postgresql-bundles.service" ];
          wantedBy = [ "multi-user.target" ];
          environment = {
            RUST_LOG = "info";
            HTTP_SERVER_HOST = "127.0.0.1";
            HTTP_SERVER_PORT = "5152";
            PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING = connBundle "presence";
            ARCHIPELAGO_URL = "http://127.0.0.1:5139";
            COMMS_URL = "http://127.0.0.1:5145";
            WORLDS_SERVER_URL = "http://127.0.0.1:5143";
          };
          serviceConfig = baseSandbox // {
            ExecStart = "${cfg.presencePackage}/bin/catalyrst-presence";
            Restart = "always";
            RestartSec = 10;
            User = "catalyrst";
            Group = "catalyrst";
            ProtectHome = true;
            ReadWritePaths = [ "/run/postgresql" ];
            MemoryHigh = "512M";
            MemoryMax = "512M";
            TasksMax = 256;
            SocketBindAllow = [ "tcp:5152" ];
            SocketBindDeny = "any";
          };
        };
      };
    })

    (lib.mkIf (cfg.acmeEmail != null) {
      security.acme = {
        acceptTerms = true;
        defaults.email = cfg.acmeEmail;
      };
    })

    {
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

    (lib.mkIf cfg.cloudflareFronted {
      networking.firewall.extraInputRules = ''
        ip saddr { 173.245.48.0/20, 103.21.244.0/22, 103.22.200.0/22, 103.31.4.0/22, 141.101.64.0/18, 108.162.192.0/18, 190.93.240.0/20, 188.114.96.0/20, 197.234.240.0/22, 198.41.128.0/17, 162.158.0.0/15, 104.16.0.0/13, 104.24.0.0/14, 172.64.0.0/13, 131.0.72.0/22 } tcp dport { 80, 443 } accept
        ip6 saddr { 2400:cb00::/32, 2606:4700::/32, 2803:f800::/32, 2405:b500::/32, 2405:8100::/32, 2a06:98c0::/29, 2c0f:f248::/32 } tcp dport { 80, 443 } accept
      '';

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

      systemd.services.cloudflare-ips-refresh = {
        description = "Refresh Cloudflare edge IP ranges (nginx real-ip include)";
        after = [ "network-online.target" "cloudflare-ips-seed.service" ];
        wants = [ "network-online.target" ];
        serviceConfig = {
          Type = "oneshot";
          User = "root";
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

    (lib.mkIf commsEnabled {
      services.nginx.virtualHosts."livekit.${cfg.domain}" = {
        onlySSL = true;
        useACMEHost = cfg.domain;
        extraConfig = ''
          add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
        '';
        locations."/rtc" = {
          proxyPass = "http://127.0.0.1:5880";
          proxyWebsockets = true;
          extraConfig = "proxy_read_timeout 3600s;\nproxy_send_timeout 3600s;";
        };
        locations."/" = { extraConfig = "return 404;"; };
      };

      systemd.services.livekit-rotate = {
        description = "Rotate LiveKit API key + secret";
        serviceConfig = {
          Type = "oneshot";
          User = "root";
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
            systemctl restart livekit.service catalyrst-archipelago.service
            exit 1
          fi

          systemctl restart catalyrst-archipelago.service

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
        };
      };

      # Rust catalyrst-archipelago: clustering + ws-connector + stats in one
      # binary on :5139 (mirrors the reference deployment's unit). Stateless,
      # in-memory, no NATS — the Node archipelago-workers trio (:5000-:5002)
      # and its NATS bus are retired.
      systemd.services.catalyrst-archipelago = {
        description = "catalyrst-archipelago (clustering + ws-connector + stats, port 5139)";
        wantedBy = [ "multi-user.target" ];
        after = [ "livekit.service" ]; wants = [ "livekit.service" ];
        environment = {
          HTTP_SERVER_PORT = "5139"; HTTP_SERVER_HOST = "127.0.0.1";
          LIVEKIT_WS_URL = "wss://livekit.${cfg.domain}";
          COMMS_GATEKEEPER_URL = cfg.commsGatekeeperUrl;
          RUST_LOG = "catalyrst_archipelago=info,tower_http=info";
        };
        serviceConfig = noPgSandbox // {
          LoadCredential = "livekit-env:/var/lib/secrets/livekit-api.env";
          ExecStart = pkgs.writeShellScript "catalyrst-archipelago-launcher" ''
            set -a
            . "$CREDENTIALS_DIRECTORY/livekit-env"
            set +a
            exec ${cfg.commsPackages.catalyrst-archipelago}/bin/catalyrst-archipelago
          '';
          DynamicUser = true;
          Restart = "always"; RestartSec = 10;
          MemoryMax = "1G";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:5139" ];
          SocketBindDeny = "any";
          IPAddressAllow = [ "localhost" "104.16.0.0/13" "172.64.0.0/13" ];
          IPAddressDeny = "any";
        };
      };

      systemd.services.pulse = {
        description = "Pulse authoritative comms server (Rust, ENet/UDP)";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ]; wants = [ "network-online.target" ];
        environment = {
          RUST_LOG = "info";
          PULSE_BIND = "0.0.0.0:7777";
          # Must stay equal to the `pulse` scrape target below: pulse refuses to start if it
          # cannot bind this, so a mismatch is a boot failure rather than a silent 0 for
          # up{job="pulse"} poisoning the shared ServiceDown alert.
          PULSE_METRICS_BIND = "127.0.0.1:5005";
        };
        serviceConfig = noPgSandbox // {
          ExecStart = "${cfg.commsPackages.pulse}/bin/catalyrst-pulse";
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
