{
  config,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  isPublic = cfg.exposure == "public";
in
# Open exactly the node's externally-bound listeners. Everything else binds
# 127.0.0.1 behind nginx, so the edge (80/443) plus the comms UDP/TCP that
# clients reach directly is the whole public surface. Ports mirror facts.nix.
lib.mkIf (cfg.enable && cfg.openFirewall) {
  networking.firewall = {
    allowedTCPPorts = [
      80
    ]
    ++ lib.optionals isPublic [ 443 ]
    ++ lib.optionals cfg.subServices.comms [
      7880
      7881
    ];
    allowedUDPPorts = lib.optionals cfg.subServices.comms [
      7882
      7777
    ];
  };
}
