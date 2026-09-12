{
  config,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  isPublic = cfg.exposure == "public";
in
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
