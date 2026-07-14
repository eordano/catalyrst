{ config, lib, pkgs, inputs, ... }: {
  imports = [
    ./hardware-configuration.nix
    ./networking.nix

    inputs.catalyrst.nixosModules.catalyrst
  ];

  networking.hostName = "my-catalyrst";

  users.users.root.openssh.authorizedKeys.keys = [
  ];
  users.users.root.hashedPassword = "!";

  services.catalyrst = {
    enable = true;
    domain = "yourdomain.example";

    acmeEmail = "ops@yourdomain.example";

    package = inputs.catalyrst.packages.${pkgs.system}.catalyrst;

    enableComms = false;

    realmName = "my-realm";
    ethRpcUrl = "http://127.0.0.1:8545";
    commsGatekeeperUrl = "http://127.0.0.1:5138";

    # Catalyst content servers to sync from. Empty means "sync from nothing";
    # name the peers of the network you actually intend to mirror.
    syncSource = [ ];
  };
}
