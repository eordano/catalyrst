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

  # Falls back to the flake's own SSR build (packages.sites) when the operator
  # leaves sitesPackage null -- same shape as squidPackage/governancePackage, so
  # public-gateway serves the /server operator UI out of the box.
  sitesPkg =
    if cfg.sitesPackage != null then
      cfg.sitesPackage
    else
      inputs.catalyrst.packages.${pkgs.stdenv.hostPlatform.system}.sites;

  inherit (import ./sandbox.nix)
    baseSandbox
    ;

  # The /server operator page collapses services this node does not enable
  # (and skips probing them) based on this list; unset would mean
  # treat-all-as-enabled. Derived from facts.nix so the page, the module and
  # the compose generator share one service vocabulary.
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
    # One option gates every admin surface: the /server and /admin pages read
    # ADMIN_WALLETS, so default it from adminAddresses. sites.env and
    # operator.env load after and still override.
    // lib.optionalAttrs (cfg.adminAddresses != [ ]) {
      ADMIN_WALLETS = lib.concatStringsSep "," cfg.adminAddresses;
    };
    serviceConfig = baseSandbox // {
      # operator.env is written by the /server page and loaded last, so an
      # operator-saved value overrides both the module defaults above and
      # sites.env after a restart.
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
