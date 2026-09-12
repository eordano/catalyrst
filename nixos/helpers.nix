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
    ;
}
