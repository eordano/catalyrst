{ config, lib, ... }:
let
  inherit (lib)
    mkOption
    mkEnableOption
    mkDefault
    mkIf
    mkMerge
    types
    ;

  cfg = config.services.catalyrst;

  boolOpt =
    default:
    mkOption {
      type = types.bool;
      inherit default;
    };
  strOpt =
    default:
    mkOption {
      type = types.str;
      inherit default;
    };

  isPublic = cfg.profile == "full-realm" || cfg.profile == "public-gateway";
in
{
  options.services.catalyrst = {
    enable = mkEnableOption "Decentraland catalyrst (Catalyst) node";

    profile = mkOption {
      type = types.enum [
        "content-node"
        "full-realm"
        "public-gateway"
      ];
      default = "public-gateway";
      description = ''
        Coarse deployment shape. Seeds `subServices`, `exposure` and `tls`
        through `lib.mkDefault`, so every derived value stays overridable
        per host.

        - `content-node`: content + sync + postgres + nginx only, no sibling
          services; `exposure = "lan"`, `tls = "none"` (plain HTTP :80).
        - `full-realm`: adds comms (livekit/archipelago/pulse), the
          explore/create/social/data bundles, socialRpc, explorerApi,
          worldStorage, profileImages and signatures; `exposure = "public"`,
          `tls = "acme-dns01"`.
        - `public-gateway`: full-realm plus `gateway.enable`, abCdn,
          governance, presence, telemetry and squid; `exposure = "public"`,
          `tls = "acme-dns01"`.
      '';
    };

    exposure = mkOption {
      type = types.enum [
        "public"
        "lan"
      ];
      default = "lan";
      description = ''
        Edge topology. `public` runs the internet-facing edge (real-IP
        restore, forceSSL, the abgen/livekit subdomains) with ACME driven by
        `tls`. `lan` serves plain HTTP on :80 to the local network with no
        ACME wiring. Seeded by `profile`; override per host.
      '';
    };

    domain = mkOption {
      type = types.str;
      description = "Host the node answers on: an FQDN for public, an IP for LAN.";
    };

    stateDir = mkOption {
      type = types.str;
      default = "/srv/catalyrst";
      description = ''
        Root of the node's on-disk state (content blobs, the play docroot).
        An option rather than a literal so the deployment path is not baked
        into the exported module.
      '';
    };

    profileImagesRender = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Render avatar face/body thumbnails locally instead of proxying
          profile-images.decentraland.org.

          Off by default because it pulls in the godot-explorer package: a
          headless export of decentraland's Godot fork, which is a heavy build.
          Turning it on makes the node self-contained for avatar imagery -- the
          images stop depending on Decentraland's servers being up, which is the
          point for a self-hosted realm.
        '';
      };

      package = mkOption {
        type = types.nullOr types.package;
        default = null;
        description = ''
          The godot-explorer package providing the renderer binary. Null takes
          `packages.godot-explorer` from the catalyrst flake input.
        '';
      };

      cacheMaxBytes = mkOption {
        type = types.int;
        default = 8 * 1024 * 1024 * 1024;
        description = ''
          Byte budget for the on-disk render cache. Over it, oldest entries are
          evicted; every entry is re-derivable, so an eviction costs one
          re-render rather than data loss. 0 disables the bound, which is how
          the cache behaved before it had one.
        '';
      };

      maxConcurrentRenders = mkOption {
        type = types.int;
        default = 1;
        description = ''
          Concurrent godot processes. Each is a full engine instance; more than
          a couple will contend for CPU on the software-GL path and make every
          render slower rather than the queue shorter.
        '';
      };

      memoryMax = mkOption {
        type = types.str;
        default = "4G";
        description = ''
          Cgroup ceiling for the unit. The shared default for small services is
          512M, which a headless Godot exceeds immediately -- this exists so the
          renderer is not silently OOM-killed mid-render.
        '';
      };
    };

    secretsDir = mkOption {
      type = types.str;
      default = "/var/lib/secrets";
      description = ''
        Directory the sibling units read runtime secrets from (admin session
        secret, livekit keys, world-storage key). An option rather than a
        literal so the secrets path is not baked into the exported module.
      '';
    };

    pgPort = mkOption {
      type = types.port;
      default = 5432;
      description = ''
        Port the catalyst's PostgreSQL listens on (socket file + TCP).
        Override when another PostgreSQL must own the standard 5432 on the
        same host; kept at 5432 otherwise.
      '';
    };

    realm = strOpt "catalyrst";

    publicUrl = mkOption {
      type = types.str;
      default = "";
      description = "External base URL. Empty => derived as <scheme>://<domain>.";
    };

    ethRpcUrl = mkOption {
      type = types.str;
      default = "https://rpc.decentraland.org/mainnet";
      description = ''
        Ethereum mainnet JSON-RPC endpoint the content server uses for
        blockchain access checks. Defaults to the public Decentraland RPC;
        point it at your own node to avoid depending on the shared endpoint.
      '';
    };

    upstream = mkOption {
      description = ''
        How far the node leans on the public decentraland.org network for
        content and services it does not originate locally. Everything here
        defaults to parity with the public network; a fully self-contained
        node turns the mirrors off and sets metaTxRelay to null.
      '';
      default = { };
      type = types.submodule {
        options = {
          # Hourly catalog mirrors (events ~7.3k rows, worlds ~1.6k). Off
          # means the crates' defaults apply and the catalogs stay local-only.
          mirrorEvents = boolOpt true;
          mirrorWorlds = boolOpt true;
          metaTxRelay = mkOption {
            type = types.nullOr types.str;
            default = "https://transactions-api.decentraland.org";
            description = ''
              Upstream transactions-api that broadcasts gasless meta-txs when
              no local relayer is provisioned (local providers always take
              precedence). null disables the fallback: without a local
              relayer, meta-tx broadcasts then answer 503.
            '';
          };
        };
      };
    };

    translateLanguages = mkOption {
      type = types.listOf types.str;
      default = [
        "en"
        "es"
        "fr"
        "de"
        "ru"
        "pt"
        "it"
        "zh"
        "ja"
        "ko"
      ];
      description = ''
        LibreTranslate language codes for load-only. The default carries the
        explorer's full ChatTranslate target list; the client sends
        source=auto, so every listed pair's argos models download on first
        boot (several GB). Must stay non-empty, ISO-639-shaped, and include
        "en" -- the argos model pairs are en<->X, so en is both the pivot for
        auto-detected sources and a client-selectable target.
      '';
    };

    # A missing texture dependency renders magenta and the asset bundle still
    # completes (upstream asset-bundle-converter tolerance); off, one
    # mis-pathed texture fails the whole scene's bundle.
    abgenMagentaMissing = boolOpt true;

    tls = mkOption {
      type = types.enum [
        "acme-http01"
        "acme-dns01"
        "none"
      ];
      default = "acme-dns01";
      description = ''
        TLS mode. Drives http/https + ws/wss in the emitted URLs.

        - `acme-dns01` (default): a wildcard certificate over the DNS-01
          challenge. The operator manages ONE DNS record (`*.domain`) instead
          of one A-record per subdomain, at the cost of a DNS-provider API
          token -- set `dnsProvider` + `dnsCredentialsFile`.
        - `acme-http01`: a single multi-SAN certificate spanning every gateway
          subdomain, issued over the HTTP-01 challenge (no DNS API token, but
          one A-record per SAN -- ~29 for a full gateway -- and every one must
          resolve to this host before the first rebuild).
        - `none`: plain HTTP, no TLS -- the LAN edge (`exposure = "lan"`) serves
          port 80 only, so this is the LAN/dev mode. There is no self-signed
          mode: the LAN vhost binds no 443 listener, so a "self-signed" tls
          would emit https URLs nothing answers.
      '';
    };

    dnsProvider = mkOption {
      type = types.str;
      default = "cloudflare";
      description = ''
        The lego DNS-01 provider for `tls = "acme-dns01"` (e.g. `cloudflare`,
        `route53`, `gcloud`, `digitalocean` -- any provider lego supports).
        Ignored under other tls modes. Cloudflare is the default because it is
        the reference deployment's provider; point it at your own.
      '';
    };

    dnsCredentialsFile = mkOption {
      type = types.str;
      default = "";
      description = ''
        Path to the environment file holding the `dnsProvider`'s API
        credentials (the variables lego expects, e.g. `CF_DNS_API_TOKEN`).
        Empty => `<secretsDir>/acme-dns.env`. Read only under
        `tls = "acme-dns01"`.
      '';
    };

    openFirewall = mkOption {
      type = types.bool;
      default = true;
      description = ''
        Open the node's own listener ports in `networking.firewall` -- the
        edge (80, plus 443 when public) and, when comms is enabled, LiveKit
        (7880/7881 tcp, 7882 udp) and pulse (7777 udp). A stock NixOS box
        firewalls everything but SSH, so without this a fresh node starts but
        answers nothing. Set false to manage the firewall yourself; the ports
        are the same set `facts.nix` records.
      '';
    };

    adminAddresses = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = "Wallet addresses allowed to mutate via the /admin console.";
    };

    contentPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        Override for the catalyrst content/lambdas server package (bin
        catalyrst-live). Null uses inputs.catalyrst's stock `catalyrst`
        package; set it to a patched build when the deployed server must
        diverge from the pinned release.
      '';
    };

    bundlesPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        The catalyrst-all multi-binary package: the explore/create/social/data
        bundles plus catalyrst-social-rpc, catalyrst-explorer-api,
        catalyrst-scene-state, catalyrst-telemetry, catalyrst-world-storage,
        catalyrst-profile-images and catalyrst-signatures. Required when the
        telemetry/worldStorage/profileImages/signatures flags are enabled.
        Typically inputs.catalyrst.packages.x86_64-linux.catalyrst-all.
      '';
    };

    governancePackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        The catalyrst-governance package. Required when
        subServices.governance is enabled. Typically
        inputs.catalyrst.packages.x86_64-linux.catalyrst-governance.
      '';
    };

    presencePackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        The catalyrst-presence package. Required when subServices.presence is
        enabled. Typically inputs.catalyrst.packages.x86_64-linux.catalyrst-presence.
      '';
    };

    squidPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        The Subsquid marketplace/LAND/ENS indexer build (eth + polygon
        processors + TypeORM migrate). null -- the default -- falls back to
        the catalyrst flake's own packages.squid, built from contracts/squid,
        so profiles that turn subServices.squid on run the indexer out of the
        box. Set it only to override that build. The units need squid.env in
        `secretsDir` (DB DSN + eth and polygon archive-RPC endpoints) to
        start.
      '';
    };

    sitesPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        Built sites SSR bundle exposing bin/sites-server (react-router-serve
        build/server/index.js) -- the web surface tier. Null keeps the sites
        units inert even when subServices.sites is on; the SSR tier is not
        part of the exported module. Typically a private path input.
      '';
    };

    telemetryAdminTokenFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        File holding the bare CATALYRST_TELEMETRY_ADMIN_TOKEN value, read at
        unit start via LoadCredential. It gates catalyrst-telemetry's
        /dash/admin endpoints and the Telemetry card in catalyrst-live's admin
        console, so both units load it. Left null the token stays unset and
        telemetry admin is disabled.
      '';
    };

    play = mkOption {
      description = ''
        Serve the pinned bevy-explorer wasm web client (the /play surface +
        editor viewport engine) as precompressed static files (index.html,
        main.js/engine.js, pkg/webgpu_build_bg.wasm(.br),
        assets_bundle.bin(.br), ui3-overlay/, vendor/). Set `package` to serve
        one store path; leave it null to keep mounting `dir` with brotli_static.
      '';
      default = { };
      type = types.submodule {
        options = {
          enable = boolOpt false;
          dir = strOpt "/srv/catalyrst/play";
          package = mkOption {
            type = types.nullOr types.package;
            default = null;
            description = ''
              The built bevy-explorer web dist -- the whole /play docroot as a
              single store path. Typically
              inputs.bevy-explorer.packages.x86_64-linux.web. Non-null: nginx
              aliases the store path, `dir` is unused, and the bundle is atomic
              and rollback-able with the generation. Null: `dir` is created
              empty and its contents arrive out of band.
            '';
          };
        };
      };
    };

    gateway = mkOption {
      description = ''
        Publish `gateway.<domain>` speaking the gateway.decentraland.org
        surface: each service prefix (/social-api, /notifications, /ab-cdn,
        /camera-reel-service, ...) is stripped and proxied to the local
        catalyrst backend, and the realm surface (/about, /content/,
        /lambdas/) is served natively. This is the host the unity-explorer
        --base-domain flag (PR 9728) fans every service URL onto once
        use-gateway is on. DNS and the certificate must already cover the
        subdomains (see `tls`). `featureFlags` additionally publishes
        `feature-flags.<domain>` -> explorer-api, whose /explorer.json is the
        first thing the explorer fetches.
      '';
      default = { };
      type = types.submodule {
        options = {
          enable = boolOpt false;
          featureFlags = boolOpt true;
        };
      };
    };

    pulse = mkOption {
      description = ''
        Pulse authoritative comms server (ENet/UDP). `sandbox` swaps the
        native systemd unit for a gVisor (runsc) podman container with host
        networking, so the internet-facing raw-UDP parser runs against
        gVisor's user-space kernel instead of the host's. Keep `bindAddress`
        at 0.0.0.0: the socket answers on every host IP, and which one clients
        use is decided solely by the pulse-server DNS record.
      '';
      default = { };
      type = types.submodule {
        options = {
          sandbox = boolOpt false;
          bindAddress = strOpt "0.0.0.0";
          port = mkOption {
            type = types.port;
            default = 7777;
          };
        };
      };
    };

    sync = {
      enable = boolOpt true;
      concurrency = mkOption {
        type = types.int;
        default = 1500;
      };
      sources = mkOption {
        type = types.listOf types.str;
        default = [
          "https://peer.dclnodes.io/content"
          "https://peer.decentraland.org/content"
          "https://peer.uadevops.com/content"
          "https://peer-eu1.decentraland.org/content"
          "https://peer.melonwave.com/content"
        ];
        description = "Upstream DCL peers the content server syncs from.";
      };
    };

    livekit.host = mkOption {
      type = types.str;
      default = "";
      description = ''
        Full ws(s) URL clients use to reach LiveKit. Empty => derived as
        <wsScheme>://livekit.<domain> (the public form).
      '';
    };

    livekit.nodeIp = mkOption {
      type = types.str;
      default = "";
      description = ''
        Single IP LiveKit advertises as its ICE candidate (rtc.node_ip with
        use_external_ip false and an rtc.ips allowlist). Keep it equal to what
        the pulse-server DNS record resolves to: clients dial every advertised
        candidate, and docker-bridge/VPC/private addresses stall the
        connection from outside. Empty => legacy auto-enumeration
        (use_external_ip true, all interfaces advertised).
      '';
    };

    comms.archipelagoExtraIpAllow = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = ''
        CIDRs the archipelago unit's IPAddressAllow admits beyond localhost.
        Archipelago dials LiveKit at its public URL, so whatever
        livekit.<domain> resolves to must be listed: behind Cloudflare
        (exposure = "public" with tls = "acme-dns01") the Cloudflare edge
        ranges are seeded automatically; a directly exposed host must list its
        own public IP here instead.
      '';
    };

    subServices = mkOption {
      description = ''
        Which sibling services to run. Seeded by `profile` through
        `lib.mkDefault`; every flag is overridable per host.
      '';
      default = { };
      type = types.submodule {
        options = {
          comms = boolOpt false;
          squid = boolOpt false;
          explore = boolOpt false;
          create = boolOpt false;
          social = boolOpt false;
          data = boolOpt false;
          abCdn = boolOpt false;
          socialRpc = boolOpt false;
          explorerApi = boolOpt false;
          sceneState = boolOpt false;
          telemetry = boolOpt false;
          worldStorage = boolOpt false;
          profileImages = boolOpt false;
          signatures = boolOpt false;
          governance = boolOpt false;
          presence = boolOpt false;
          sites = boolOpt false;
        };
      };
    };

    federation = mkOption {
      description = ''
        Worlds/content federation peer allowlist. Federation is off unless a
        peer file resolves and the worlds server is told to read it. The
        module ships a default `federation-peers.toml` seed (the reference
        network peers, the one intentional public host reference in the export)
        and, when `seedDefault` is on and no `peersFile` override is set, provisions it
        to /etc/catalyrst/federation-peers.toml. The seed carries a blank
        `mtls_root_pem` on every entry, so it is a documented template that is
        refused until the peer's root certificate is supplied -- federation
        never turns on by accident.
      '';
      default = { };
      type = types.submodule {
        options = {
          seedDefault = boolOpt true;
          peers = mkOption {
            type = types.listOf types.str;
            default = [ ];
            description = ''
              Additional peer identifiers to admit beyond the shipped seed.
              Reserved for the full fed-log/NATS wiring; the shipped seed (or
              `peersFile`) is authoritative today.
            '';
          };
          peersFile = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = ''
              Explicit federation-peers.toml to use instead of the shipped
              seed. When set it wins over `seedDefault`; when null and
              `seedDefault` is on, the shipped seed is provisioned.
            '';
          };
          peerId = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = ''
              This catalyst's stable federation identity (FED_PEER_ID),
              normally its public domain. null lets the comms tier fall back
              to its DB-persisted per-instance id, which is fine until the
              node federates for real -- set a stable public name before that.
            '';
          };
          gossip = mkOption {
            type = types.enum [
              "off"
              "nats"
            ];
            default = "off";
            description = ''
              Federation gossip transport. "off" runs snapshot-pull only (no
              NATS dependency). "nats" publishes the fed log over `natsUrl`
              and fails at startup when the broker is unreachable, rather
              than silently not publishing.
            '';
          };
          natsUrl = mkOption {
            type = types.str;
            default = "nats://127.0.0.1:4222";
            description = ''
              NATS broker for federation gossip (FED_NATS_URL); read only
              when gossip = "nats". The default is the module's own nats unit
              (subServices.comms). A remote broker also needs the mTLS
              material below.
            '';
          };
          natsRootCa = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = "Root CA for a TLS NATS broker (FED_NATS_ROOT_CA).";
          };
          natsClientCert = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = ''
              Client certificate for mTLS NATS peering (FED_NATS_CLIENT_CERT);
              set together with natsClientKey.
            '';
          };
          natsClientKey = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = "Client key paired with natsClientCert (FED_NATS_CLIENT_KEY).";
          };
        };
      };
    };

    resources = mkOption {
      description = "Per-service memory caps + Postgres tuning.";
      default = { };
      type = types.submodule {
        options = {
          syncMemoryHigh = strOpt "12G";
          syncMemoryMax = strOpt "14G";
          pgSharedBuffers = strOpt "3GB";
          pgEffectiveCacheSize = strOpt "8GB";
          pulseMemoryMax = strOpt "6G";
          pulseMemoryHigh = strOpt "4G";
          livekitMemoryMax = strOpt "2G";
          natsMemoryMax = strOpt "512M";
          archipelagoMemoryMax = strOpt "1G";
          bundleMemoryMax = strOpt "1500M";
          squidMemoryMax = strOpt "5G";
          squidMemoryHigh = strOpt "4G";
        };
      };
    };

    edge = mkOption {
      description = ''
        nginx-edge behaviour knobs that exist to absorb traffic the pinned
        explorer build generates but this node cannot answer the way a full
        public edge does. Each traffic-shaping toggle defaults off, so the
        rendered nginx.conf is unchanged for it unless a host opts in; the CIDR
        lists carry loopback-only defaults.
      '';
      default = { };
      type = types.submodule {
        options = {
          superadminCidrs = mkOption {
            type = types.listOf types.str;
            default = [ "127.0.0.1" ];
            description = ''
              Client CIDRs allowed through the nginx superadmin gate fronting
              the telemetry /dash/{sql,flag,group,admin} endpoints and the sites
              admin block: each renders one `allow` line ahead of `deny all`.
              Defaults to loopback only; add a trusted internal range (a
              VPN/CGNAT block, an office prefix) to reach the admin surface from
              off-host.
            '';
          };
          silenceImposterRequests = mkOption {
            type = types.bool;
            default = false;
            description = ''
              Answer `/bvimposters/` with a bare 404 at the edge instead of
              proxying it to the content server.

              The explorer wasm has `/bvimposters` baked in as its imposter
              (distant-LOD) supply base -- a same-origin mount that only exists
              on a full public edge, where nginx proxies it to an imposters
              service. This module ships no such binary, so the requests fall
              through the catch-all to the content server and come back "Not
              found". Turning this on keeps the client-visible outcome
              identical (imposters stay absent) while sparing the content
              server the fan-out and the log the noise. It is a silencer, not a
              fix: real imposters need the service, a free port and baked tiles.
            '';
          };
          contentReadLimit = mkOption {
            description = ''
              A dedicated, wider limit_req zone for the read-heavy catalyst
              paths (`/content/`, `/lambdas/`).

              catread is declared at the *server* level of the main vhost, so
              it is inherited by every location that does not declare its own
              limit_req -- including the catch-all that carries the SDK7 scene
              pointer sweep. At the inherited rate, one client loading one
              scene can exceed it and 429 itself. Enabling this attaches an
              explicit limit_req on those two prefixes, which overrides the
              inherited catread for them and leaves catread in place everywhere
              else -- the content-server catch-all must never be left unlimited.
            '';
            default = { };
            type = types.submodule {
              options = {
                enable = boolOpt false;
                rate = strOpt "200r/s";
                burst = mkOption {
                  type = types.ints.positive;
                  default = 400;
                };
              };
            };
          };
          exemptTrustedCidrsFromRateLimit = mkOption {
            type = types.bool;
            default = false;
            description = ''
              Exempt `rateLimitExemptCidrs` (a trusted internal CIDR range) from
              the catread and content-read zones, by keying them on a variable
              that is empty for those clients (an empty key is not accounted). A
              trusted internal driver on that range can otherwise throttle
              itself. Leaves catdeploy, abgenheavy and the connection zone keyed
              on $binary_remote_addr for everyone.
            '';
          };
          rateLimitExemptCidrs = mkOption {
            type = types.listOf types.str;
            default = [ "127.0.0.1" ];
            description = ''
              CIDRs the rate-limit exemption covers when
              `exemptTrustedCidrsFromRateLimit` is on. Defaults to loopback
              only; add the trusted internal range that drives this node.
            '';
          };
        };
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    (mkIf (cfg.profile == "content-node") {
      services.catalyrst = {
        exposure = mkDefault "lan";
        tls = mkDefault "none";
      };
    })

    (mkIf isPublic {
      services.catalyrst = {
        exposure = mkDefault "public";
        tls = mkDefault "acme-dns01";
        subServices = {
          comms = mkDefault true;
          explore = mkDefault true;
          create = mkDefault true;
          social = mkDefault true;
          data = mkDefault true;
          socialRpc = mkDefault true;
          explorerApi = mkDefault true;
          worldStorage = mkDefault true;
          profileImages = mkDefault true;
          signatures = mkDefault true;
        };
      };
    })

    (mkIf (cfg.profile == "public-gateway") {
      services.catalyrst = {
        gateway.enable = mkDefault true;
        subServices = {
          abCdn = mkDefault true;
          governance = mkDefault true;
          presence = mkDefault true;
          telemetry = mkDefault true;
          squid = mkDefault true;
          sites = mkDefault true;
        };
      };
    })

    (mkIf (cfg.exposure == "public" && cfg.tls == "acme-dns01") {
      services.catalyrst.comms.archipelagoExtraIpAllow = mkDefault [
        "104.16.0.0/13"
        "172.64.0.0/13"
      ];
    })

    (mkIf (cfg.federation.seedDefault && cfg.federation.peersFile == null) {
      environment.etc."catalyrst/federation-peers.toml".source = ./federation-peers.toml;
    })
  ]);
}
