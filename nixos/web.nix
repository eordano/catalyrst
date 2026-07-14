{
  config,
  pkgs,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  inherit (cfg) domain;
  escapedDomain = lib.replaceStrings [ "." ] [ "\\." ] domain;
  # lego reads per-provider tuning from env vars under a provider prefix;
  # listed here are the providers whose prefix is not the uppercased name.
  legoEnvPrefix =
    {
      digitalocean = "DO";
      route53 = "AWS";
      gcloud = "GCE";
    }
    .${cfg.dnsProvider} or (lib.toUpper cfg.dnsProvider);
  w = import ./web-lib.nix { inherit cfg lib; };
  inherit (w)
    isPublic
    secHeaders
    corsFallback
    protectedStorage
    contentReadLocations
    ;

  # The names an acme-http01 cert must cover: every subdomain a vhost could
  # actually serve, mirrored against the same gates those vhosts use below.
  # HTTP-01 cannot issue wildcards, so each SAN must resolve to a live vhost
  # or the challenge fails -- unlike the acme-dns01 branch's single "*.domain".
  # web-gateway.nix appends its subdomains to this same certificate through
  # security.acme's list merge, keeping each SAN next to the vhost serving it.
  acmeHttp01ExtraDomainNames = [
    "www.${domain}"
    "abgen.${domain}"
    "livekit.${domain}"
  ];

  landingRoot = pkgs.runCommand "${cfg.realm}-landing" { } ''
    mkdir -p "$out"
    cp ${./landing/index.html} "$out/index.html"
  '';

  contractsRoot = pkgs.runCommand "${cfg.realm}-contracts" { } ''
    mkdir -p "$out"
    cp ${./contracts/addresses.json} "$out/addresses.json"
  '';

  playRootFor =
    pkg:
    pkgs.runCommandLocal "${cfg.realm}-play-root" { } ''
      cp -r --no-preserve=mode,ownership ${pkg}/. "$out"
      rm -rf "$out"/*.prev-* "$out"/pkg.prev-* "$out"/provenance.prev-*.json
      for f in index.html main.js engine.js pkg/webgpu_build.js \
               pkg/webgpu_build_bg.wasm pkg/webgpu_build_bg.wasm.br \
               pkg/manifest.json assets_bundle.bin assets_bundle.bin.br; do
        test -f "$out/$f" || { echo "play bundle is missing $f" >&2; exit 1; }
      done
      for d in ui3-overlay/chunks vendor assets modules; do
        test -d "$out/$d" || { echo "play bundle is missing directory $d" >&2; exit 1; }
      done
      test ! -e "$out/wasm_worker.js" || {
        echo "play bundle mixes the wasm_worker topology with the ESM engine" >&2
        exit 1
      }
    '';
  playRoot = if cfg.play.package != null then "${playRootFor cfg.play.package}" else cfg.play.dir;

  b = port: { proxyPass = "http://127.0.0.1:${toString port}"; };
  explore = b 5143;
  create = b 5144;
  social = b 5145;
  data = b 5146;
  abcdn = b 5147;
  bundleLocations = {
    "/api/places" = explore;
    "/api/destinations" = explore;
    "/api/map" = explore;
    "/api/report" = explore;
    "/places/api/" = {
      proxyPass = "http://127.0.0.1:5143/api/";
    };
    "/events/api/" = {
      proxyPass = "http://127.0.0.1:5143/api/";
    };
    "/places" = explore;
    "/world_names" = explore;
    "/worlds" = explore;
    "/categories" = explore;
    "/pois" = explore;
    "/api/events" = explore;
    "/api/schedules" = explore;
    "/api/poster" = explore;
    "/api/poster-vertical" = explore;
    "/api/profiles/settings" = explore;
    "/events/" = explore;
    "/v1/map.png" = explore;
    "/v1/minimap.png" = explore;
    "/v1/estatemap.png" = explore;
    "/v1/tiles" = explore;
    "/v2/" = explore;
    "/world/" = explore;
    "/wallet/" = explore;
    "/contents/" = explore;
    "/v1/newsletter" = create;
    "/v1/collections/" = create;
    "/v1/storage/" = create;
    "/images" = create;
    "/users" = create;
    "/profiles" = create;
    "/registry" = create;
    "/denylist" = create;
    "/queues/" = create;
    "/flush-cache" = create;
    "~ ^/places/[^/]+/images" = create;
    "/v1/communities" = social;
    "/v1/members" = social;
    "/v1/community-voice-chats" = social;
    "/v1/moderation/" = social;
    "/federation/communities" = social;
    "/social/communities" = social;
    "/get-scene-adapter" = social;
    "/get-server-scene-adapter" = social;
    "/scene-participants" = social;
    "/scene-bans/" = social;
    "/private-messages/token" = social;
    "/community-voice-chat" = social;
    "/cast/" = social;
    "= /bans" = social;
    "= /livekit-webhook" = social;
    "/notifications" = social;
    "/subscription" = social;
    "/set-email" = social;
    "/confirm-email" = social;
    "/badges/" = social;
    "/translate" = social;
    "= /market/v1/catalog" = {
      proxyPass = "http://127.0.0.1:5146/v1/catalog";
    };
    "= /market/v2/catalog" = {
      proxyPass = "http://127.0.0.1:5146/v2/catalog";
    };
    "/v1/catalog" = data;
    "/v2/catalog" = data;
    "/v1/items" = data;
    "/v1/nfts" = data;
    "/v1/orders" = data;
    "/v1/bids" = data;
    "/v1/sales" = data;
    "/v1/trades" = data;
    "/v1/accounts" = data;
    "/v1/activity" = data;
    "/v1/contracts" = data;
    "/v1/owners" = data;
    "/v1/prices" = data;
    "/v1/trendings" = data;
    "/v1/volume" = data;
    "/v1/transactions" = data;
    "/users/" = data;
    "/seasons" = data;
    "= /captcha" = data;
    "/api/v3/simple/price" = data;
    "~ ^/(mainnet|sepolia|polygon|amoy|ethereum)$" = data;
    "/LOD/" = abcdn // {
      extraConfig = "proxy_read_timeout 660s;";
    };
    "/manifest/" = abcdn;
  };

  superadminGate = ''
    ${lib.concatMapStringsSep "\n    " (c: "allow ${c};") cfg.edge.superadminCidrs}
    deny all;
  '';

  singleLocations =
    lib.optionalAttrs cfg.subServices.worldStorage {
      "/world-storage/" = {
        proxyPass = "http://127.0.0.1:5154/";
        extraConfig = "proxy_set_header x-original-path $request_uri;";
      };
    }
    // lib.optionalAttrs cfg.subServices.profileImages {
      "/profile-images/" = {
        proxyPass = "http://127.0.0.1:5161/";
      };
    }
    // lib.optionalAttrs cfg.subServices.telemetry {
      "= /telemetry/dash/sql" = {
        proxyPass = "http://127.0.0.1:5150/dash/sql";
        extraConfig = superadminGate;
      };
      "= /telemetry/dash/flag" = {
        proxyPass = "http://127.0.0.1:5150/dash/flag";
        extraConfig = superadminGate;
      };
      "= /telemetry/dash/group/target" = {
        proxyPass = "http://127.0.0.1:5150/dash/group/target";
        extraConfig = superadminGate;
      };
      "/telemetry/dash/admin/" = {
        proxyPass = "http://127.0.0.1:5150/dash/admin/";
        extraConfig = superadminGate;
      };
      "/telemetry/" = {
        proxyPass = "http://127.0.0.1:5150/";
      };
    }
    // lib.optionalAttrs cfg.subServices.governance {
      "/governance-api/" = {
        proxyPass = "http://127.0.0.1:5151/";
      };
    }
    // lib.optionalAttrs cfg.subServices.presence {
      "/presence/" = {
        proxyPass = "http://127.0.0.1:5152/";
      };
    };

  sitesLoc = {
    proxyPass = "http://127.0.0.1:5158";
    extraConfig = "proxy_buffering off;";
  };
  sitesLocations = lib.optionalAttrs cfg.subServices.sites {
    "/assets/" = sitesLoc;
    "= /favicon.ico" = sitesLoc;
    "= /content" = sitesLoc;
    "/marketplace" = sitesLoc;
    "/governance" = sitesLoc;
    "/creator-hub" = sitesLoc;
    "/api/creator-hub" = sitesLoc;
    "/api/governance" = sitesLoc;
    "/auth" = sitesLoc;
    "/connect" = sitesLoc;
    "/internal" = sitesLoc;
    "/client" = sitesLoc;
    "/bevy-overlay" = sitesLoc;
    "/discover" = sitesLoc;
    "/download" = sitesLoc;
    "/create" = sitesLoc;
    "/builder" = sitesLoc;
    "/blog" = sitesLoc;
    "/dao" = sitesLoc;
    "/shop" = sitesLoc;
    "/support" = sitesLoc;
    "/whats-on" = sitesLoc;
    "/for" = sitesLoc;
    "/explorer" = sitesLoc;
    "/legal" = sitesLoc;
    "/landings" = sitesLoc;
    "/operator" = sitesLoc;
  };

  playCoep = ''
    ${lib.optionalString isPublic ''add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;''}
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
    add_header Content-Security-Policy "default-src 'self' https: data: blob:; script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'; connect-src 'self' https: wss: data: blob:; img-src 'self' https: data: blob:; media-src 'self' https: data: blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; frame-ancestors 'self'; base-uri 'self'" always;
    add_header Cross-Origin-Opener-Policy   same-origin    always;
    add_header Cross-Origin-Embedder-Policy credentialless always;
    add_header Cross-Origin-Resource-Policy cross-origin   always;
  '';
  playLocations = lib.optionalAttrs cfg.play.enable {
    "= /play" = {
      extraConfig = "return 308 /play/;";
    };
    "/play/" = {
      alias = "${playRoot}/";
      index = "index.html";
      extraConfig = ''
        brotli_static on;
        disable_symlinks off;
        try_files $uri $uri/ =404;
        # Stable-name artifacts (ui.js, the wasm, assets_bundle.bin): store but
        # revalidate on every use -- anything cacheable-by-name would pin a
        # browser to a dead build. Hashed/pinned assets opt back in below.
        add_header Cache-Control "no-cache" always;
        ${playCoep}
        location ~ ^/play/(?<play_pinned>(?:ui3-overlay/chunks|vendor)/.+)$ {
            alias ${playRoot}/$play_pinned;
            brotli_static on;
            add_header Cache-Control "public, max-age=31536000, immutable";
            ${playCoep}
        }
      '';
    };
  };

  imposterLocations = lib.optionalAttrs cfg.edge.silenceImposterRequests {
    "/bvimposters/" = {
      extraConfig = ''
        access_log off;
        return 404;
      '';
    };
  };

  readLimitKey =
    if cfg.edge.exemptTrustedCidrsFromRateLimit then "$catlimit_key" else "$binary_remote_addr";
  contentReadZone = lib.optionalString cfg.edge.contentReadLimit.enable ''
    limit_req_zone  ${readLimitKey} zone=catcontent:10m rate=${cfg.edge.contentReadLimit.rate};
  '';
  limitExemptConfig = lib.optionalString cfg.edge.exemptTrustedCidrsFromRateLimit ''
    geo $catlimit_exempt {
      default 0;
      ${lib.concatMapStringsSep "\n      " (c: "${c} 1;") cfg.edge.rateLimitExemptCidrs}
    }
    map $catlimit_exempt $catlimit_key {
      0 $binary_remote_addr;
      1 "";
    }
  '';

  mkMainLocations =
    { adminBlock }:
    bundleLocations
    // singleLocations
    // sitesLocations
    // playLocations
    // imposterLocations
    // contentReadLocations
    // {
      "= /" = {
        root = "${landingRoot}";
        extraConfig = "try_files /index.html =404;";
      };
      "= /ui" = {
        extraConfig = "return 301 /ui/;";
      };
      "/ui/" = {
        extraConfig = ''
          alias ${cfg.stateDir}/ui/;
          index index.html;
        '';
      };
      "= /contracts/addresses.json" = {
        root = "${contractsRoot}";
        extraConfig = ''
          try_files /addresses.json =404;
          default_type application/json;
        '';
      };
      "= /metrics" = {
        extraConfig = "return 404;";
      };
      "/admin" =
        if cfg.subServices.sites then
          sitesLoc // { extraConfig = sitesLoc.extraConfig + superadminGate; }
        else
          adminBlock // { extraConfig = (adminBlock.extraConfig or "") + superadminGate; };
      "/server" =
        if cfg.subServices.sites then
          sitesLoc // { extraConfig = sitesLoc.extraConfig + superadminGate; }
        else
          {
            extraConfig = "return 404;";
          };
      "= /debug" = {
        extraConfig = "return 404;";
      };
      "/ws" = {
        proxyPass = "http://127.0.0.1:5139";
        proxyWebsockets = true;
        extraConfig = ''
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
      "/social-rpc" = {
        proxyPass = "http://127.0.0.1:5148";
        proxyWebsockets = true;
        extraConfig = ''
          rewrite ^/social-rpc/?(.*)$ /$1 break;
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
      "/scene-state/admin" = {
        extraConfig = "return 404;";
      };
      "/scene-state" = {
        proxyPass = "http://127.0.0.1:5209";
        proxyWebsockets = true;
        extraConfig = ''
          rewrite ^/scene-state/?(.*)$ /$1 break;
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
      "= /content/entities" = {
        proxyPass = "http://127.0.0.1:5141";
        extraConfig = ''
          proxy_read_timeout 600s;
          proxy_buffering off;
          client_max_body_size 200m;
          client_body_timeout 300s;
          limit_req zone=catdeploy burst=4 nodelay;
        '';
      };
      "/__protected_storage/" = protectedStorage;
      "= /private/dumps" = {
        extraConfig = "return 301 /private/dumps/;";
      };
      "/private/dumps/" = {
        # superadmin-gated like every other operator surface: the name says
        # /private, so it must not be a world-listable autoindex.
        extraConfig = ''
          alias /srv/dumps/;
          autoindex on;
          sendfile on;
          tcp_nopush on;
          aio threads;
          output_buffers 1 256k;
        ''
        + superadminGate;
      };
      "/" = {
        proxyPass = "http://127.0.0.1:5141";
        extraConfig = ''
          proxy_read_timeout 600s;
          proxy_buffering off;
        '';
      };
    };

  sharedHttpConfig =
    limitExemptConfig
    + ''
      limit_req_zone  ${readLimitKey} zone=catread:10m   rate=30r/s;
    ''
    + contentReadZone
    + ''
      limit_req_zone  $binary_remote_addr zone=catdeploy:10m rate=2r/s;
      limit_req_zone  $binary_remote_addr zone=abgenheavy:10m rate=2r/s;
      limit_conn_zone $binary_remote_addr zone=catws:10m;
      limit_req_status 429;
      limit_conn_status 429;
      map $upstream_http_access_control_allow_origin $cors_fallback_acao {
        "" "*";
        default "";
      }
      map $http_origin $cors_reflect_acao {
        default "";
        ~^https://([a-z0-9-]+\.)*decentraland\.(org|zone|today)$ $http_origin;
        ~^https://([a-z0-9-]+\.)*${escapedDomain}$ $http_origin;
      }
      map $msec $catalyrst_epoch_ms {
        ~^(?<epochs>\d+)\.(?<epochms>\d+)$ $epochs$epochms;
      }
    '';
in
lib.mkIf cfg.enable (
  lib.mkMerge [
    {
      services.nginx = {
        enable = true;
        recommendedTlsSettings = true;
        recommendedProxySettings = true;
        recommendedOptimisation = true;
        recommendedGzipSettings = true;
        serverTokens = false;
        commonHttpConfig = sharedHttpConfig;
      };
    }

    {
      assertions =
        let
          mainLocs = mkMainLocations { adminBlock = ""; };
          routeFor = {
            worldStorage = "/world-storage/";
            telemetry = "/telemetry/";
            governance = "/governance-api/";
            presence = "/presence/";
          };
        in
        lib.mapAttrsToList (name: route: {
          assertion = cfg.subServices.${name} == builtins.hasAttr route mainLocs;
          message = "services.catalyrst.subServices.${name} and its nginx route ${route} must be enabled together (feature<->nginx coupling).";
        }) routeFor
        ++ [
          {
            assertion = cfg.play.package == null || cfg.play.enable;
            message = "services.catalyrst.play.package is set but play.enable is false -- the bundle would sit in the closure with no route serving it.";
          }
          {
            assertion = !isPublic || cfg.tls == "acme-http01" || cfg.tls == "acme-dns01";
            message = "services.catalyrst with exposure=public needs an ACME tls mode (acme-http01 or acme-dns01): the public vhosts set useACMEHost, and tls=none defines no certificate for them to consume.";
          }
          {
            assertion =
              !(isPublic && cfg.tls != "none")
              || config.systemd.services."acme-order-renew-${domain}".serviceConfig ? ExecStart;
            message = "the ACME first-issue retry targets acme-order-renew-${domain}, but nixpkgs defines no such unit -- retarget the override in web.nix to the unit that runs lego.";
          }
        ];
    }

    (lib.mkIf cfg.play.enable {
      services.nginx.additionalModules = [ pkgs.nginxModules.brotli ];
      services.nginx.recommendedBrotliSettings = true;
      systemd.tmpfiles.rules = lib.optionals (cfg.play.package == null) [
        "d ${cfg.play.dir} 0755 catalyrst catalyrst -"
      ];
    })

    (lib.mkIf isPublic {
      security.acme = lib.mkIf (cfg.tls == "acme-dns01" || cfg.tls == "acme-http01") {
        acceptTerms = true;
        certs.${domain} =
          if cfg.tls == "acme-dns01" then
            {
              dnsProvider = cfg.dnsProvider;
              environmentFile =
                if cfg.dnsCredentialsFile != "" then cfg.dnsCredentialsFile else "${cfg.secretsDir}/acme-dns.env";
              extraDomainNames = [ "*.${domain}" ];
              webroot = null;
              group = "nginx";
              postRun = "systemctl reload nginx.service || true";
            }
          else
            {
              # acme-http01: with no dnsProvider set, nixpkgs emits the
              # /.well-known/acme-challenge location on every useACMEHost vhost,
              # served from the vhost default acmeRoot (/var/lib/acme/
              # acme-challenge) that this webroot matches -- so each SAN below
              # must resolve to one of those live vhosts. webroot is also the
              # challenge method security.acme's assertion requires.
              extraDomainNames = acmeHttp01ExtraDomainNames;
              webroot = "/var/lib/acme/acme-challenge";
              group = "nginx";
              postRun = "systemctl reload nginx.service || true";
            };
      };

      # nixpkgs runs the order once at boot and then on the daily renew timer,
      # so a failed first order must retry itself or the node serves the
      # placeholder certificate for a day; Restart activates the RestartSec
      # nixpkgs already sets (900 s). DNS providers also publish challenge TXT
      # records slower than lego's 60 s default window; the prefixed env vars
      # widen it, and acme-dns.env still overrides them.
      systemd.services."acme-order-renew-${domain}" =
        lib.mkIf (cfg.tls == "acme-dns01" || cfg.tls == "acme-http01")
          {
            serviceConfig.Restart = "on-failure";
            unitConfig.StartLimitIntervalSec = 0;
            environment = lib.mkIf (cfg.tls == "acme-dns01") {
              "${legoEnvPrefix}_PROPAGATION_TIMEOUT" = "900";
              "${legoEnvPrefix}_POLLING_INTERVAL" = "10";
            };
          };

      services.nginx.virtualHosts.${domain} = {
        serverAliases = [ "www.${domain}" ];
        forceSSL = true;
        useACMEHost = domain;
        extraConfig = ''
          ${secHeaders}
          ${corsFallback}
          client_max_body_size 1m;
          limit_req zone=catread burst=60 nodelay;
        '';
        locations = mkMainLocations {
          adminBlock = {
            extraConfig = "return 404;";
          };
        };
      };

      services.nginx.virtualHosts."abgen.${domain}" = {
        forceSSL = true;
        useACMEHost = domain;
        extraConfig = ''
          add_header Strict-Transport-Security "max-age=63072000; includeSubDomains" always;
          add_header X-Content-Type-Options "nosniff" always;
          ${corsFallback}
        '';
        locations."/manifest/" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."/LOD/" = {
          proxyPass = "http://127.0.0.1:5147";
          extraConfig = "proxy_read_timeout 660s;";
        };
        locations."~ ^/v[0-9]+/" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."/entities/" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."/worlds/" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."/lods-unity/" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."/profiles" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."= /health" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."= /ping" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."= /status" = {
          proxyPass = "http://127.0.0.1:5147";
        };
        locations."= /api/preflight" = {
          extraConfig = "return 404;";
        };
        locations."= /wizard.html" = {
          extraConfig = "return 404;";
        };
        locations."/" = {
          extraConfig = ''
            return 404;
          '';
        };
      };
      services.nginx.virtualHosts."livekit.${domain}" = {
        onlySSL = true;
        useACMEHost = domain;
        extraConfig = ''
          add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
        '';
        locations."/rtc" = {
          proxyPass = "http://127.0.0.1:7880";
          proxyWebsockets = true;
          extraConfig = "proxy_read_timeout 3600s;\nproxy_send_timeout 3600s;";
        };
        locations."/" = {
          extraConfig = "return 404;";
        };
      };

      systemd.tmpfiles.rules = [
        "d /srv/dumps                  0755 root root -"
      ];
    })

    (lib.mkIf (!isPublic) {
      services.nginx.virtualHosts.${domain} = {
        default = true;
        listen = [
          {
            addr = "0.0.0.0";
            port = 80;
          }
        ];
        extraConfig = ''
          ${secHeaders}
          ${corsFallback}
          client_max_body_size 1m;
          limit_req zone=catread burst=60 nodelay;
        '';
        locations = mkMainLocations {
          adminBlock = {
            proxyPass = "http://127.0.0.1:5141";
            extraConfig = ''
              proxy_read_timeout 600s;
              proxy_buffering off;
            '';
          };
        };
      };
      systemd.tmpfiles.rules = [
        "d /srv/dumps                  0755 root root -"
      ];
    })
  ]
)
