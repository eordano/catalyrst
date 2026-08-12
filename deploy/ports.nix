{
  # scope: "core" = upstreamable realm product | "overlay" = this operator's extras
  #        | "instance-infra" = per-instance plumbing (front, proxy, DB, bus).
  # Definitions, rationale table, and the rule for new services: deploy/SCOPE.md.
  umbrella = {
    squid-api             = { port = 5130; scope = "core";           env = "squid"; portVar = "GQL_PORT"; unit = null;                           upstream = null; }; # RETIRED 2026-07-31 — GraphQL layer off; port reserved, processors live on 5131/5132
    squid-eth-metrics     = { port = 5131; scope = "core";           env = "squid"; portVar = "ETH_PROMETHEUS_PORT"; unit = null;                upstream = null; };
    squid-polygon-metrics = { port = 5132; scope = "core";           env = "squid"; portVar = "POLYGON_PROMETHEUS_PORT"; unit = null;            upstream = null; };
    market                = { port = 5133; scope = "core";           env = "catalyrst-market";        unit = "umbrella-catalyrst-market";        upstream = "cat_market"; };
    places                = { port = 5134; scope = "core";           env = "catalyrst-places";        unit = "umbrella-catalyrst-places";        upstream = "cat_places"; };
    events                = { port = 5135; scope = "core";           env = "catalyrst-events";        unit = "umbrella-catalyrst-events";        upstream = "cat_events"; };
    communities           = { port = 5136; scope = "core";           env = "catalyrst-communities";   unit = "umbrella-catalyrst-communities";   upstream = "cat_communities"; };
    explorer-api          = { port = 5137; scope = "core";           env = "catalyrst-explorer-api";  unit = "umbrella-catalyrst-explorer-api";  upstream = "cat_explorer_api"; };
    comms                 = { port = 5138; scope = "core";           env = "catalyrst-comms";         unit = "umbrella-catalyrst-comms";         upstream = "cat_comms"; };
    archipelago           = { port = 5139; scope = "core";           env = "catalyrst-archipelago";   unit = "umbrella-catalyrst-archipelago";   upstream = "cat_archipelago"; };
    content               = { port = 5141; scope = "core";           env = "catalyrst-content";       unit = "umbrella-catalyrst";               upstream = "cat_content"; };
    worlds                = { port = 5142; scope = "core";           env = "catalyrst-worlds";        unit = "umbrella-catalyrst-worlds";        upstream = "cat_worlds"; };
    world-storage         = { port = 5143; scope = "core";           env = "catalyrst-world-storage"; unit = "umbrella-catalyrst-world-storage"; upstream = "cat_world_storage"; };
    builder               = { port = 5144; scope = "core";           env = "catalyrst-builder";       unit = "umbrella-catalyrst-builder";       upstream = "cat_builder"; };
    badges                = { port = 5145; scope = "core";           env = "catalyrst-badges";        unit = "umbrella-catalyrst-badges";        upstream = "cat_badges"; };
    credits               = { port = 5146; scope = "core";           env = "catalyrst-credits";       unit = "umbrella-catalyrst-credits";       upstream = "cat_credits"; };
    abgen                 = { port = 5147; scope = "core";           env = "catalyrst-abgen";         unit = "umbrella-catalyrst-abgen";         upstream = "cat_abgen"; };
    notifications         = { port = 5148; scope = "core";           env = "catalyrst-notifications"; unit = "umbrella-catalyrst-notifications"; upstream = "cat_notifications"; };
    social-rpc            = { port = 5149; scope = "core";           env = "catalyrst-social-rpc";    unit = "umbrella-catalyrst-social-rpc";    upstream = null; directProxy = true; };
    telemetry             = { port = 5150; scope = "core";           env = "catalyrst-telemetry";     unit = "umbrella-catalyrst-telemetry";     upstream = "cat_telemetry"; };
    governance            = { port = 5151; scope = "core";           env = "catalyrst-governance";    unit = "umbrella-catalyrst-governance";    upstream = "cat_governance"; }; # discourse/snapshot enrichment reads the :5434 archives (umbrella-discourse-archive / umbrella-snapshot-archive writers) — lore exception CLOSED 2026-07-31, see SCOPE.md
    presence              = { port = 5152; scope = "core";           env = "catalyrst-presence";      unit = "umbrella-catalyrst-presence";      upstream = "cat_presence"; };
    metabase              = { port = 5153; scope = "overlay";        env = "metabase"; portVar = "MB_JETTY_PORT"; unit = "umbrella-metabase";    upstream = "cat_metabase"; };
    bvimposters           = { port = 5154; scope = "core";           env = "catalyrst-bvimposters";   unit = "umbrella-catalyrst-bvimposters";   upstream = "cat_bvimposters"; };
    economy               = { port = 5155; scope = "core";           env = "catalyrst-economy";       unit = "umbrella-catalyrst-economy";       upstream = "cat_economy"; };
    price                 = { port = 5156; scope = "core";           env = "catalyrst-price";         unit = "umbrella-catalyrst-price";         upstream = "cat_price"; };
    media                 = { port = 5157; scope = "core";           env = "catalyrst-media";         unit = "umbrella-catalyrst-media";         upstream = "cat_media"; };
    sites                 = { port = 5158; scope = "instance-infra"; env = "sites"; portVar = "PORT"; unit = "umbrella-sites";                   upstream = "cat_sites"; };
    map                   = { port = 5162; scope = "core";           env = "catalyrst-map";           unit = "umbrella-catalyrst-map";           upstream = "cat_map"; };
    camera-reel           = { port = 5163; scope = "core";           env = "catalyrst-camera-reel";   unit = "umbrella-catalyrst-camera-reel";   upstream = "cat_camera_reel"; };
    vrm-renders           = { port = 5164; scope = "overlay";        env = null;                      unit = null;                               upstream = null; };
    sync                  = { port = 5166; scope = "core";           env = "catalyrst-sync";          unit = "umbrella-catalyrst-sync";          upstream = null; };
    bvwebgpu              = { port = 5167; scope = "overlay";        env = "catalyrst-bvwebgpu";      unit = "umbrella-catalyrst-bvwebgpu";      upstream = "cat_bvwebgpu"; };
    preview-tunnel        = { port = 5168; scope = "overlay";        env = "preview-tunnel";          unit = "umbrella-preview-tunnel";          upstream = "cat_preview_tunnel"; };
    editor-scene          = { port = 5171; scope = "overlay";        env = null;                      unit = "umbrella-editor-scene";            upstream = "cat_editor_scene"; };
    project-realm         = { port = 5172; scope = "overlay";        env = null;                      unit = "umbrella-project-realm";           upstream = "cat_project_realm"; };
    slides                = { port = 5190; scope = "overlay";        env = null;                      unit = "umbrella-slides";                  upstream = "cat_slides"; };
    abgen-compare         = { port = 5198; scope = "overlay";        env = "abgen-compare"; portVar = null; unit = "umbrella-abgen-compare";     upstream = "abgen_compare"; };
    scene-state           = { port = 5209; scope = "core";           env = "catalyrst-scene-state";   unit = "umbrella-catalyrst-scene-state";   upstream = null; directProxy = true; };

    nginx-http            = { port = 5080; scope = "instance-infra"; env = null; unit = "umbrella-nginx";    upstream = null; };
    nginx-tls             = { port = 5443; scope = "instance-infra"; env = null; unit = "umbrella-nginx";    upstream = null; };
    postgres              = { port = 5434; scope = "instance-infra"; env = null; portVar = null; unit = "umbrella-postgres"; upstream = null; };
    nats-client           = { port = 4222; scope = "instance-infra"; env = null; unit = "umbrella-nats";     upstream = null; };
    nats-monitor          = { port = 8222; scope = "instance-infra"; env = null; unit = "umbrella-nats";     upstream = null; };
    livekit-signaling     = { port = 7880; scope = "instance-infra"; env = null; unit = "umbrella-livekit";  upstream = null; directProxy = true; };
    livekit-media-udp     = { port = 7882; scope = "instance-infra"; env = null; unit = "umbrella-livekit";  upstream = null; };
  };

  # Non-catalyrst listeners (lore overlay + dev tools); int-valued by design,
  # no scope field — see SCOPE.md "External ports".
  external = {
    ui3-storybook-dev      = 5006;
    forgejo                = 5160;
    code-intel             = 5170;
    ui3-overlay-dev        = 5174;
    lorebook               = 5180;
    sites-multiplayer-test = 5197;
    vrm-gallery            = 5200;
    avatar-render          = 5557;
  };
}
