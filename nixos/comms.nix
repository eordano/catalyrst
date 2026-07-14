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

  commsPackages = inputs.catalyrst.packages.x86_64-linux;

  pulsePatched = commsPackages.pulse;

  livekitBaseConfig = pkgs.writeText "livekit-base.yaml" (
    if cfg.livekit.nodeIp != "" then
      ''
        port: 7880
        rtc:
          tcp_port: 7881
          udp_port: 7882
          use_external_ip: false
          node_ip: ${cfg.livekit.nodeIp}
          interfaces:
            excludes:
              - docker0
              - tailscale0
              - ifb0
          ips:
            includes:
              - ${cfg.livekit.nodeIp}/32
        logging:
          level: info
          json: false
      ''
    else
      ''
        port: 7880
        rtc:
          tcp_port: 7881
          udp_port: 7882
          use_external_ip: true
        logging:
          level: info
          json: false
      ''
  );

  inherit (import ./sandbox.nix)
    noPgSandbox
    noJitHardening
    rootOneshotSandbox
    ;
in
lib.mkIf (cfg.enable && cfg.subServices.comms) {
  systemd.services.nats = {
    description = "NATS message bus (archipelago)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" ];
    serviceConfig = noJitHardening // {
      ExecStart = "${pkgs.nats-server}/bin/nats-server -a 127.0.0.1 -p 4222 -m 8222";
      Restart = "always";
      RestartSec = 5;
      DynamicUser = true;
      MemoryMax = cfg.resources.natsMemoryMax;
      TasksMax = 128;
      SocketBindAllow = [
        "tcp:4222"
        "tcp:8222"
      ];
      SocketBindDeny = "any";
      IPAddressAllow = [ "localhost" ];
      IPAddressDeny = "any";
    };
  };

  # Mint livekit.yaml (the `keys:` block livekit's preStart merges) and
  # livekit-api.env (archipelago + the explore/social bundles source it) on
  # first boot if absent. Without this the credentials never exist -- the
  # rotate timer below only ROTATES an existing pair -- and every comms unit
  # fails LoadCredential on a fresh node.
  systemd.services.livekit-secret = {
    description = "Generate the LiveKit API key + secret on first boot";
    wantedBy = [ "multi-user.target" ];
    before = [
      "livekit.service"
      "catalyrst-archipelago.service"
    ];
    serviceConfig = rootOneshotSandbox // {
      Type = "oneshot";
      RemainAfterExit = true;
      User = "root";
      ReadWritePaths = [ cfg.secretsDir ];
    };
    script = ''
      set -euo pipefail
      umask 077
      YAML=${cfg.secretsDir}/livekit.yaml
      ENV=${cfg.secretsDir}/livekit-api.env
      if [ ! -s "$YAML" ] || [ ! -s "$ENV" ]; then
        KEY="API$(${pkgs.openssl}/bin/openssl rand -hex 6)"
        SECRET="$(${pkgs.openssl}/bin/openssl rand -base64 36 | tr -d '\n')"
        printf 'keys:\n  %s: %s\n' "$KEY" "$SECRET" > "$YAML"
        printf 'LIVEKIT_API_KEY=%s\nLIVEKIT_API_SECRET=%s\n' "$KEY" "$SECRET" > "$ENV"
        chmod 600 "$YAML" "$ENV"
      fi
    '';
  };

  systemd.services.livekit-rotate = {
    description = "Rotate LiveKit API key + secret";
    after = [ "livekit-secret.service" ];
    requires = [ "livekit-secret.service" ];
    serviceConfig = rootOneshotSandbox // {
      Type = "oneshot";
      User = "root";
      ReadWritePaths = [
        cfg.secretsDir
        "/var/lib/node-exporter-textfile"
      ];
    };
    script = ''
      set -euo pipefail
      umask 077

      YAML=${cfg.secretsDir}/livekit.yaml
      ENV=${cfg.secretsDir}/livekit-api.env
      METRIC=/var/lib/node-exporter-textfile/livekit_rotation.prom

      cp -a "$YAML" "$YAML.prev"
      cp -a "$ENV"  "$ENV.prev"

      KEY="API$(${pkgs.openssl}/bin/openssl rand -hex 6)"
      SECRET="$(${pkgs.openssl}/bin/openssl rand -base64 36 | tr -d '\n')"

      ytmp=$(mktemp "$YAML.XXXXXX")
      ${pkgs.gawk}/bin/awk -v k="$KEY" -v s="$SECRET" '
        BEGIN { in_keys=0 }
        /^keys:[[:space:]]*$/ { print "keys:"; print "  " k ": " s; in_keys=1; next }
        in_keys && /^[[:space:]]/ { next }
        { in_keys=0; print }
      ' "$YAML" > "$ytmp"
      chmod 600 "$ytmp"; chown root:root "$ytmp"
      mv "$ytmp" "$YAML"

      etmp=$(mktemp "$ENV.XXXXXX")
      printf 'LIVEKIT_API_KEY=%s\nLIVEKIT_API_SECRET=%s\n' "$KEY" "$SECRET" > "$etmp"
      chmod 600 "$etmp"; chown root:root "$etmp"
      mv "$etmp" "$ENV"

      systemctl restart livekit.service
      sleep 5

      if ! systemctl is-active --quiet livekit.service; then
        ${pkgs.util-linux}/bin/logger -t livekit-rotate "ROLLBACK: livekit failed to come up with new key"
        mv "$YAML.prev" "$YAML"
        mv "$ENV.prev"  "$ENV"
        systemctl restart livekit.service catalyrst-archipelago.service
        exit 1
      fi

      systemctl restart catalyrst-archipelago.service

      mkdir -p "$(dirname "$METRIC")"
      printf '# HELP livekit_rotation_timestamp_seconds Unix time of last successful LiveKit key rotation\n# TYPE livekit_rotation_timestamp_seconds gauge\nlivekit_rotation_timestamp_seconds %d\n' "$(date +%s)" > "$METRIC"

      ${pkgs.util-linux}/bin/logger -t livekit-rotate "rotated LiveKit API key (kid=$KEY)"
    '';
  };
  systemd.timers.livekit-rotate = {
    description = "Quarterly LiveKit key rotation";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "*-01,04,07,10-01 03:00:00";
      Persistent = true;
      RandomizedDelaySec = "1h";
    };
  };

  systemd.services.livekit = {
    description = "LiveKit SFU (comms media)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    preStart = ''
      umask 077
      {
        cat ${livekitBaseConfig}
        ${pkgs.gawk}/bin/awk '/^keys:/ { f = 1; print; next }
          f && /^[[:space:]]/ { print; next }
          { f = 0 }' "$CREDENTIALS_DIRECTORY/livekit.yaml"
      } > "$RUNTIME_DIRECTORY/config.yaml"
    '';
    serviceConfig = noJitHardening // {
      LoadCredential = "livekit.yaml:${cfg.secretsDir}/livekit.yaml";
      RuntimeDirectory = "livekit";
      RuntimeDirectoryMode = "0700";
      ExecStart = "${pkgs.livekit}/bin/livekit-server --config %t/livekit/config.yaml";
      DynamicUser = true;
      Restart = "always";
      RestartSec = 5;
      MemoryMax = cfg.resources.livekitMemoryMax;
      TasksMax = 1024;
      SocketBindAllow = [
        "tcp:7880"
        "tcp:7881"
        "udp:7882"
      ];
      SocketBindDeny = "any";
    };
  };

  systemd.services.catalyrst-archipelago = {
    description = "catalyrst-archipelago (clustering + ws-connector + stats, port 5139)";
    wantedBy = [ "multi-user.target" ];
    after = [ "livekit.service" ];
    wants = [ "livekit.service" ];
    environment = {
      HTTP_SERVER_PORT = "5139";
      HTTP_SERVER_HOST = "127.0.0.1";
      LIVEKIT_WS_URL = d.lkWsUrl;
      COMMS_GATEKEEPER_URL = "http://127.0.0.1:5145";
      RUST_LOG = "catalyrst_archipelago=info,tower_http=info";
    };
    serviceConfig = noPgSandbox // {
      LoadCredential = "livekit-env:${cfg.secretsDir}/livekit-api.env";
      ExecStart = pkgs.writeShellScript "catalyrst-archipelago-launcher" ''
        set -a
        . "$CREDENTIALS_DIRECTORY/livekit-env"
        set +a
        exec ${commsPackages.catalyrst-archipelago}/bin/catalyrst-archipelago
      '';
      DynamicUser = true;
      Restart = "always";
      RestartSec = 10;
      MemoryMax = cfg.resources.archipelagoMemoryMax;
      TasksMax = 256;
      SocketBindAllow = [ "tcp:5139" ];
      SocketBindDeny = "any";
      IPAddressAllow = [ "localhost" ] ++ cfg.comms.archipelagoExtraIpAllow;
      IPAddressDeny = "any";
    };
  };

  systemd.services.pulse = lib.mkIf (!cfg.pulse.sandbox) {
    description = "Pulse authoritative comms server (rust, ENet/UDP)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    environment = {
      RUST_LOG = "info";
      PULSE_BIND = "${cfg.pulse.bindAddress}:${toString cfg.pulse.port}";
    };
    serviceConfig = noJitHardening // {
      ExecStart = "${pulsePatched}/bin/catalyrst-pulse";
      Restart = "always";
      RestartSec = 10;
      DynamicUser = true;
      MemoryHigh = cfg.resources.pulseMemoryHigh;
      MemoryMax = cfg.resources.pulseMemoryMax;
      TasksMax = 512;
      SocketBindAllow = [
        "udp:7777"
        "tcp:5005"
      ];
      SocketBindDeny = "any";
    };
  };
}
