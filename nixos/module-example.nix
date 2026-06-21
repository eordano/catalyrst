# nixos/module-example.nix
#
# Example of how to wire the catalyrst NixOS module into your own host
# configuration. This file is illustrative — copy + adapt, do not import
# directly. It is NOT imported by flake.nix.
#
# Layout: your real host config lives outside this repo (or in a private
# overlay) and looks something like this. `inputs.catalyrst` is whatever name
# you gave the input in your top-level flake.nix.
{ config, lib, pkgs, inputs, ... }: {
  imports = [
    # Host-specific files (generated per machine):
    ./hardware-configuration.nix
    ./networking.nix

    # The catalyrst module itself:
    inputs.catalyrst.nixosModules.catalyrst
  ];

  # ---- operator identity ----
  networking.hostName = "my-catalyrst";

  users.users.root.openssh.authorizedKeys.keys = [
    # "ssh-ed25519 AAAA... you@host"
  ];
  users.users.root.hashedPassword = "!";

  # ---- the catalyrst module surface ----
  services.catalyrst = {
    enable = true;
    domain = "yourdomain.example";

    # ACME registration email — the module wires this into
    # security.acme.defaults.email. See docs/tls-acme.md.
    acmeEmail = "ops@yourdomain.example";

    # The Rust server build. Pull from the flake's packages output.
    package = inputs.catalyrst.packages.${pkgs.system}.catalyrst;

    # Comms is opt-in. Most operators only want the content server.
    enableComms = false;
    # If you flip enableComms = true, also wire commsPackages:
    # commsPackages = inputs.catalyrst.packages.${pkgs.system};

    realmName = "my-realm";
    syncSource = [
      "https://peer.decentraland.org/content"
      "https://peer-eu1.decentraland.org/content"
    ];
  };
}
