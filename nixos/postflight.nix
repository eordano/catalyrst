{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.catalyrst;
  pf = cfg.postflight;
  d = import ./helpers.nix cfg;

  comms = cfg.subServices.comms;
  social = cfg.subServices.social;
  v4 = d.v4.enabled;
  isPublic = cfg.exposure == "public";

  hostOf =
    url:
    let
      m = builtins.match "[a-z]+://(\\[[^]]+]|[^/:]+).*" url;
    in
    if m == null then "" else builtins.head m;
  isLoopback = host: host == "localhost" || host == "[::1]" || lib.hasPrefix "127." host;
  controlAdvertised = v4 && (d.wsScheme == "wss" || isLoopback cfg.domain);
  refreshAdvertised =
    d.v4.islandRefresh
    && (lib.hasPrefix "https://" d.v4.islandRefreshUrl || isLoopback (hostOf d.v4.islandRefreshUrl));

  pulseUnit = if cfg.pulse.sandbox then "podman-pulse.service" else "pulse.service";
  watched = [
    "catalyrst-sync.service"
  ]
  ++ lib.optionals comms [
    "nats.service"
    "livekit.service"
    "catalyrst-archipelago.service"
    pulseUnit
  ]
  ++ lib.optional v4 "catalyrst-comms-control-ready.service"
  ++ lib.optional (comms && social) "catalyrst-social.service";

  orNull = value: if value == "" then null else value;
  wantV4 = [
    (if controlAdvertised then d.v4.controlUrl else null)
    (if controlAdvertised then d.v4.audience else null)
    (if v4 then d.v4.pulseNativeEndpoint else null)
    (if v4 then orNull d.v4.pulseWebTransportUrl else null)
    (if v4 then d.v4.audience else null)
    (if refreshAdvertised then d.v4.islandRefreshUrl else null)
  ];

  logged =
    lib.optionals comms [
      {
        unit = pulseUnit;
        line = "durable Pulse handshake replay protection enabled";
      }
    ]
    ++ lib.optionals v4 [
      {
        unit = "catalyrst-archipelago.service";
        line = "control_v4_armed=true";
      }
      {
        unit = pulseUnit;
        line = "control position clustering enabled";
      }
    ]
    ++ lib.optionals (v4 && social) [
      {
        unit = "catalyrst-social.service";
        line = "cluster subscriber started";
      }
    ];

  refreshProbe =
    if isPublic then
      "--insecure --resolve ${lib.escapeShellArg "${cfg.domain}:443:127.0.0.1"} ${lib.escapeShellArg "https://${cfg.domain}/island-refresh"}"
    else
      "--header ${lib.escapeShellArg "Host: ${cfg.domain}"} http://127.0.0.1/island-refresh";

  script = pkgs.writeShellApplication {
    name = "catalyrst-postflight";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.curl
      pkgs.gnugrep
      pkgs.gnused
      pkgs.iproute2
      pkgs.jq
      pkgs.systemd
    ];
    text = ''
      strict=${if pf.strict then "1" else "0"}
      deadline=$(( $(date +%s) + ${toString pf.timeoutSec} ))
      problems=0

      note() { printf 'catalyrst-postflight: %s\n' "$1" >&2; }
      ok() { note "ok: $1"; }
      bad() { note "FAILED: $1"; problems=$((problems + 1)); }
      waiting() { [ "$(date +%s)" -lt "$deadline" ]; }

      journal() { journalctl --quiet --no-pager --output=cat --invocation=0 --unit="$1" | sed 's/\x1b\[[0-9;]*m//g'; }

      active() {
        while ! systemctl is-active --quiet "$1"; do
          if systemctl is-failed --quiet "$1" || ! waiting; then return 1; fi
          sleep 2
        done
      }

      holds() { journal "$1" | grep --count "$2" -- "$3" >/dev/null; }

      logged() {
        while ! holds "$1" --fixed-strings "$2"; do
          if ! waiting; then break; fi
          sleep 2
        done
        if holds "$1" --fixed-strings "$2"; then return 0; fi
        if holds "$1" --extended-regexp '^(Starting|Started) '; then return 1; fi
        return 2
      }

      for unit in ${lib.escapeShellArgs watched}; do
        if active "$unit"; then
          ok "$unit is active"
        else
          bad "$unit is $(systemctl is-active "$unit" || true)"
        fi
      done

      want=${lib.escapeShellArg (builtins.toJSON wantV4)}
      about=""
      got="null"
      while :; do
        about=$(curl --silent --max-time 5 http://127.0.0.1:5141/about || true)
        got=$(jq --compact-output '[.comms.v4.control.url, .comms.v4.control.audience, .comms.v4.pulse.nativeEndpoint, .comms.v4.pulse.webTransportUrl, .comms.v4.pulse.audience, .comms.v4.islandRefreshUrl]' <<<"$about" 2>/dev/null || echo null)
        if [ "$got" = "$want" ]; then break; fi
        if ! waiting; then break; fi
        sleep 2
      done
      if [ -z "$about" ]; then
        bad "/about does not answer on 127.0.0.1:5141"
      elif [ "$got" = "$want" ]; then
        ok "/about advertises comms.v4 as configured: $want"
      else
        bad "/about advertises comms.v4 $got, this configuration says $want"
        adapter=$(jq --raw-output '.comms.fixedAdapter // "none"' <<<"$about" 2>/dev/null || echo unreadable)
        note "  the advertised adapter is $adapter; an offline adapter means the server cannot reach ${cfg.domain} and withholds comms.v4 with it."
      fi
      ${lib.optionalString comms ''
        protocol=$(jq --raw-output '.comms.protocol // "none"' <<<"$about" 2>/dev/null || echo unreadable)
        if [ "$protocol" = v3 ]; then
          ok "/about keeps comms.protocol v3 for clients that predate v4"
        else
          bad "/about says comms.protocol $protocol, expected v3"
        fi
        if ss --udp --listening --numeric --no-header | grep --count -- ':${toString cfg.pulse.port} ' >/dev/null; then
          ok "Pulse listens on udp ${toString cfg.pulse.port}"
        else
          bad "nothing listens on udp ${toString cfg.pulse.port}"
        fi
        if [ -e /var/lib/catalyrst-pulse/replay.tsv ]; then
          ok "the Pulse replay journal is on the host at /var/lib/catalyrst-pulse/replay.tsv"
        else
          bad "/var/lib/catalyrst-pulse/replay.tsv is missing: handshake replay protection would not survive a restart"
        fi
      ''}
      ${lib.concatMapStrings (entry: ''
        seen=0
        logged ${lib.escapeShellArg entry.unit} ${lib.escapeShellArg entry.line} || seen=$?
        case "$seen" in
          0) ok "${entry.unit} logged '${entry.line}'" ;;
          1) bad "${entry.unit} never logged '${entry.line}' in its current run" ;;
          *) note "skipped: the journal no longer holds the start of ${entry.unit}, so '${entry.line}' cannot be read back" ;;
        esac
      '') logged}
      ${lib.optionalString (v4 && social) ''
        if holds catalyrst-social.service --extended-regexp 'cluster subscriber disabled|staying idle'; then
          bad "catalyrst-social.service runs without its cluster subscriber: no client is given a room through the authority"
        fi
      ''}
      ${lib.optionalString d.v4.islandRefresh ''
        code=$(curl --silent --max-time 10 --output /dev/null --write-out '%{http_code}' --request POST --header 'Content-Type: application/x-protobuf' --header 'Accept: application/x-protobuf' ${refreshProbe} || true)
        if [ "$code" = 400 ]; then
          ok "nginx hands /island-refresh to the social bundle (an unsigned protobuf POST is refused with 400)"
        else
          bad "an unsigned protobuf POST to /island-refresh answers $code, expected 400 from the social bundle"
        fi
      ''}

      if [ "$problems" -eq 0 ]; then
        note "every check passed."
        exit 0
      fi
      note "$problems check(s) failed."
      if [ "$strict" = 1 ]; then
        note "Set services.catalyrst.postflight.strict = false to report without failing the unit."
        exit 1
      fi
      exit 0
    '';
  };
in
{
  options.services.catalyrst.postflight = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        After every activation that changes the stack, check that what runs is
        what this configuration describes: the units are active, `/about`
        advertises exactly the `comms.v4` endpoints derived here (or none),
        Pulse listens and keeps its replay journal on the host, each comms unit
        logged that it is armed in its current run, and nginx hands
        `/island-refresh` to the social bundle. Nothing is stopped or rolled
        back; a failed check fails `catalyrst-postflight.service`, which is
        what `nixos-rebuild switch` and `colmena apply` report.
      '';
    };

    strict = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Fail the unit when a check fails. Set false to log the findings and
        leave the unit green.
      '';
    };

    timeoutSec = lib.mkOption {
      type = lib.types.ints.positive;
      default = 180;
      description = "How long the checks wait for the stack to settle before they count as failed.";
    };
  };

  config = lib.mkIf (cfg.enable && pf.enable) {
    warnings =
      lib.optional (v4 && !controlAdvertised)
        "services.catalyrst.comms.v4: the server only advertises a control URL that is wss (or ws on loopback), and ${d.v4.controlUrl} is neither, so clients are offered Pulse without a v4 control socket. Turn TLS on or set comms.v4.enable = false."
      ++
        lib.optional (d.v4.islandRefresh && !refreshAdvertised)
          "services.catalyrst.comms.v4: ${d.v4.islandRefreshUrl} is neither https nor loopback, so /about will not advertise the island refresh endpoint.";

    systemd.services.catalyrst-postflight = {
      description = "catalyrst postflight (the running stack matches this configuration)";
      wantedBy = [ "multi-user.target" ];
      after = watched ++ [ "nginx.service" ];
      restartTriggers = map (unit: config.systemd.units.${unit}.unit) watched;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = lib.getExe script;
        TimeoutStartSec = pf.timeoutSec + 60;
      };
    };
  };
}
