{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  commsPackages = inputs.catalyrst.packages.x86_64-linux;

  pulseImage = pkgs.dockerTools.buildLayeredImage {
    name = "catalyrst-pulse";
    tag = "latest";
    config = {
      Cmd = [ "${commsPackages.pulse}/bin/catalyrst-pulse" ];
    };
  };
in
lib.mkIf (cfg.enable && cfg.subServices.comms && cfg.pulse.sandbox) {
  virtualisation.podman.enable = true;
  virtualisation.containers.containersConf.settings.engine.runtimes.runsc-host = [
    "${pkgs.writeShellScript "runsc-host" ''exec ${pkgs.gvisor}/bin/runsc --network=host "$@"''}"
  ];

  systemd.services.podman-pulse = {
    after = [ "nats.service" ];
    wants = [ "nats.service" ];
  };

  virtualisation.oci-containers.containers.pulse = {
    image = "catalyrst-pulse:latest";
    imageFile = pulseImage;
    environment = {
      RUST_LOG = "info";
      PULSE_BIND = "${cfg.pulse.bindAddress}:${toString cfg.pulse.port}";
      PULSE_NATS_URL = "nats://127.0.0.1:4222";
    };
    extraOptions = [
      "--runtime=runsc-host"
      "--network=host"
      "--memory=${cfg.resources.pulseMemoryMax}"
      "--memory-reservation=${cfg.resources.pulseMemoryHigh}"
    ];
  };
}
