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

  scenePkgs = inputs.catalyrst.packages.x86_64-linux;

  inherit (import ./sandbox.nix { }) noJitHardening;
in
lib.mkIf (cfg.enable && cfg.subServices.sceneState) {
  systemd.services.catalyrst-scene-state = {
    description = "catalyrst-scene-state (authoritative SDK7 scene state, rust+V8, :5209)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    environment = {
      HTTP_SERVER_HOST = "127.0.0.1";
      HTTP_SERVER_PORT = "5209";
      REALM_NAME = cfg.realm;
      WORLD_SERVER_URL = d.publicUrl;
      STORAGE_URL = d.publicUrl;
      RUST_LOG = "catalyrst_scene_state=info";
    };
    serviceConfig = noJitHardening // {
      ExecStart = "${scenePkgs.catalyrst-scene-state}/bin/catalyrst-scene-state";
      Restart = "always";
      RestartSec = 10;
      DynamicUser = true;
      MemoryHigh = "1536M";
      MemoryMax = "2G";
      TasksMax = 512;
      SocketBindAllow = [ "tcp:5209" ];
      SocketBindDeny = "any";
    };
  };
}
