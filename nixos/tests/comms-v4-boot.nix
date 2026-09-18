{ pkgs, self, ... }:
let
  realm =
    { config, lib, ... }:
    {
      imports = [ self.nixosModules.catalyrst ];

      virtualisation.memorySize = 4096;
      virtualisation.diskSize = 8192;
      virtualisation.cores = 4;
      environment.systemPackages = [
        pkgs.curl
        pkgs.jq
      ];

      networking.extraHosts = "127.0.0.1 realm.test livekit.realm.test";

      services.catalyrst = {
        enable = true;
        profile = "full-realm";
        domain = "realm.test";
        tls = "acme-http01";
        sync.enable = false;
        pulse.sandbox = false;
        livekit.nodeIp = config.networking.primaryIPAddress;
        preflight.strict = false;
        postflight.timeoutSec = 240;
        subServices = {
          explore = false;
          create = false;
          data = false;
          socialRpc = false;
          explorerApi = false;
          worldStorage = false;
          profileImages = false;
          signatures = false;
        };
      };

      systemd.services.catalyrst-sync.environment.COMMS_OFFLINE_WHEN_UNREACHABLE = "false";
    };
in
pkgs.testers.runNixOSTest {
  name = "catalyrst-module-comms-v4-boot";
  node.specialArgs.inputs.catalyrst = self;

  nodes.machine = realm;

  nodes.deaf =
    { lib, ... }:
    {
      imports = [ realm ];
      services.catalyrst.postflight.timeoutSec = lib.mkForce 90;
      systemd.services.catalyrst-social.environment.CLUSTER_SUBSCRIBER_ENABLED = lib.mkForce "false";
    };

  nodes.sandboxed =
    { lib, ... }:
    {
      imports = [ realm ];
      services.catalyrst.pulse.sandbox = lib.mkForce true;
    };

  testScript = ''
    start_all()

    machine.wait_for_unit("catalyrst-postflight.service", timeout=600)
    report = machine.succeed("journalctl --no-pager --output=cat --unit=catalyrst-postflight.service")
    print(report)
    assert "catalyrst-postflight: every check passed" in report, report
    assert "FAILED" not in report, report
    for line in [
        "/about advertises comms.v4 as configured",
        "catalyrst-archipelago.service logged 'control_v4_armed=true'",
        "pulse.service logged 'control position clustering enabled'",
        "pulse.service logged 'durable Pulse handshake replay protection enabled'",
        "catalyrst-social.service logged 'cluster subscriber started'",
        "nginx hands /island-refresh to the social bundle",
        "Pulse listens on udp 7777",
    ]:
        assert line in report, f"postflight never reported: {line}"

    about = machine.succeed("curl --silent http://127.0.0.1:5141/about | jq --compact-output .comms.v4")
    assert '"url":"wss://realm.test/ws/v4"' in about, about
    assert '"audience":"realm.test"' in about, about
    assert '"nativeEndpoint":"pulse-server.realm.test:7777"' in about, about
    assert '"islandRefreshUrl":"https://realm.test/island-refresh"' in about, about

    machine.succeed(
        "sudo -u postgres psql -d comms_control -tAc "
        "\"select to_regclass('archipelago_v4_assignments') is not null\" | grep -qx t"
    )

    machine.succeed("systemctl restart pulse.service")
    machine.wait_for_unit("pulse.service")
    machine.succeed("test -e /var/lib/catalyrst-pulse/replay.tsv")
    machine.succeed("systemctl restart catalyrst-postflight.service")
    again = machine.succeed(
        "journalctl --no-pager --output=cat --invocation=0 --unit=catalyrst-postflight.service"
    )
    assert "catalyrst-postflight: every check passed" in again, again

    machine.succeed("systemctl cat catalyrst-postflight.service | grep -q X-Restart-Triggers")

    sandboxed.wait_for_unit("catalyrst-postflight.service", timeout=900)
    boxed = sandboxed.succeed("journalctl --no-pager --output=cat --unit=catalyrst-postflight.service")
    print(boxed)
    assert "catalyrst-postflight: every check passed" in boxed, boxed
    for line in [
        "podman-pulse.service logged 'durable Pulse handshake replay protection enabled'",
        "podman-pulse.service logged 'control position clustering enabled'",
        "the Pulse replay journal is on the host",
        "Pulse listens on udp 7777",
    ]:
        assert line in boxed, f"postflight never reported: {line}"
    sandboxed.succeed("systemctl restart podman-pulse.service")
    sandboxed.wait_for_unit("podman-pulse.service")
    sandboxed.succeed("test -e /var/lib/catalyrst-pulse/replay.tsv")
    sandboxed.succeed("systemctl restart catalyrst-postflight.service")

    deaf.wait_until_succeeds(
        "systemctl is-failed catalyrst-postflight.service", timeout=900
    )
    refused = deaf.succeed("journalctl --no-pager --output=cat --unit=catalyrst-postflight.service")
    print(refused)
    assert "catalyrst-social.service never logged 'cluster subscriber started'" in refused, refused
    assert "catalyrst-postflight: 2 check(s) failed" in refused, refused
    assert "catalyrst-postflight: every check passed" not in refused, refused
  '';
}
