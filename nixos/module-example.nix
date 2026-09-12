{
  content-node-example =
    { pkgs, inputs, ... }:
    {
      imports = [ inputs.catalyrst.nixosModules.catalyrst ];

      services.catalyrst = {
        enable = true;
        profile = "content-node";

        domain = "node.home.arpa";

        contentPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst;

        sync.sources = [ "https://peer.decentraland.org/content" ];

        federation.seedDefault = false;
      };
    };

  public-gateway-example =
    { pkgs, inputs, ... }:
    {
      imports = [ inputs.catalyrst.nixosModules.catalyrst ];

      services.catalyrst = {
        enable = true;
        profile = "public-gateway";

        domain = "example.org";

        adminAddresses = [ "0x0000000000000000000000000000000000000000" ];

        ethRpcUrl = "https://eth-rpc.example.org/mainnet";

        contentPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst;
        bundlesPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-all;
        governancePackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-governance;
        presencePackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-presence;

        play = {
          enable = true;
          package = inputs.bevy-explorer.packages.${pkgs.system}.web;
        };

        federation.seedDefault = true;
      };
    };
}
