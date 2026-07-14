# nginx building blocks shared by web.nix and web-gateway.nix. A plain
# function (helpers.nix-style), not a module: both consumers import it so the
# header/location fragments stay byte-identical across the vhost files.
{ cfg, lib }:
let
  inherit (cfg) domain;
  isPublic = cfg.exposure == "public";

  secHeaders =
    if isPublic then
      ''
        add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
        add_header X-Frame-Options "SAMEORIGIN" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header Referrer-Policy "strict-origin-when-cross-origin" always;
        add_header Permissions-Policy "interest-cohort=()" always;
        add_header Content-Security-Policy "default-src 'self'; connect-src 'self' https://auth-api.${domain}; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'self'; base-uri 'self'" always;
        add_header Cross-Origin-Opener-Policy "same-origin" always;
        add_header Cross-Origin-Resource-Policy "same-site" always;
      ''
    else
      ''
        add_header X-Frame-Options "SAMEORIGIN" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header Referrer-Policy "strict-origin-when-cross-origin" always;
        add_header Permissions-Policy "interest-cohort=()" always;
        add_header Content-Security-Policy "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'self'; base-uri 'self'" always;
        add_header Cross-Origin-Opener-Policy "same-origin" always;
        add_header Cross-Origin-Resource-Policy "same-site" always;
      '';

  corsFallback = ''
    add_header Access-Control-Allow-Origin $cors_fallback_acao always;
  '';

  protectedStorage = {
    extraConfig = ''
      internal;
      alias ${cfg.stateDir}/content_rust/contents/;
      ${secHeaders}
      add_header Access-Control-Allow-Origin "*" always;
      add_header Cache-Control "public, max-age=31536000, immutable";
      etag off;
      add_header ETag $upstream_http_etag always;
      sendfile on;
      tcp_nopush on;
      aio threads;
      output_buffers 1 256k;
    '';
  };

  contentReadLocations = lib.optionalAttrs cfg.edge.contentReadLimit.enable (
    lib.genAttrs [ "/content/" "/lambdas/" ] (_: {
      proxyPass = "http://127.0.0.1:5141";
      extraConfig = ''
        proxy_read_timeout 600s;
        proxy_buffering off;
        limit_req zone=catcontent burst=${toString cfg.edge.contentReadLimit.burst} nodelay;
      '';
    })
  );
in
{
  inherit
    isPublic
    secHeaders
    corsFallback
    protectedStorage
    contentReadLocations
    ;
}
