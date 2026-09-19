{ pkgs }:
let
  lib = pkgs.lib;
  root = ../.;
  cfg = {
    enable = true;
    subServices.comms = true;
    pulse = {
      sandbox = false;
      bindAddress = "0.0.0.0";
      port = 7777;
    };
    domain = "realm.test";
    tls = "none";
    publicUrl = "";
    exposure = "public";
    pgPort = 5432;
    livekit = {
      nodeIp = "";
      host = "";
    };
    contentPackage = null;
  };
  args = {
    inherit pkgs lib;
    config.services.catalyrst = cfg;
    inputs.catalyrst = {
      packages.x86_64-linux = { };
      shortRev = "test";
    };
  };
  native = (import (root + /comms.nix) args).content.systemd.services;
  sandbox =
    (import (root + /pulse-sandbox.nix) (
      args
      // {
        config.services.catalyrst = cfg // {
          pulse = cfg.pulse // {
            sandbox = true;
          };
        };
      }
    )).content;
  sync =
    (import (root + /catalyrst-sync.nix) args).content.systemd.services.catalyrst-sync.environment;
in
assert native.catalyrst-archipelago.environment.NATS_URL == "nats://127.0.0.1:4222";
assert native.pulse.content.environment.PULSE_NATS_URL == "nats://127.0.0.1:4222";
assert
  sandbox.virtualisation.oci-containers.containers.pulse.environment.PULSE_NATS_URL
  == "nats://127.0.0.1:4222";
assert builtins.elem "nats.service" native.catalyrst-archipelago.after;
assert builtins.elem "nats.service" native.pulse.content.after;
assert builtins.elem "nats.service" sandbox.systemd.services.podman-pulse.after;
assert sync.COMMS_FIXED_ADAPTER == "archipelago:ws://realm.test/ws";
assert sync.HTTP_SERVER_HOST == "127.0.0.1";
assert sync.TRUSTED_CLIENT_IP_HEADER == "x-real-ip";
pkgs.runCommand "catalyrst-comms-wiring" { } "touch $out"
