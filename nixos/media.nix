# LibreTranslate: the real translation backend behind the social bundle's
# /translate route (see bundles.nix's TRANSLATE_BACKEND_URL). Gated the same
# way as the social bundle itself -- it only needs to run where catalyrst-social
# does.
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
    # Download the argos models at startup. A fresh box ships none, so without
    # this the daemon crashes on an empty language list (IndexError at
    # languages[0]). All cfg.translateLanguages packs download on first boot
    # (network required, several GB); models then cache under the state dir.
    updateModels = true;
    # LibreTranslate CLI flags go through extraArgs as an attrset (rendered
    # by lib.cli.toCommandLineShellGNU), not a raw argv list.
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

  # The nixpkgs unit sets no Restart= and re-runs --update-models on every
  # start, so a failed argos fetch would otherwise leave /translate dead
  # until a manual restart.
  systemd.services.libretranslate.serviceConfig = {
    Restart = "on-failure";
    RestartSec = 30;
  };
}
