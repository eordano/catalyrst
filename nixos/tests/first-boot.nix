# Boots the module in the public-gateway shape and asserts the five first-boot
# fixes that evaluation cannot see (07bc77f35): postgres readiness ordering,
# livekit secret generation, the singles package fallback, the /private/dumps
# gate, and the opensea egress deny. Packages are stubs -- this exercises the
# MODULE (units, ordering, secrets, sandbox, nginx render), not the binaries,
# which carry their own cargo tests. Runs in CI / on a KVM host; the VM boot
# needs /dev/kvm.
{ pkgs, self, ... }:
let
  system = "x86_64-linux";

  # One executable standing in for every catalyrst binary a unit execs. The
  # long-running services sleep; squid-migrate is the lone oneshot and exits 0.
  # content-migrate reads share/catalyrst-server/migrations/*.sql, so ship one.
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

  # Stands in for inputs.catalyrst: the module reads .packages.x86_64-linux.*
  # directly and .shortRev via `or`, nothing else.
  fakeCatalyrst = {
    packages.${system} = stubPackages;
    shortRev = "test000";
  };
in
pkgs.testers.runNixOSTest {
  name = "catalyrst-module-first-boot";
  node.specialArgs.inputs.catalyrst = fakeCatalyrst;

  nodes.machine =
    { lib, ... }:
    {
      imports = [ self.nixosModules.catalyrst ];

      virtualisation.memorySize = 4096;
      virtualisation.diskSize = 6144;
      environment.systemPackages = [
        pkgs.nginx
        pkgs.postgresql_18
        pkgs.curl
      ];

      # The gateway's opensea fallback statically proxies opensea.decentraland.org,
      # which nginx resolves at config-load; the hermetic VM has no external DNS
      # (prod does), so point it at loopback to let nginx start.
      networking.extraHosts = "127.0.0.1 opensea.decentraland.org";

      services.catalyrst = {
        enable = true;
        profile = "public-gateway";
        domain = "test.local";
        # acme-http01 rides nixpkgs' preliminary self-signed cert so nginx
        # starts; the default acme-dns01 needs a real DNS provider the hermetic
        # VM has none of. http01 also exercises the multi-SAN vhost path.
        tls = "acme-http01";
        # The LOD seed preStart builds an npm package from GitHub; irrelevant to
        # the first-boot fixes and would need network in the VM.
        subServices.abCdn = false;
      };

      # squid.env is an operator secret with no auto-mint (it carries RPC
      # endpoints); provide a dummy so the squid units start and their
      # postgres-setup ordering is genuinely exercised.
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
