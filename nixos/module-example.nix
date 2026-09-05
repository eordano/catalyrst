# Two worked examples of consuming services.catalyrst from another flake.
#
# Each attribute below is a NixOS module. Import the one you want into a host's
# configuration, or copy its `services.catalyrst` block. Both assume the
# consumer flake passes `inputs` through `specialArgs` (the comms, sync and
# squid units read `inputs.catalyrst.packages`):
#
#   nixpkgs.lib.nixosSystem {
#     specialArgs = { inherit inputs; };
#     modules = [ inputs.catalyrst.nixosModules.catalyrst ./host.nix ];
#   };
{
  # A. LAN content node. No sibling services, self-signed TLS, plain-HTTP edge.
  # `profile = "content-node"` seeds exposure = "lan" and tls = "none" (plain HTTP :80).
  content-node-example =
    { pkgs, inputs, ... }:
    {
      imports = [ inputs.catalyrst.nixosModules.catalyrst ];

      services.catalyrst = {
        enable = true;
        profile = "content-node";

        # The host a content-only node answers on (a LAN IP also works).
        domain = "node.home.arpa";

        # Only the content/lambdas server binary is needed for this profile.
        contentPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst;

        # Which network to mirror. Name the peers you actually intend to sync.
        sync.sources = [ "https://peer.decentraland.org/content" ];

        # Federation is off by default here: leave the shipped seed unprovisioned.
        federation.seedDefault = false;
      };
    };

  # B. Public gateway. The full realm plus the gateway/abCdn/governance/
  # presence/telemetry/squid surface. `profile = "public-gateway"` is the module
  # default; it seeds exposure = "public" and tls = "acme-dns01" (a wildcard
  # certificate over one DNS record, needs a DNS-provider API token).
  public-gateway-example =
    { pkgs, inputs, ... }:
    {
      imports = [ inputs.catalyrst.nixosModules.catalyrst ];

      services.catalyrst = {
        enable = true;
        profile = "public-gateway";

        domain = "example.org";
        # tls defaults to acme-dns01: a wildcard cert, so you manage one
        # `*.example.org` DNS record instead of ~29 A-records. Supply your DNS
        # provider + its API-token env file (variables lego expects):
        #   dnsProvider = "cloudflare";           # or route53, gcloud, ...
        #   dnsCredentialsFile = "/run/secrets/acme-dns.env";
        # Prefer no DNS token? `tls = "acme-http01"` instead -- no credential,
        # but one A-record per subdomain, each resolving before the first build.

        adminAddresses = [ "0x0000000000000000000000000000000000000000" ];

        # Placeholder -- point this at your own Ethereum RPC to avoid depending
        # on the shared public endpoint.
        ethRpcUrl = "https://eth-rpc.example.org/mainnet";

        # The realm binaries. bundlesPackage carries the explore/create/social/
        # data bundles and the telemetry/worldStorage/profileImages/signatures
        # singles that this profile enables.
        contentPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst;
        bundlesPackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-all;
        governancePackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-governance;
        presencePackage = inputs.catalyrst.packages.${pkgs.system}.catalyrst-presence;

        # The bevy-explorer wasm /play surface, served as one atomic store path.
        play = {
          enable = true;
          package = inputs.bevy-explorer.packages.${pkgs.system}.web;
        };

        # squidPackage stays null: the module falls back to the flake's own
        # packages.squid, so this profile's subServices.squid = true brings the
        # indexer up for real -- it needs ${secretsDir}/squid.env (DB DSN plus
        # eth + polygon archive-RPC endpoints) to start. sitesPackage stays
        # null and the SSR sites tier off until that package exists.

        # Ships the default federation seed (see nixos/federation-peers.toml) to
        # /etc/catalyrst/federation-peers.toml. As shipped every entry's
        # mtls_root_pem is blank, so federation stays refused until each peer's
        # root certificate is filled in -- copy the file, supply the roots, and
        # set federation.peersFile to the copy.
        federation.seedDefault = true;
      };
    };
}
