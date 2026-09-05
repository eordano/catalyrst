{
  config,
  pkgs,
  lib,
  inputs,
  ...
}:
let
  cfg = config.services.catalyrst;
  d = import ./helpers.nix cfg;
  facts = import ./facts.nix;

  # Content-addressed npm lockfile hash for scene-lod-entities-manifest-builder
  # (pinned by the `rev` below). Inlined rather than imported from colmena's
  # private globals/vendor-hashes.nix, which is outside the exported tree.
  catalyrstNpmDepsHash = "sha256-C6mcS/eBbQuN9E6+Z8QTN3vI0Qjv/0U0BBqsb7MlmQ4=";

  commsPackages = inputs.catalyrst.packages.x86_64-linux;
  # abgen comes through inputs.catalyrst's re-export (the flake's
  # packages.abgen), so the exported module's only consumer-supplied flake
  # input stays inputs.catalyrst.
  # inputs.catalyrst, never the consumer flake's own rev -- COMMIT_HASH is in
  # every bundle unit's text, and a whole-repo rev would restart all of them
  # on every unrelated consumer commit (see catalyrst-sync.nix).
  commitHash = inputs.catalyrst.shortRev or inputs.catalyrst.dirtyShortRev or "dirty";

  inherit (import ./sandbox.nix)
    baseSandbox
    ;

  catalyrstBundles =
    if cfg.bundlesPackage != null then cfg.bundlesPackage else commsPackages.catalyrst-all;

  upstreamFlags = builtins.fromJSON (builtins.readFile ./feature-flags-upstream.json);

  abgenServer = commsPackages.abgen;

  conn = db: "postgresql:///${db}?host=/run/postgresql&user=catalyrst${d.pgPortQuery}";
  connAuth = db: "postgres://catalyrst@%%2Frun%%2Fpostgresql${d.pgPortColon}/${db}";

  commonEnv = {
    RUST_LOG = "info";
    HTTP_SERVER_HOST = "127.0.0.1";
    COMMIT_HASH = commitHash;
    ETH_NETWORK = "mainnet";
    NETWORK_ID = "1";

    CONTENT_PG_CONNECTION_STRING = conn "content";
    DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "marketplace_squid";
    DAPPS_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
    DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "marketplace_squid";
    FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "marketplace_squid";
    SQUID_PG_COMPONENT_PSQL_SCHEMA = "squid_marketplace";
    PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "places";
    PLACES_EVENTS_PG_CONNECTION_STRING = conn "places_events";
    WORLDS_PG_CONNECTION_STRING = conn "worlds";
    WORLD_STORAGE_PG_CONNECTION_STRING = conn "worlds";
    BUILDER_PG_CONNECTION_STRING = conn "builder";
    CAMERA_REEL_PG_CONNECTION_STRING = conn "camera_reel";
    AB_REGISTRY_PG_CONNECTION_STRING = conn "ab_registry";
    COMMUNITIES_PG_CONNECTION_STRING = conn "communities";
    MUTES_PG_CONNECTION_STRING = conn "communities";
    COMMS_PG_CONNECTION_STRING = conn "comms";
    NOTIFICATIONS_PG_CONNECTION_STRING = conn "notifications";
    BADGES_PG_CONNECTION_STRING = conn "badges";
    MEDIA_PG_CONNECTION_STRING = conn "media";
    PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING = conn "price";
    CREDITS_PG_CONNECTION_STRING = conn "credits";
    SIGNATURES_PG_CONNECTION_STRING = conn "signatures";

    CONTENT_SERVER_ADDRESS = "${d.publicUrl}/content";
    CONTENT_URL = "${d.publicUrl}/content/";
    CONTENT_BASE_URL = "${d.publicUrl}/content";
    CONTENT_PUBLIC_URL = "${d.publicUrl}/content";
    LAMBDAS_PUBLIC_URL = "${d.publicUrl}/lambdas";
    CATALYST_URL = "http://127.0.0.1:5141";
    PLACES_API_URL = "http://127.0.0.1:5143";
    COMMS_GATEKEEPER_URL = "http://127.0.0.1:5145";
    ARCHIPELAGO_STATS_URL = "http://127.0.0.1:5139";
    PROFILE_IMAGES_URL = "${d.publicUrl}/profile-images";
    PROFILE_CDN_BASE_URL = "${d.publicUrl}/profile-images";

    LIVEKIT_HOST = d.lkHostBare;
    LIVEKIT_WS_URL = d.lkWsUrl;

    WORLDS_CONTENT_DIR = "${cfg.stateDir}/worlds/contents";
    CONTENT_STORAGE_DIR = "${cfg.stateDir}/camera-reel";
    COMMUNITIES_CONTENT_DIR = "${cfg.stateDir}/communities/content";
    ABGEN_OUT_ROOT = "${cfg.stateDir}/ab-generator/out";
  }
  # Federation knobs stay UNSET unless configured -- every FED_* consumer
  # (worlds, market, places, events, social-service, comms) treats absence as
  # snapshot-pull-only / local identity, so the default spread changes nothing.
  // lib.optionalAttrs (cfg.federation.peerId != null) {
    FED_PEER_ID = cfg.federation.peerId;
  }
  // lib.optionalAttrs (cfg.federation.gossip == "nats") {
    FED_GOSSIP = "nats";
    FED_NATS_URL = cfg.federation.natsUrl;
  }
  // lib.optionalAttrs (cfg.federation.natsRootCa != null) {
    FED_NATS_ROOT_CA = toString cfg.federation.natsRootCa;
  }
  //
    lib.optionalAttrs (cfg.federation.natsClientCert != null && cfg.federation.natsClientKey != null)
      {
        FED_NATS_CLIENT_CERT = toString cfg.federation.natsClientCert;
        FED_NATS_CLIENT_KEY = toString cfg.federation.natsClientKey;
      };

  lodManifestBuilder = pkgs.buildNpmPackage {
    pname = "scene-lod-entities-manifest-builder";
    version = "1dc76d5";
    nodejs = pkgs.nodejs_24;
    src = pkgs.fetchFromGitHub {
      owner = "decentraland";
      repo = "scene-lod-entities-manifest-builder";
      rev = "1dc76d5458bfe70e70f62d692e44af13e247b90e";
      hash = "sha256-Yo7hIJWoGPD4E53VI1zkiHvqtrFUc5vKwESj7VbkWvk=";
    };
    npmDepsHash = catalyrstNpmDepsHash;
    dontNpmPrune = true;
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -r package.json dist node_modules $out/
      runHook postInstall
    '';
  };

  # Badge art baked into the store so the social bundle's /assets ServeDir
  # (catalyrst-badges) is self-contained -- no hand-provisioned host dir. The
  # PNG tree ships in the exported source (crates/catalyrst-badges/assets),
  # so the module carries it wherever nixosModules.catalyrst is imported.
  badgesAssets = pkgs.runCommand "catalyrst-badges-assets" { } ''
    cp -r ${../crates/catalyrst-badges/assets} "$out"
  '';

  rwDirs = [
    cfg.stateDir
    "/run/postgresql"
  ];

  mkBundle =
    {
      name,
      bin,
      port,
      extraEnv ? { },
      needsLivekit ? false,
      mem ? cfg.resources.bundleMemoryMax,
      afterExtra ? [ ],
      path ? [ ],
      preStart ? null,
      extraRwDirs ? [ ],
      pkg ? catalyrstBundles,
    }:
    let
      exe = "${pkg}/bin/${bin}";
      execStart =
        if needsLivekit then
          pkgs.writeShellScript "${name}-launcher" ''
            set -a
            . "$CREDENTIALS_DIRECTORY/livekit-env"
            set +a
            exec ${exe}
          ''
        else
          exe;
    in
    {
      description = "catalyrst ${name}";
      after = [
        "postgresql.service"
        "postgresql-bundles.service"
        "network-online.target"
      ]
      ++ afterExtra;
      wants = [
        "network-online.target"
        "postgresql-bundles.service"
      ];
      wantedBy = [ "multi-user.target" ];
      unitConfig.RequiresMountsFor = [ cfg.stateDir ];
      inherit path;
      environment = commonEnv // extraEnv;
      serviceConfig =
        baseSandbox
        // {
          ExecStart = execStart;
          Restart = "always";
          RestartSec = 10;
          LimitNOFILE = 1048576;
          User = "catalyrst";
          Group = "catalyrst";
          ProtectHome = true;
          ReadWritePaths = rwDirs ++ extraRwDirs;
          MemoryHigh = mem;
          MemoryMax = mem;
          TasksMax = 1024;
          SocketBindAllow = [ "tcp:${toString port}" ];
          SocketBindDeny = "any";
        }
        // lib.optionalAttrs (preStart != null) {
          ExecStartPre = preStart;
        }
        // lib.optionalAttrs needsLivekit {
          LoadCredential = "livekit-env:${cfg.secretsDir}/livekit-api.env";
        };
    };
in
lib.mkIf cfg.enable {
  systemd.services =
    lib.optionalAttrs cfg.subServices.explore {
      catalyrst-explore = mkBundle {
        name = "explore bundle (places, events, archipelago, worlds, map, lists)";
        bin = "catalyrst-explore";
        port = 5143;
        needsLivekit = true;
        afterExtra = [ "livekit.service" ];
        extraEnv = {
          BUNDLE_HTTP_PORT = "5143";
          PLACES_DERIVE_FROM_CONTENT = "1";
          HTTP_BASE_URL = d.publicUrl;
          MAP_IMAGE_BASE_URL = "${d.publicUrl}/v2";
        }
        # Catalog mirrors ride cfg.upstream (absent env = crate default, off);
        # upstream URLs and the hourly interval are the crates' defaults. The
        # worlds mirror needs INSERT/UPDATE/DELETE on place for the bundle's
        # DB role.
        // lib.optionalAttrs cfg.upstream.mirrorEvents {
          EVENTS_MIRROR_UPSTREAM = "true";
        }
        // lib.optionalAttrs cfg.upstream.mirrorWorlds {
          WORLDS_MIRROR_UPSTREAM = "true";
        }
        # The worlds member is the WORLDS_FED_PEERS_FILE consumer; unset means
        # worlds federation off (a normal configuration, not a degraded one).
        // lib.optionalAttrs (d.fedPeersFile != null) {
          WORLDS_FED_PEERS_FILE = d.fedPeersFile;
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.create {
      catalyrst-create = mkBundle {
        name = "create bundle (builder, camera-reel, ab-registry)";
        bin = "catalyrst-create";
        port = 5144;
        extraEnv = {
          BUNDLE_HTTP_PORT = "5144";
          API_URL = d.publicUrl;
          BUILDER_CONTENT_BUCKET_URL = "${d.publicUrl}/content";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.social {
      catalyrst-social = mkBundle {
        name = "social bundle (communities, comms, notifications, badges, media)";
        bin = "catalyrst-social";
        port = 5145;
        needsLivekit = true;
        afterExtra = [ "livekit.service" ];
        extraEnv = {
          BUNDLE_HTTP_PORT = "5145";
          LAMBDAS_URL = "http://127.0.0.1:5141/lambdas";
          CDN_URL = d.publicUrl;
          WORLD_CONTENT_URL = "http://127.0.0.1:5143";
          AUTHORITATIVE_SERVER_ADDRESS = "0x265540169a73708a26c07622dbcd8555e950675e";
          TRANSLATE_BACKEND = "http";
          TRANSLATE_BACKEND_URL = "http://127.0.0.1:${toString facts.units.libretranslate.port}";
          # The base URL must match the badges.<domain> fan-out vhost
          # web-gateway.nix proxies to this bundle: the badges crate rewrites
          # every asset URL in its JSONB responses to this prefix at serve
          # time and serves /assets from BADGES_ASSETS_DIR (tower ServeDir).
          BADGES_ASSETS_DIR = "${badgesAssets}";
          BADGES_PUBLIC_ASSET_BASE_URL = "${d.scheme}://badges.${cfg.domain}";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.data {
      catalyrst-data = mkBundle {
        name = "data bundle (market, economy, price, credits, rpc)";
        bin = "catalyrst-data";
        port = 5146;
        extraEnv = {
          BUNDLE_HTTP_PORT = "5146";
          PRICE_BASE_URL = "http://127.0.0.1:5146";
          RPC_UPSTREAM_MAINNET = cfg.ethRpcUrl;
          RPC_UPSTREAM_ETHEREUM = cfg.ethRpcUrl;
          RPC_UPSTREAM_SEPOLIA = "https://rpc.decentraland.org/sepolia";
          RPC_UPSTREAM_POLYGON = "https://rpc.decentraland.org/polygon";
          RPC_UPSTREAM_AMOY = "https://rpc.decentraland.org/amoy";
          CONTRACT_ADDRESSES_URL = "${d.publicUrl}/contracts/addresses.json";
          DAPPS_PG_COMPONENT_PSQL_SCHEMA = "marketplace";
          DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA = "marketplace";
          FAVORITES_PG_COMPONENT_PSQL_SCHEMA = "favorites";
          DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING = connAuth "marketplace_squid";
          DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING = connAuth "marketplace_squid";
          FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING = connAuth "marketplace_squid";
        }
        # Meta-transactions relay through the upstream broadcaster until local
        # relayer credentials are provisioned (local providers take precedence).
        // lib.optionalAttrs (cfg.upstream.metaTxRelay != null) {
          TRANSACTIONS_UPSTREAM_URL = cfg.upstream.metaTxRelay;
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.abCdn {
      catalyrst-ab-cdn = mkBundle {
        name = "ab-cdn (asset-bundle CDN, abgen)";
        bin = "abgen";
        pkg = abgenServer;
        port = 5147;
        path = [
          pkgs.nodejs_24
          pkgs.bash
          pkgs.coreutils
        ];
        preStart = pkgs.writeShellScript "abgen-lod-seed" ''
          set -eu
          pkg="${lodManifestBuilder}"
          work="${cfg.stateDir}/lod-cache/lod-work"
          stamp="$work/.seeded-from"
          if [ "$(${pkgs.coreutils}/bin/cat "$stamp" 2>/dev/null || true)" != "$pkg" ]; then
            ${pkgs.coreutils}/bin/rm -rf "$work"
            ${pkgs.coreutils}/bin/mkdir -p "$work"
            ${pkgs.coreutils}/bin/cp -r --no-preserve=mode,ownership \
              "$pkg/package.json" "$pkg/dist" "$pkg/node_modules" "$work/"
            ${pkgs.coreutils}/bin/echo -n "$pkg" > "$stamp"
          fi
        '';
        extraEnv = {
          HTTP_SERVER_PORT = "5147";
          ABGEN_CACHE_DIR = "${cfg.stateDir}/ab-generator/serve-cache";
          ABGEN_LOD_JIT = "1";
          ABGEN_SIMPLIFIER = "meshopt";
          ABGEN_UPSTREAM_AB_CDN = "https://ab-cdn.decentraland.org";
          ABGEN_LOD_MANIFEST_BUILDER = "${lodManifestBuilder}";
          ABGEN_LOD_CACHE_DIR = "${cfg.stateDir}/lod-cache";
          HOME = "${cfg.stateDir}/lod-cache/home";
          npm_config_cache = "${cfg.stateDir}/lod-cache/npm-cache";
          npm_config_update_notifier = "false";
          npm_config_script_shell = "${pkgs.bash}/bin/bash";
          CATALYST_URL = d.publicUrl;
        }
        # A missing texture dependency renders magenta and the bundle still
        # completes (upstream asset-bundle-converter tolerance); absent, one
        # mis-pathed texture fails the whole scene's bundle.
        // lib.optionalAttrs cfg.abgenMagentaMissing {
          ABGEN_MAGENTA_MISSING = "true";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.socialRpc {
      catalyrst-social-rpc = mkBundle {
        name = "social-rpc (dcl-rpc WebSocket: friends/presence/voice)";
        bin = "catalyrst-social-rpc";
        port = 5148;
        afterExtra = [ "catalyrst-social.service" ];
        extraEnv = {
          HTTP_SERVER_PORT = "5148";
          DATABASE_URL = conn "communities";
        };
      };
    }
    // lib.optionalAttrs cfg.subServices.explorerApi {
      catalyrst-explorer-api = {
        description = "catalyrst explorer-api (realm provider + auth + feature flags)";
        after = [ "network-online.target" ];
        wants = [ "network-online.target" ];
        wantedBy = [ "multi-user.target" ];
        restartTriggers = [
          config.environment.etc."catalyrst/feature-flags.json".text
          config.environment.etc."catalyrst/denylist.json".text
        ];
        environment = {
          RUST_LOG = "info";
          HTTP_SERVER_HOST = "127.0.0.1";
          HTTP_SERVER_PORT = "5137";
          REALM_NAME = cfg.realm;
          ENV_NAME = "prd";
          NETWORK_ID = "1";
          CATALYST_URL = "http://127.0.0.1:5141";
          LAMBDAS_URL = "${d.publicUrl}/lambdas";
          PUBLIC_REALM_URL = d.publicUrl;
          HOT_SCENES_URL = "http://127.0.0.1:5143/hot-scenes";
          FEATURE_FLAGS_CONFIG_PATH = "/etc/catalyrst/feature-flags.json";
          BLOCKLIST_PATH = "/etc/catalyrst/denylist.json";
        };
        serviceConfig = baseSandbox // {
          ExecStart = "${catalyrstBundles}/bin/catalyrst-explorer-api";
          Restart = "always";
          RestartSec = 10;
          User = "catalyrst";
          Group = "catalyrst";
          ProtectHome = true;
          MemoryHigh = "512M";
          MemoryMax = "512M";
          TasksMax = 256;
          SocketBindAllow = [ "tcp:5137" ];
          SocketBindDeny = "any";
        };
      };
    };

  environment.etc = lib.mkIf cfg.subServices.explorerApi {
    "catalyrst/feature-flags.json".text = builtins.toJSON {
      flags =
        upstreamFlags.flags
        // lib.optionalAttrs cfg.gateway.enable {
          "explorer-use-gateway" = true;
          "explorer-alfa-minimum-requirements" = true;
        };
      variants =
        upstreamFlags.variants
        // lib.optionalAttrs cfg.gateway.enable {
          "explorer-alfa-minimum-requirements" = {
            name = "minimum_requirements";
            payload = {
              type = "json";
              value = builtins.toJSON {
                windows_supported_versions = [
                  "Windows 10"
                  "Windows 11"
                ];
                mac_supported_versions = [
                  "Mac OS X"
                  "macOS"
                ];
                integrated_gpu_supported_versions = [
                  "intel(r) hd graphics"
                  "intel(r) uhd graphics"
                  "intel iris"
                  "iris(r) xe graphics"
                  "amd radeon(tm) graphics"
                  "amd radeon graphics"
                  "amd radeon vega"
                  "amd radeon r5"
                  "amd radeon r6"
                  "amd radeon r7"
                ];
                always_accepted_cpus = [
                  "threadripper"
                  "core ultra"
                  "core 5"
                  "core 7"
                  "core 9"
                ];
                minimum_macos_major_version = 11;
                macos_supported_version_regex = "(\\d+)\\.\\d+";
                ryzen_supported_cpu_regex = "ryzen.*?(\\d+)";
                ryzen_supported_minimum_series = 5;
                intel_supported_minimum_series = 5;
                intel_supported_minimum_generation = 7;
                intel_ultra_supported_minimum_generation = 5;
                intel_cpu_supported_version_regex = "i([3579])-?(\\d{4,5})";
                intel_ultra_cpu_supported_version_regex = "ultra\\s+([579])";
                rtx_gpu_supported_version_regex = "rtx\\s*(\\d{4})";
                rx_gpu_supported_version_regex = "rx\\s*(\\d{4})";
                arc_gpu_supported_version_regex = "a(\\d{3})";
                minimum_rtx_supported_version = 2000;
                minimum_rx_supported_version = 5000;
                minimum_arc_supported_version = 500;
                apple_silicon_supported_regex = "apple\\s+m\\d";
              };
            };
            enabled = true;
          };
        };
    };
    "catalyrst/denylist.json".text = ''{"users":[],"names":[],"coordinates":[]}'';
  };

  systemd.tmpfiles.rules = [
    "d ${cfg.stateDir}/worlds              0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/worlds/contents     0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/camera-reel         0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/communities         0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/communities/content 0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/ab-generator             0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/ab-generator/out         0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/ab-generator/serve-cache 0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/lod-cache           0755 catalyrst catalyrst -"
    "d ${cfg.stateDir}/lod-cache/home      0700 catalyrst catalyrst -"
    "d ${cfg.stateDir}/lod-cache/npm-cache 0755 catalyrst catalyrst -"
  ];
}
