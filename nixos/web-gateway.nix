# The gateway.decentraland.org-shaped surface: gateway.<domain> plus the
# per-service subdomains the unity-explorer --base-domain flag fans onto.
# Everything here is gated on gateway.enable; the base edge lives in web.nix.
{
  config,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  inherit (cfg) domain;
  w = import ./web-lib.nix { inherit cfg lib; };
  inherit (w)
    isPublic
    secHeaders
    corsFallback
    protectedStorage
    contentReadLocations
    ;

  # The SANs this file's vhosts answer on, appended to web.nix's domain cert
  # through security.acme's list merge so each name stays next to the vhost
  # (and the gate) that serves it. acme-http01 only: the acme-dns01 branch's
  # "*.<domain>" wildcard already covers every subdomain.
  gatewaySans = [
    "gateway.${domain}"
    "peer.${domain}"
    "asset-bundle-registry.${domain}"
    "opensea.${domain}"
    "realm-provider-ea.${domain}"
    "auth-api.${domain}"
    "places.${domain}"
    "api.${domain}"
    "archipelago-ea-stats.${domain}"
    "badges.${domain}"
    "notifications.${domain}"
    "autotranslate-server.${domain}"
    "assets-cdn.${domain}"
    "metamorph-api.${domain}"
    "camera-reel-service.${domain}"
    "credits.${domain}"
    "ab-cdn.${domain}"
    "profile-images.${domain}"
  ]
  ++ lib.optionals (cfg.gateway.featureFlags && cfg.subServices.explorerApi) [
    "feature-flags.${domain}"
    "config.${domain}"
  ]
  ++ lib.optionals cfg.subServices.explore [
    "dcl-lists.${domain}"
    "events.${domain}"
    "worlds-content-server.${domain}"
  ]
  ++ lib.optionals cfg.subServices.social [
    "comms-gatekeeper.${domain}"
    "social-api.${domain}"
  ]
  ++ lib.optionals cfg.subServices.data [ "rpc.${domain}" ]
  ++ lib.optionals cfg.subServices.socialRpc [ "rpc-social-service-ea.${domain}" ];

  gwStrip = port: prefix: {
    proxyPass = "http://127.0.0.1:${toString port}";
    extraConfig = ''
      rewrite ^${prefix}/?(.*)$ /$1 break;
    '';
  };
  gwContent = {
    proxyPass = "http://127.0.0.1:5141";
    extraConfig = ''
      proxy_read_timeout 600s;
      proxy_buffering off;
    '';
  };
  gatewayLocations = {
    "= /" = {
      extraConfig = ''
        default_type text/plain;
        return 400 "Missing service prefix";
      '';
    };
    "/about" = gwContent;
    "/content/" = gwContent;
    "/lambdas/" = gwContent;
    "/entities/" = gwContent;
    "/__protected_storage/" = protectedStorage;
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
    "= /ws" = {
      proxyPass = "http://127.0.0.1:5139";
      proxyWebsockets = true;
      extraConfig = ''
        proxy_read_timeout 3600s;
        limit_conn catws 8;
      '';
    };

    "/places" = gwStrip 5143 "/places";
    "/api" = gwStrip 5143 "/api";
    "/archipelago-ea-stats" = gwStrip 5143 "/archipelago-ea-stats";
    "/worlds-content-server" = gwStrip 5143 "/worlds-content-server";

    "/asset-bundle-registry" = gwStrip 5147 "/asset-bundle-registry";
    "/ab-cdn" = {
      proxyPass = "http://127.0.0.1:5147";
      extraConfig = ''
        proxy_read_timeout 900s;
        proxy_buffering off;
        rewrite ^/ab-cdn/(v\d+)/assets/(.*)$ /ab-cdn/$1/$2;
        rewrite ^/ab-cdn/?(.*)$ /$1 break;
      '';
    };

    "/camera-reel-service" = gwStrip 5144 "/camera-reel-service";

    "/social-api" = gwStrip 5145 "/social-api";
    "/comms-gatekeeper" = gwStrip 5145 "/comms-gatekeeper";
    "/notifications" = gwStrip 5145 "/notifications";
    "/badges" = gwStrip 5145 "/badges";
    "/metamorph-api" = gwStrip 5145 "/metamorph-api";
    "/assets-cdn" = gwStrip 5145 "/assets-cdn";

    "/credits" = gwStrip 5146 "/credits";

    "= /metrics" = {
      extraConfig = "return 404;";
    };
    "/admin" = {
      extraConfig = "return 404;";
    };
    "= /debug" = {
      extraConfig = "return 404;";
    };
    "/" = {
      extraConfig = "return 404;";
    };
  }
  // lib.optionalAttrs cfg.subServices.explorerApi {
    "/realm-provider-ea" = gwStrip 5137 "/realm-provider-ea";
    "/auth-api" = gwStrip 5137 "/auth-api";
  }
  // lib.optionalAttrs cfg.subServices.profileImages {
    "/profile-images" = gwStrip 5161 "/profile-images";
  };

  fanOutHosts =
    lib.mapAttrs'
      (
        sub: port:
        lib.nameValuePair "${sub}.${domain}" {
          forceSSL = true;
          useACMEHost = domain;
          extraConfig = ''
            ${secHeaders}
            ${corsFallback}
            client_max_body_size 1m;
            limit_req zone=catread burst=60 nodelay;
          '';
          locations = {
            "/" = {
              proxyPass = "http://127.0.0.1:${toString port}${lib.optionalString (sub == "auth-api") "/auth/"}";
            }
            // lib.optionalAttrs (sub == "ab-cdn") {
              extraConfig = ''
                proxy_read_timeout 900s;
                proxy_buffering off;
                rewrite ^/(v\d+)/assets/(.*)$ /$1/$2 break;
              '';
            }
            // lib.optionalAttrs (sub == "auth-api") {
              extraConfig = ''
                if ($request_method = OPTIONS) {
                  ${secHeaders}
                  add_header Access-Control-Allow-Origin $cors_reflect_acao always;
                  add_header Access-Control-Allow-Methods "*" always;
                  add_header Access-Control-Allow-Headers $http_access_control_request_headers always;
                  add_header Access-Control-Max-Age 3600 always;
                  add_header Vary "Access-Control-Request-Headers" always;
                  return 204;
                }
              '';
            };
          }
          // lib.optionalAttrs (sub == "auth-api") {
            "= /health" = {
              proxyPass = "http://127.0.0.1:${toString port}/health";
            };
          };
        }
      )
      {
        realm-provider-ea = 5137;
        auth-api = 5137;
        places = 5143;
        api = 5143;
        archipelago-ea-stats = 5143;
        badges = 5145;
        notifications = 5145;
        autotranslate-server = 5145;
        assets-cdn = 5145;
        metamorph-api = 5145;
        camera-reel-service = 5144;
        credits = 5146;
        ab-cdn = 5147;
        profile-images = 5161;
      };
in
lib.mkIf (cfg.enable && isPublic && cfg.gateway.enable) {
  security.acme.certs.${domain}.extraDomainNames = lib.mkIf (cfg.tls == "acme-http01") gatewaySans;

  services.nginx.proxyCachePath.opensea = {
    enable = true;
    keysZoneName = "opensea";
    keysZoneSize = "10m";
    maxSize = "2g";
    inactive = "30d";
  };

  services.nginx.virtualHosts = {
    "gateway.${domain}" = {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        proxy_hide_header X-Content-Type-Options;
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations = gatewayLocations;
    };

    "feature-flags.${domain}" = lib.mkIf (cfg.gateway.featureFlags && cfg.subServices.explorerApi) {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."= /explorer.json" = {
        proxyPass = "http://127.0.0.1:5137";
        extraConfig = ''
          if ($request_method = OPTIONS) {
            ${secHeaders}
            add_header Access-Control-Allow-Origin $cors_reflect_acao always;
            add_header Access-Control-Allow-Credentials "true" always;
            add_header Access-Control-Allow-Headers "X-Debug,Cookie,X-Address-Hash,Origin" always;
            add_header Access-Control-Allow-Methods "GET" always;
            add_header Access-Control-Expose-Headers "ETag,Set-Cookie" always;
            add_header Access-Control-Max-Age 86000 always;
            add_header Vary "Origin" always;
            return 204;
          }
          ${secHeaders}
          add_header Access-Control-Allow-Origin $cors_reflect_acao always;
          add_header Access-Control-Allow-Credentials "true" always;
          add_header Access-Control-Expose-Headers "ETag,Set-Cookie" always;
          add_header Vary "Origin" always;
          proxy_hide_header Access-Control-Allow-Origin;
        '';
      };
      locations."= /health" = {
        extraConfig = ''
          default_type application/json;
          return 200 '{"status":"ok"}';
        '';
      };
      locations."/" = {
        extraConfig = ''
          default_type text/plain;
          return 404 "Not found";
        '';
      };
    };

    "peer.${domain}" = {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        proxy_hide_header X-Content-Type-Options;
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations = {
        "/about" = gwContent;
        "/content/" = gwContent;
        "/lambdas/" = gwContent;
        "/entities/" = gwContent;
        "/__protected_storage/" = protectedStorage;
        "/" = {
          extraConfig = ''
            default_type application/octet-stream;
            return 404 '{ "ok": false, "error": "404 path not found" }';
          '';
        };
      }
      // contentReadLocations;
    };

    "asset-bundle-registry.${domain}" = {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."= /health" = {
        extraConfig = ''
          default_type application/json;
          return 200 '{"status":"ok"}';
        '';
      };
      locations."= /status" = {
        extraConfig = ''
          default_type application/json;
          return 200 '{"data":{"version":"2.5.1","currentTime":$catalyrst_epoch_ms,"commitHash":"mirror"}}';
        '';
      };
      locations."/" = {
        proxyPass = "http://127.0.0.1:5147";
        extraConfig = ''
          if ($request_method = OPTIONS) {
            ${secHeaders}
            add_header Access-Control-Allow-Origin "*" always;
            add_header Access-Control-Allow-Methods "GET,HEAD,OPTIONS,DELETE,POST,PUT" always;
            add_header Access-Control-Allow-Headers $http_access_control_request_headers always;
            add_header Access-Control-Max-Age 86400 always;
            add_header Vary "Access-Control-Request-Headers" always;
            return 204;
          }
        '';
      };
    };

    "config.${domain}" = lib.mkIf (cfg.gateway.featureFlags && cfg.subServices.explorerApi) {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5137";
        extraConfig = ''
          if ($request_method = OPTIONS) {
            ${secHeaders}
            add_header Access-Control-Allow-Origin "*" always;
            add_header Access-Control-Allow-Headers "*" always;
            add_header Access-Control-Allow-Methods "GET" always;
            add_header Access-Control-Expose-Headers "ETag" always;
            add_header Access-Control-Max-Age 86000 always;
            return 204;
          }
        '';
      };
    };

    "dcl-lists.${domain}" = lib.mkIf cfg.subServices.explore {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5143";
      };
    };

    "events.${domain}" = lib.mkIf cfg.subServices.explore {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5143";
      };
    };

    "worlds-content-server.${domain}" = lib.mkIf cfg.subServices.explore {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5143";
      };
    };

    "comms-gatekeeper.${domain}" = lib.mkIf cfg.subServices.social {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5145";
      };
    };

    "social-api.${domain}" = lib.mkIf cfg.subServices.social {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5145";
      };
    };

    "rpc.${domain}" = lib.mkIf cfg.subServices.data {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."= /" = {
        extraConfig = ''
          default_type application/json;
          return 200 '{"mainnet":"https://rpc.${domain}/mainnet","sepolia":"https://rpc.${domain}/sepolia","polygon":"https://rpc.${domain}/polygon","amoy":"https://rpc.${domain}/amoy"}';
        '';
      };
      locations."/" = {
        proxyPass = "http://127.0.0.1:5146";
        proxyWebsockets = true;
        extraConfig = ''
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
    };

    "rpc-social-service-ea.${domain}" = lib.mkIf cfg.subServices.socialRpc {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5148";
        proxyWebsockets = true;
        extraConfig = ''
          proxy_read_timeout 3600s;
          limit_conn catws 8;
        '';
      };
    };

    "opensea.${domain}" = {
      forceSSL = true;
      useACMEHost = domain;
      extraConfig = ''
        ${secHeaders}
        ${corsFallback}
        client_max_body_size 1m;
        limit_req zone=catread burst=60 nodelay;
      '';
      locations."/" = {
        proxyPass = "http://127.0.0.1:5162";
        extraConfig = ''
          proxy_cache opensea;
          proxy_cache_key $request_uri;
          proxy_cache_valid 200 30d;
          proxy_ignore_headers Cache-Control Expires Set-Cookie;
          proxy_cache_use_stale error timeout updating http_500 http_503;
          proxy_read_timeout 90s;
          proxy_intercept_errors on;
          error_page 404 500 502 504 = @dcl_opensea;
        '';
      };
      locations."@dcl_opensea" = {
        proxyPass = "https://opensea.decentraland.org";
        extraConfig = ''
          proxy_set_header Host opensea.decentraland.org;
          proxy_ssl_server_name on;
          proxy_ssl_name opensea.decentraland.org;
          proxy_cache opensea;
          proxy_cache_key "dcl:$request_uri";
          proxy_cache_valid 200 30d;
          proxy_cache_valid 404 5m;
          proxy_ignore_headers Cache-Control Expires Set-Cookie;
          proxy_cache_use_stale error timeout updating http_500 http_502 http_503 http_504;
        '';
      };
    };
  }
  // fanOutHosts;
}
