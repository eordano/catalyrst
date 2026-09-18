{ pkgs }:
let
  lib = pkgs.lib;
  root = ../.;
  shipped =
    (lib.evalModules {
      modules = [
        (root + /options.nix)
        { _module.check = false; }
      ];
    }).config.services.catalyrst;
  cfg = {
    enable = true;
    subServices = {
      comms = true;
      explore = false;
      create = false;
      social = true;
      data = false;
      abCdn = false;
      socialRpc = false;
      explorerApi = false;
    };
    pulse = {
      sandbox = false;
      bindAddress = "0.0.0.0";
      port = 7777;
    };
    comms.v4 = shipped.comms.v4;
    domain = "realm.test";
    tls = "none";
    publicUrl = "";
    secretsDir = "/var/lib/secrets";
    exposure = "public";
    pgPort = 5432;
    livekit = {
      nodeIp = "";
      host = "";
    };
    contentPackage = null;
    federation = {
      peerId = null;
      gossip = "off";
      natsRootCa = null;
      natsClientCert = null;
      natsClientKey = null;
      peersFile = null;
      seedDefault = false;
    };
  };
  argsFor = c: {
    inherit pkgs lib;
    config.services.catalyrst = c;
    inputs.catalyrst = {
      packages.x86_64-linux = { };
      shortRev = "test";
    };
  };
  evalFor =
    c:
    let
      sandboxed = c // {
        pulse = c.pulse // {
          sandbox = true;
        };
      };
      services = (import (root + /comms.nix) (argsFor c)).content.systemd.services;
    in
    {
      archipelago = services.catalyrst-archipelago;
      broker = services.nats.serviceConfig;
      firewall = (import (root + /firewall.nix) (argsFor c)).content.networking.firewall;
      rotate = services.livekit-rotate.script;
      relaySecret = services.pulse-relay-secret;
      pulse = services.pulse.content;
      sandbox = (import (root + /pulse-sandbox.nix) (argsFor sandboxed)).content;
      sync =
        (import (root + /catalyrst-sync.nix) (argsFor c))
        .content.systemd.services.catalyrst-sync.environment;
      social =
        (import (root + /bundles.nix) (argsFor c)).content.systemd.services.catalyrst-social.environment;
      socialUnit = (import (root + /bundles.nix) (argsFor c)).content.systemd.services.catalyrst-social;
      postgresql = (import (root + /postgresql.nix) (argsFor c)).content.services.postgresql;
    };
  on = evalFor cfg;
  off = evalFor (
    cfg
    // {
      comms.v4 = cfg.comms.v4 // {
        enable = false;
      };
    }
  );
  overridden = evalFor (
    cfg
    // {
      tls = "acme";
      comms.v4 = cfg.comms.v4 // {
        audience = "realm-a";
        pulseIssuer = "replica-1";
        pulseNativeEndpoint = "pulse.example:7001";
        pulseWebTransportUrl = "https://pulse.example:7743";
      };
    }
  );
  lan = evalFor (
    cfg
    // {
      exposure = "lan";
      domain = "10.0.0.5";
      livekit = cfg.livekit // {
        host = "ws://10.0.0.5:7880";
      };
    }
  );
  relayed = evalFor (
    cfg
    // {
      comms.v4 = cfg.comms.v4 // {
        applicationRelay = true;
      };
    }
  );
  relayedOff = evalFor (
    cfg
    // {
      comms.v4 = cfg.comms.v4 // {
        enable = false;
        applicationRelay = true;
      };
    }
  );
  fronted = evalFor (
    cfg
    // {
      publicUrl = "https://realm.example";
    }
  );
  explored = evalFor (
    cfg
    // {
      subServices = cfg.subServices // {
        explore = true;
      };
    }
  );
  containerUnit = e: e.sandbox.virtualisation.oci-containers.containers.pulse;
  container = e: (containerUnit e).environment;
  podmanPulse = e: e.sandbox.systemd.services.podman-pulse;
  v4Names = [
    "ARCHIPELAGO_CONTROL_V4_AUDIENCE"
    "ARCHIPELAGO_CONTROL_PG_CONNECTION_STRING"
    "COMMS_CONTROL_V4_AUDIENCE"
    "COMMS_CONTROL_PG_CONNECTION_STRING"
    "CLUSTER_SUBSCRIBER_ENABLED"
    "PULSE_V4_ENABLED"
    "PULSE_V4_AUDIENCE"
    "PULSE_V4_ISSUER"
    "COMMS_V4_CONTROL_URL"
    "COMMS_V4_CONTROL_AUDIENCE"
    "COMMS_V4_PULSE_AUDIENCE"
    "COMMS_V4_PULSE_NATIVE_ENDPOINT"
    "COMMS_V4_PULSE_WEBTRANSPORT_URL"
    "COMMS_V4_ISLAND_REFRESH_URL"
  ];
  relayNames = [
    "PULSE_APPLICATION_RELAY_ENABLED"
    "PULSE_ROOM_AUTHORITY_URL"
    "PULSE_ROOM_AUTHORITY_ALLOW_LOOPBACK_HTTP"
  ];
  relaySecretNames = [
    "PULSE_ROOM_AUTHORITY_KEY"
    "PULSE_APPLICATION_RELAY_LIVEKIT_API_KEY"
    "PULSE_APPLICATION_RELAY_LIVEKIT_SECRET"
  ];
  relaySecretUnit = "pulse-relay-secret.service";
  livekitCredential = "livekit-env:/var/lib/secrets/livekit-api.env";
  envsOf = e: [
    e.archipelago.environment
    e.pulse.environment
    (container e)
    e.sync
    e.social
  ];
  carries = names: e: lib.any (env: lib.any (name: env ? ${name}) names) (envsOf e);
  carriesV4 = carries v4Names;
  restarts = e: units: lib.hasInfix "systemctl restart ${lib.concatStringsSep " " units}\n" e.rotate;
  controlConn = user: "postgresql:///comms_control?host=/run/postgresql&user=${user}";
in
assert on.archipelago.environment.NATS_URL == "nats://127.0.0.1:4222";
assert on.pulse.environment.PULSE_NATS_URL == "nats://127.0.0.1:4222";
assert (container on).PULSE_NATS_URL == "nats://127.0.0.1:4222";
assert lib.hasInfix " -a 127.0.0.1 -p 4222 " on.broker.ExecStart;
assert on.broker.IPAddressAllow == [ "localhost" ];
assert on.broker.IPAddressDeny == "any";
assert !(builtins.elem 4222 on.firewall.allowedTCPPorts);
assert !(builtins.elem 8222 on.firewall.allowedTCPPorts);
assert !(builtins.elem 4222 lan.firewall.allowedTCPPorts);
assert builtins.elem "nats.service" on.archipelago.after;
assert builtins.elem "nats.service" on.pulse.after;
assert builtins.elem "nats.service" on.sandbox.systemd.services.podman-pulse.after;
assert on.sync.COMMS_FIXED_ADAPTER == "archipelago:ws://realm.test/ws";
assert on.sync.HTTP_SERVER_HOST == "127.0.0.1";
assert on.sync.TRUSTED_CLIENT_IP_HEADER == "x-real-ip";
assert on.sync.COMMS_PROTOCOL == "v3";

assert on.archipelago.environment.ARCHIPELAGO_CONTROL_V4_AUDIENCE == "realm.test";
assert
  on.archipelago.environment.ARCHIPELAGO_CONTROL_PG_CONNECTION_STRING == controlConn "archipelago";
assert on.social.COMMS_CONTROL_V4_AUDIENCE == "realm.test";
assert on.social.COMMS_CONTROL_PG_CONNECTION_STRING == controlConn "catalyrst";
assert on.social.NATS_URL == on.archipelago.environment.NATS_URL;
assert on.social.NATS_URL == on.pulse.environment.PULSE_NATS_URL;
assert on.social.CLUSTER_SUBSCRIBER_ENABLED == "true";
assert builtins.elem "nats.service" on.socialUnit.after;
assert !(off.social ? NATS_URL);
assert !(builtins.elem "nats.service" off.socialUnit.after);
assert on.pulse.environment.PULSE_V4_ENABLED == "true";
assert on.pulse.environment.PULSE_V4_AUDIENCE == "realm.test";
assert on.pulse.environment.PULSE_V4_ISSUER == "pulse-server.realm.test";
assert (container on).PULSE_V4_ENABLED == "true";
assert (container on).PULSE_V4_AUDIENCE == "realm.test";
assert (container on).PULSE_V4_ISSUER == "pulse-server.realm.test";
assert on.sync.COMMS_V4_CONTROL_URL == "ws://realm.test/ws/v4";
assert on.sync.COMMS_V4_CONTROL_AUDIENCE == "realm.test";
assert on.sync.COMMS_V4_PULSE_AUDIENCE == "realm.test";
assert on.sync.COMMS_V4_PULSE_NATIVE_ENDPOINT == "pulse-server.realm.test:7777";
assert !(on.sync ? COMMS_V4_PULSE_WEBTRANSPORT_URL);
assert builtins.elem "postgresql-comms-control.service" on.archipelago.after;
assert on.archipelago.serviceConfig.User == "archipelago";
assert !on.archipelago.serviceConfig.DynamicUser;
assert !(on.archipelago.serviceConfig ? PrivateUsers);
assert builtins.elem "comms_control" on.postgresql.ensureDatabases;
assert lib.any (user: user.name == "archipelago") on.postgresql.ensureUsers;

assert overridden.sync.COMMS_V4_CONTROL_URL == "wss://realm.test/ws/v4";
assert overridden.sync.COMMS_V4_PULSE_NATIVE_ENDPOINT == "pulse.example:7001";
assert overridden.sync.COMMS_V4_PULSE_WEBTRANSPORT_URL == "https://pulse.example:7743";
assert overridden.archipelago.environment.ARCHIPELAGO_CONTROL_V4_AUDIENCE == "realm-a";
assert overridden.social.COMMS_CONTROL_V4_AUDIENCE == "realm-a";
assert overridden.pulse.environment.PULSE_V4_AUDIENCE == "realm-a";
assert overridden.pulse.environment.PULSE_V4_ISSUER == "replica-1";

assert lan.sync.COMMS_V4_PULSE_NATIVE_ENDPOINT == "10.0.0.5:7777";
assert lan.pulse.environment.PULSE_V4_ISSUER == "10.0.0.5";
assert lan.sync.COMMS_V4_CONTROL_URL == "ws://10.0.0.5/ws/v4";

assert !shipped.comms.v4.applicationRelay;
assert on.sync.COMMS_V4_ISLAND_REFRESH_URL == "http://realm.test/island-refresh";
assert overridden.sync.COMMS_V4_ISLAND_REFRESH_URL == "https://realm.test/island-refresh";
assert fronted.sync.COMMS_V4_ISLAND_REFRESH_URL == "https://realm.example/island-refresh";
assert !carries relayNames on;
assert !on.relaySecret.condition;
assert on.socialUnit.serviceConfig.LoadCredential == [ livekitCredential ];
assert !(on.pulse.serviceConfig ? EnvironmentFile);
assert (containerUnit on).environmentFiles == [ ];
assert !(builtins.elem relaySecretUnit on.socialUnit.after);

assert restarts on [
  "catalyrst-archipelago.service"
  "catalyrst-social.service"
];
assert restarts explored [
  "catalyrst-archipelago.service"
  "catalyrst-explore.service"
  "catalyrst-social.service"
];
assert restarts relayed [
  "catalyrst-archipelago.service"
  "catalyrst-social.service"
  relaySecretUnit
  "pulse.service"
];

assert relayed.relaySecret.condition;
assert builtins.elem "pulse.service" relayed.relaySecret.content.before;
assert builtins.elem "catalyrst-social.service" relayed.relaySecret.content.before;
assert builtins.elem "livekit-secret.service" relayed.relaySecret.content.requires;
assert relayed.pulse.environment.PULSE_APPLICATION_RELAY_ENABLED == "true";
assert
  relayed.pulse.environment.PULSE_ROOM_AUTHORITY_URL
  == "http://127.0.0.1:5145/internal/pulse/room-authority/v1";
assert relayed.pulse.environment.PULSE_ROOM_AUTHORITY_ALLOW_LOOPBACK_HTTP == "true";
assert (container relayed).PULSE_APPLICATION_RELAY_ENABLED == "true";
assert
  (container relayed).PULSE_ROOM_AUTHORITY_URL == relayed.pulse.environment.PULSE_ROOM_AUTHORITY_URL;
assert (container relayed).PULSE_ROOM_AUTHORITY_ALLOW_LOOPBACK_HTTP == "true";
assert relayed.pulse.serviceConfig.EnvironmentFile == "/var/lib/secrets/pulse-relay.env";
assert (containerUnit relayed).environmentFiles == [ "/var/lib/secrets/pulse-relay.env" ];
assert builtins.elem relaySecretUnit relayed.pulse.after;
assert relayed.pulse.requires == [ relaySecretUnit ];
assert builtins.elem relaySecretUnit (podmanPulse relayed).after;
assert (podmanPulse relayed).requires == [ relaySecretUnit ];
assert builtins.elem "podman-pulse.service"
  (import (root + /comms.nix) (
    argsFor (
      cfg
      // {
        pulse = cfg.pulse // {
          sandbox = true;
        };
        comms.v4 = cfg.comms.v4 // {
          applicationRelay = true;
        };
      }
    )
  )).content.systemd.services.pulse-relay-secret.content.before;
assert
  relayed.socialUnit.serviceConfig.LoadCredential == [
    livekitCredential
    "pulse-room-authority-env:/var/lib/secrets/pulse-room-authority.env"
  ];
assert builtins.elem relaySecretUnit relayed.socialUnit.after;
assert !carries relaySecretNames relayed;

assert carriesV4 on;
assert !carriesV4 off;
assert !carries relayNames off;
assert !carriesV4 relayedOff;
assert !carries relayNames relayedOff;
assert !relayedOff.relaySecret.condition;
assert relayedOff.socialUnit.serviceConfig.LoadCredential == [ livekitCredential ];
assert (containerUnit relayedOff).environmentFiles == [ ];
assert off.archipelago.serviceConfig.DynamicUser;
assert off.archipelago.serviceConfig.PrivateUsers;
assert !(off.archipelago.serviceConfig ? User);
assert !(builtins.elem "postgresql-comms-control.service" off.archipelago.after);
assert !(builtins.elem "comms_control" off.postgresql.ensureDatabases);
assert !(lib.any (user: user.name == "archipelago") off.postgresql.ensureUsers);
pkgs.runCommand "catalyrst-comms-wiring" { } "touch $out"
