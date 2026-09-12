{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  d = import ./helpers.nix cfg;
  facts = import ./facts.nix;

  sitesPkg =
    if cfg.sitesPackage != null then
      cfg.sitesPackage
    else
      inputs.catalyrst.packages.${pkgs.stdenv.hostPlatform.system}.sites;

  inherit (import ./sandbox.nix)
    baseSandbox
    ;

  gateHolds =
    gate:
    if gate == null then
      true
    else if gate == "gateway.enable" then
      cfg.gateway.enable
    else if gate == "landAuthzIndex.enable" then
      cfg.landAuthzIndex.enable
    else
      cfg.subServices.${gate};
  enabledServices = lib.concatStringsSep "," (
    lib.filter (key: facts.services.${key}.unit != null && gateHolds facts.services.${key}.subService) (
      lib.attrNames facts.services
    )
  );
in
lib.mkIf (cfg.enable && cfg.subServices.sites) {
  systemd.services.catalyrst-sites = {
    description = "sites SSR web app (react-router-serve, port 5158)";
    after = [
      "postgresql.service"
      "network-online.target"
    ]
    ++ lib.optional cfg.subServices.telemetry "catalyrst-telemetry.service";
    wants = [ "network-online.target" ];
    wantedBy = [ "multi-user.target" ];
    environment = {
      PORT = "5158";
      HOST = "127.0.0.1";
      CATALYST_URL = d.publicUrl;
      CATALYST_DATABASE_URL = "postgresql:///content?host=/run/postgresql&user=catalyrst${d.pgPortQuery}";
      TELEMETRY_URL = "http://127.0.0.1:5150";
      GOVERNANCE_API_URL = "http://127.0.0.1:5151";
      WORLDS_URL = "http://127.0.0.1:5143";
      BEVY_PLAY_URL = "/play";
      CATALYRST_OPERATOR_ENV_FILE = "/var/lib/catalyrst-sites/operator.env";
      CATALYRST_ENABLED_SERVICES = enabledServices;
    }
    // lib.optionalAttrs (cfg.adminAddresses != [ ]) {
      ADMIN_WALLETS = lib.concatStringsSep "," cfg.adminAddresses;
    };
    serviceConfig = baseSandbox // {
      EnvironmentFile = [
        "-${cfg.secretsDir}/sites.env"
        "-/var/lib/catalyrst-sites/operator.env"
      ];
      StateDirectory = "catalyrst-sites";
      ExecStart = "${sitesPkg}/bin/sites-server";
      Restart = "always";
      RestartSec = 10;
      User = "catalyrst";
      Group = "catalyrst";
      ProtectHome = true;
      ReadWritePaths = [ "/run/postgresql" ];
      MemoryHigh = "1G";
      MemoryMax = "1536M";
      TasksMax = 512;
      SocketBindAllow = [ "tcp:5158" ];
      SocketBindDeny = "any";
    };
  };
}
