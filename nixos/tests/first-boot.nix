{ pkgs, self, ... }:
let
  system = "x86_64-linux";

  stub = pkgs.runCommand "catalyrst-stub" { } ''
    mkdir -p $out/bin $out/share/catalyrst-server/migrations
    for b in catalyrst-live catalyrst-explore catalyrst-create catalyrst-social \
             catalyrst-data catalyrst-social-rpc catalyrst-explorer-api \
             catalyrst-telemetry catalyrst-world-storage catalyrst-profile-images \
             catalyrst-signatures catalyrst-scene-state catalyrst-land-authz-index \
             catalyrst-governance catalyrst-presence abgen \
             catalyrst-archipelago catalyrst-pulse squid-eth squid-polygon \
             sites-server; do
      printf '#!/bin/sh\nexec sleep infinity\n' > "$out/bin/$b"
    done
    printf '#!/bin/sh\nexit 0\n' > "$out/bin/squid-migrate"
    chmod +x $out/bin/*
    printf -- '-- stub migration\nSELECT 1;\n' \
      > $out/share/catalyrst-server/migrations/0001_stub.sql
  '';

  stubPackages = builtins.listToAttrs (
    map
      (n: {
        name = n;
        value = stub;
      })
      [
        "catalyrst"
        "catalyrst-all"
        "abgen"
        "catalyrst-archipelago"
        "pulse"
        "catalyrst-scene-state"
        "catalyrst-governance"
        "catalyrst-presence"
        "squid"
        "sites"
      ]
  );

  fakeCatalyrst = {
    packages.${system} = stubPackages;
    shortRev = "test000";
  };
  routingBackend = pkgs.writeText "routing-backend.py" ''
    import http.server
    import sys

    port = int(sys.argv[1])

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            payload = f"{port}:{self.path}".encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
  '';
  dappsFlags = pkgs.writeText "test-dapps.json" (
    builtins.toJSON {
      flags.dapps-migration-test = true;
      variants = { };
    }
  );
in
pkgs.testers.runNixOSTest {
  name = "catalyrst-module-first-boot";
  node.specialArgs.inputs.catalyrst = fakeCatalyrst;

  nodes.machine =
    { lib, ... }:
    {
      imports = [
        self.nixosModules.catalyrst
        ({ config, lib, ... }: {
          options.services.nginx.customRecommendedTlsSettings = lib.mkOption {
            type = lib.types.bool;
            default = true;
          };
          config.services.nginx.appendHttpConfig = lib.mkIf config.services.nginx.customRecommendedTlsSettings "ssl_session_timeout 1d;";
        })
      ];

      virtualisation.memorySize = 4096;
      virtualisation.diskSize = 6144;
      environment.systemPackages = [
        pkgs.nginx
        pkgs.postgresql_18
        pkgs.curl
      ];

      networking.extraHosts = "127.0.0.1 opensea.decentraland.org";

      services.catalyrst = {
        enable = true;
        profile = "public-gateway";
        domain = "test.local";
        tls = "acme-http01";
        subServices.abCdn = false;
        postflight.enable = false;
        gateway.dappsFlagsFile = dappsFlags;
      };

      systemd.services =
        lib.mapAttrs
          (_: port: {
            serviceConfig.ExecStart = lib.mkForce "${pkgs.python3}/bin/python ${routingBackend} ${toString port}";
          })
          {
            catalyrst-sites = 5158;
            catalyrst-explore = 5143;
            catalyrst-social = 5145;
            catalyrst-explorer-api = 5137;
          };

      system.activationScripts.testSquidEnv = ''
        mkdir -p /var/lib/secrets
        cat > /var/lib/secrets/squid.env <<'EOF'
        RPC_ENDPOINT_ETH=http://127.0.0.1:9
        RPC_ENDPOINT_POLYGON=http://127.0.0.1:9
        SQD_PORTAL_API_KEY=test-portal-key
        DB_HOST=/run/postgresql
        DB_NAME=marketplace_squid
        DB_USER=squid
        DB_SCHEMA=squid_marketplace
        EOF
        chmod 600 /var/lib/secrets/squid.env
      '';
    };

  testScript = ''
    import json

    machine.wait_for_unit("multi-user.target")

    # P0 #2 -- postgres readiness ordering. These DDL oneshots run psql against
    # named databases/roles; reaching active means they ran AFTER
    # postgresql-setup created them, not racing it.
    machine.wait_for_unit("postgresql.service")
    machine.wait_for_unit("postgresql-ownership.service")
    machine.wait_for_unit("postgresql-bundles.service")
    machine.wait_for_unit("catalyrst-content-migrate.service")
    machine.wait_for_unit("squid-search-path.service")
    dbs = machine.succeed("sudo -u postgres psql -Atl | cut -d'|' -f1")
    assert "content" in dbs, "content DB missing"
    assert "marketplace_squid" in dbs, "marketplace_squid DB missing"

    # P0 #1 -- livekit secrets auto-minted (no unit generated them before this fix;
    # the rotate timer only rotated an existing pair).
    machine.wait_for_unit("livekit-secret.service")
    machine.succeed("test -s /var/lib/secrets/livekit.yaml")
    machine.succeed("test -s /var/lib/secrets/livekit-api.env")
    machine.succeed("grep -q '^keys:' /var/lib/secrets/livekit.yaml")
    # sibling auto-minted secrets from the same pattern
    machine.succeed("test -s /var/lib/secrets/catalyrst-admin.env")
    machine.succeed("test -s /var/lib/secrets/catalyrst-world-storage.env")

    # P0 #3 -- the default profile resolves governance/presence from the flake's
    # own builds with NO package option set (previously three hard assertions).
    machine.wait_for_unit("catalyrst-governance.service")
    machine.wait_for_unit("catalyrst-presence.service")

    # The /server operator UI (sites tier) is now seeded on for public-gateway
    # and its package falls back to the flake's own build.
    machine.wait_for_unit("catalyrst-sites.service")

    # libretranslate downloads its argos models at startup (updateModels) so a
    # fresh box does not crash on an empty language list. The download needs
    # network the hermetic VM lacks, so assert the flag is wired, not a live run.
    machine.succeed(
        "systemctl show libretranslate.service -p ExecStart | grep -q -- '--update-models'"
    )

    # nginx config VALIDATED -- it will not start on a bad config, so reaching
    # active is `nginx -t` passing on the full rendered vhost set.
    machine.wait_for_unit("nginx.service")

    for port in (5158, 5143, 5145, 5137):
        machine.wait_for_open_port(port)

    def request(host, path):
        return machine.succeed(
            f"curl --fail --silent --insecure --resolve {host}:443:127.0.0.1 https://{host}{path}"
        )

    assert request("test.local", "/places") == "5158:/places"
    assert request("test.local", "/places/example") == "5158:/places/example"
    assert request("test.local", "/places/api/places") == "5143:/api/places"
    assert request("test.local", "/media/convert?width=640") == "5145:/media/convert?width=640"
    assert request("gateway.test.local", "/auth-api/requests/example") == "5137:/auth/requests/example"
    assert request("auth-api.test.local", "/requests/example") == "5137:/auth/requests/example"
    flags = json.loads(request("feature-flags.test.local", "/dapps.json"))
    assert flags == {"flags": {"dapps-migration-test": True}, "variants": {}}

    # P0 #4 -- /private/dumps carries the superadmin deny in the live config.
    # Read the config the service actually loaded (its ExecStart -c path), not
    # `nginx -T`, which a bare nginx resolves against the default config path.
    nginx_conf = machine.succeed(
        "systemctl show nginx.service -p ExecStart --value "
        "| grep -oP '/nix/store/\\S+nginx\\.conf' | head -1"
    ).strip()
    machine.succeed(
        f"grep -A20 'location /private/dumps/' {nginx_conf} | grep -q 'deny all'"
    )

    # P0 #5 -- the opensea resolver still denies egress to the link-local
    # metadata range (SSRF containment) AND is now reachable on loopback:
    # systemd IP filters are bidirectional, so the earlier blanket deny also
    # dropped nginx's ingress. localhost must be allowed; the app-level URL
    # guard keeps the SSRF closed.
    machine.succeed(
        "systemctl show catalyrst-opensea-resolver -p IPAddressDeny | grep -q 169.254"
    )
    # systemctl renders `localhost` as its resolved CIDRs.
    machine.succeed(
        "systemctl show catalyrst-opensea-resolver -p IPAddressAllow | grep -q '127.0.0.0/8'"
    )
    machine.wait_for_unit("catalyrst-opensea-resolver.service")
    machine.wait_until_succeeds(
        "curl -sf -m 5 http://127.0.0.1:5162/health | grep -q '\"ok\":true'", timeout=30
    )

    # squid processors pin their Prometheus port to the SocketBindAllow value,
    # so the documented minimal squid.env no longer crash-loops on the default
    # 0.0.0.0:3000.
    machine.succeed(
        "systemctl show squid-eth -p Environment | grep -q ETH_PROMETHEUS_PORT=5131"
    )
  '';
}
