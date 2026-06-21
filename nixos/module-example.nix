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
    syncSource = [
      "https://peer.decentraland.org/content"
      "https://peer-eu1.decentraland.org/content"
    ];
  };
}
