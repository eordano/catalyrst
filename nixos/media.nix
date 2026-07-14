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
    # languages[0]); load-only scopes the download to en,es. Needs network on
    # first boot; models then cache under the state dir.
    updateModels = true;
    # LibreTranslate CLI flags go through extraArgs as an attrset (rendered
    # by lib.cli.toCommandLineShellGNU), not a raw argv list -- load-only
    # trims the model set to what the /translate contract test exercises.
    extraArgs = {
      load-only = "en,es";
    };
  };
}
