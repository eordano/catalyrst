{
  config,
  lib,
  ...
}:
let
  cfg = config.services.catalyrst;
  facts = import ./facts.nix;
in
lib.mkIf (cfg.enable && cfg.subServices.social) {
  services.libretranslate = {
    enable = true;
    host = "127.0.0.1";
    port = facts.units.libretranslate.port;
    disableWebUI = true;
    updateModels = true;
    extraArgs = {
      load-only = lib.concatStringsSep "," cfg.translateLanguages;
    };
  };

  assertions = [
    {
      assertion = cfg.translateLanguages != [ ];
      message = "services.catalyrst.translateLanguages must not be empty -- LibreTranslate crashes on an empty language list.";
    }
    {
      assertion = builtins.elem "en" cfg.translateLanguages;
      message = "services.catalyrst.translateLanguages must include \"en\" -- argos model pairs are en<->X, so en is the pivot for source=auto and a client target.";
    }
    {
      assertion = builtins.all (c: builtins.match "[a-z]{2,3}(-[A-Z]{2})?" c != null) cfg.translateLanguages;
      message = "services.catalyrst.translateLanguages entries must be ISO-639-shaped codes (e.g. \"en\", \"pt\", \"zt\"), got: ${builtins.concatStringsSep " " cfg.translateLanguages}";
    }
  ];

  systemd.services.libretranslate.serviceConfig = {
    Restart = "on-failure";
    RestartSec = 30;
  };
}
