cfg:
let
  scheme = if cfg.tls == "none" then "http" else "https";
  wsScheme = if cfg.tls == "none" then "ws" else "wss";
  publicUrl = if cfg.publicUrl != "" then cfg.publicUrl else "${scheme}://${cfg.domain}";
  lkWsUrl = if cfg.exposure == "lan" then cfg.livekit.host else "${wsScheme}://livekit.${cfg.domain}";
  lkHostBare = if cfg.exposure == "lan" then cfg.livekit.host else "livekit.${cfg.domain}";
  inherit (cfg) pgPort;
  pgPortFlag = if cfg.pgPort != 5432 then " -p ${toString cfg.pgPort}" else "";
  pgPortQuery = if cfg.pgPort != 5432 then "&port=${toString cfg.pgPort}" else "";
  pgPortColon = if cfg.pgPort != 5432 then ":${toString cfg.pgPort}" else "";

  fedPeersFile =
    if cfg.federation.peersFile != null then
      toString cfg.federation.peersFile
    else if cfg.federation.seedDefault then
      "/etc/catalyrst/federation-peers.toml"
    else
      null;

  orDerived = value: derived: if value != "" then value else derived;
  pulseHost = if cfg.exposure == "lan" then cfg.domain else "pulse-server.${cfg.domain}";
  v4 = {
    enabled = cfg.subServices.comms && cfg.comms.v4.enable;
    audience = orDerived cfg.comms.v4.audience cfg.domain;
    controlUrl = "${wsScheme}://${cfg.domain}/ws/v4";
    controlDb = "comms_control";
    controlRole = "archipelago";
    controlConn = user: "postgresql:///${v4.controlDb}?host=/run/postgresql&user=${user}${pgPortQuery}";
    pulseIssuer = orDerived cfg.comms.v4.pulseIssuer pulseHost;
    pulseNativeEndpoint = orDerived cfg.comms.v4.pulseNativeEndpoint "${pulseHost}:${toString cfg.pulse.port}";
    pulseWebTransportUrl = cfg.comms.v4.pulseWebTransportUrl;
    pulseEnv = {
      PULSE_V4_ENABLED = "true";
      PULSE_V4_AUDIENCE = v4.audience;
      PULSE_V4_ISSUER = v4.pulseIssuer;
    }
    // (
      if v4.relay.enabled then
        {
          PULSE_APPLICATION_RELAY_ENABLED = "true";
          PULSE_ROOM_AUTHORITY_URL = v4.relay.authorityUrl;
          PULSE_ROOM_AUTHORITY_ALLOW_LOOPBACK_HTTP = "true";
        }
      else
        { }
    );
    islandRefresh = v4.enabled && cfg.subServices.social;
    islandRefreshUrl = "${publicUrl}/island-refresh";
    relay = {
      enabled = v4.enabled && cfg.subServices.social && cfg.comms.v4.applicationRelay;
      authorityUrl = "http://127.0.0.1:5145/internal/pulse/room-authority/v1";
      keyFile = "${cfg.secretsDir}/pulse-room-authority.env";
      pulseEnvFile = "${cfg.secretsDir}/pulse-relay.env";
    };
  };
in
{
  inherit
    scheme
    wsScheme
    publicUrl
    lkWsUrl
    lkHostBare
    pgPort
    pgPortFlag
    pgPortQuery
    pgPortColon
    fedPeersFile
    v4
    ;
}
