
{
  services = {
    "abgen-compare" = {
      secret = [ ];
      config = [
        "ABGEN_AB_CDN"
        "ABGEN_CATALYST_URL"
        "ABGEN_JIT_GENERATED_DIR"
        "ABGEN_PLATFORMS"
        "ABGEN_RATE_TRUSTED_NETS"
        "ABGEN_RUNS_DIR"
        "ABGEN_URL"
      ];
      prodFallback = { };
    };
    "catalyrst-abgen" = {
      secret = [
        "CONTENT_PG_CONNECTION_STRING"
      ];
      config = [
        "ABGEN_CACHE_DIR"
        "ABGEN_CATALYST_URL"
        "ABGEN_GLTFPACK"
        "ABGEN_GPU"
        "ABGEN_JIT_CONTENT_DIGEST"
        "ABGEN_LOD_BUILD_CONCURRENCY"
        "ABGEN_LOD_JIT"
        "ABGEN_LOD_JIT_FAIL_TTL_S"
        "ABGEN_LOD_JIT_TIMEOUT_S"
        "ABGEN_LOD_MANIFEST_BUILDER"
        "ABGEN_LOD_SUBPROC_TIMEOUT_S"
        "ABGEN_OUT_ROOT"
        "ABGEN_ROOT"
        "ABGEN_SHADER_BUNDLE"
        "ABGEN_SIMPLIFIER"
        "ABGEN_VERSION"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LD_LIBRARY_PATH"
        "RUST_LOG"
        "TURBOJPEG_LIB"
      ];
      prodFallback = { };
    };
    "catalyrst-archipelago" = {
      secret = [
        "ARCHIPELAGO_GOSSIP_HMAC_KEY"
        "CONTENT_PG_CONNECTION_STRING"
        "LIVEKIT_API_KEY"
        "LIVEKIT_API_SECRET"
        "POSTGRES_CONTENT_PASSWORD"
      ];
      config = [
        "ARCHIPELAGO_CONFIG_PATH"
        "ARCHIPELAGO_GOSSIP_PEERS"
        "ARCHIPELAGO_NODE_ID"
        "ARCHIPELAGO_REQUIRE_AUTH"
        "COMMIT_HASH"
        "COMMS_GATEKEEPER_URL"
        "CONTENT_BASE_URL"
        "DENY_LIST_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LIVEKIT_WS_URL"
        "POSTGRES_CONTENT_DB"
        "POSTGRES_CONTENT_USER"
        "POSTGRES_HOST"
        "POSTGRES_PORT"
        "RUST_LOG"
      ];
      prodFallback = {
        CONTENT_BASE_URL = "https://peer.decentraland.org/content";
        DENY_LIST_URL = "https://config.decentraland.org/denylist.json";
      };
    };
    "catalyrst-badges" = {
      secret = [
        "BADGES_PG_CONNECTION_STRING"
        "CATALYRST_BADGES_ADMIN_TOKEN"
      ];
      config = [
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-builder" = {
      secret = [
        "BUILDER_MARKETPLACE_PG_CONNECTION_STRING"
        "BUILDER_PG_CONNECTION_STRING"
        "CATALYRST_BUILDER_ADMIN_TOKEN"
        "NEWSLETTER_SERVICE_API_KEY"
      ];
      config = [
        "BUILDER_ADMIN_ADDRESSES"
        "BUILDER_CONTENT_BUCKET_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "NEWSLETTER_PUBLICATION_ID"
        "NEWSLETTER_SERVICE_URL"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-bvimposters" = {
      secret = [ ];
      config = [
        "BVIMPOSTERS_BAKE_ENABLED"
        "BVIMPOSTERS_BAKE_MAX_FAILURES"
        "BVIMPOSTERS_BAKE_QUARANTINE_SECS"
        "BVIMPOSTERS_BAKE_QUEUE_DEPTH"
        "BVIMPOSTERS_BAKE_TIMEOUT_SECS"
        "BVIMPOSTERS_BAKE_WRAPPER"
        "BVIMPOSTERS_CDN_BASE"
        "BVIMPOSTERS_CDN_REALM_SEGMENT"
        "BVIMPOSTERS_IMPOST_BIN"
        "BVIMPOSTERS_IMPOST_CONTENT_SERVER"
        "BVIMPOSTERS_IMPOST_SERVER"
        "BVIMPOSTERS_QUARANTINE_LIST"
        "BVIMPOSTERS_READTHROUGH_TIMEOUT_SECS"
        "BVIMPOSTERS_STORE_MAX_BYTES"
        "BVIMPOSTERS_STORE_ROOT"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-bvwebgpu" = {
      secret = [ ];
      config = [
        "ABGEN_BVWEBGPU"
        "ABGEN_BVWEBGPU_MESHOPT"
        "ABGEN_CACHE_DIR"
        "ABGEN_CATALYST_URL"
        "ABGEN_GPU_BACKEND"
        "ABGEN_JIT_CACHE_MAX_BYTES"
        "ABGEN_OUT_ROOT"
        "ABGEN_ROOT"
        "ABGEN_SHADER_JIT"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
        "TURBOJPEG_LIB"
      ];
      prodFallback = { };
    };
    "catalyrst-camera-reel" = {
      secret = [
        "CAMERA_REEL_PG_CONNECTION_STRING"
        "CATALYRST_CAMERA_REEL_ADMIN_TOKEN"
      ];
      config = [
        "API_URL"
        "BUCKET_URL"
        "CONTENT_STORAGE_DIR"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "MAX_IMAGES_PER_USER"
        "PLACES_API_URL"
        "PLACES_CACHE_MAX_SIZE"
        "PLACES_CACHE_TTL_SECONDS"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-comms" = {
      secret = [
        "COMMS_GATEKEEPER_AUTH_TOKEN"
        "COMMS_PG_CONNECTION_STRING"
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "LIVEKIT_API_KEY"
        "LIVEKIT_API_SECRET"
        "LIVEKIT_WEBHOOK_KEY"
        "MODERATOR_TOKEN"
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING"
      ];
      config = [
        "AUTHORITATIVE_SERVER_ADDRESS"
        "CATALYST_URL"
        "COMMUNITY_VOICE_CHAT_NO_MODERATOR_TTL"
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA"
        "FED_PEER_ID"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAMBDAS_URL"
        "LIVEKIT_ALLOW_DEV_CREDS"
        "LIVEKIT_HOST"
        "LIVEKIT_TOKEN_TTL_SECS"
        "LIVEKIT_WS_URL"
        "PLACES_API_URL"
        "PLATFORM_USER_MODERATORS"
        "PRIVATE_MESSAGES_ROOM_ID"
        "RUST_LOG"
        "VOICE_CHAT_CONNECTION_INTERRUPTED_TTL"
        "VOICE_CHAT_INITIAL_CONNECTION_TTL"
        "WORLD_CONTENT_URL"
      ];
      prodFallback = {
        LAMBDAS_URL = "https://peer.decentraland.org/lambdas";
        WORLD_CONTENT_URL = "https://worlds-content-server.decentraland.org";
      };
    };
    "catalyrst-communities" = {
      secret = [
        "API_ADMIN_TOKEN"
        "COMMUNITIES_PG_CONNECTION_STRING"
        "CONTENT_PG_CONNECTION_STRING"
        "MUTES_PG_CONNECTION_STRING"
      ];
      config = [
        "CDN_URL"
        "COMMUNITIES_CONTENT_DIR"
        "COMMUNITIES_GLOBAL_MODERATORS"
        "CONTENT_SERVER_ADDRESS"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PLACES_API_URL"
        "RUST_LOG"
      ];
      prodFallback = {
        CDN_URL = "https://cdn.decentraland.org";
        CONTENT_SERVER_ADDRESS = "https://peer.decentraland.org/content/";
      };
    };
    "catalyrst-content" = {
      secret = [
        "POSTGRES_CONTENT_PASSWORD"
      ];
      config = [
        "BOOTSTRAP_FROM_SCRATCH"
        "CONTENT_SERVER_ADDRESS"
        "CONTENT_URL"
        "ETH_NETWORK"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAMBDAS_URL"
        "LAND_IMAGE_BASE_URL"
        "LOG_LEVEL"
        "LOG_REQUESTS"
        "MAP_PARCEL_VIEW_URL"
        "MAP_SATELLITE_BASE_URL"
        "MAP_SATELLITE_SUFFIX"
        "POSTGRES_CONTENT_DB"
        "POSTGRES_CONTENT_USER"
        "POSTGRES_HOST"
        "POSTGRES_PORT"
        "PROFILE_CDN_BASE_URL"
        "PUBLIC_URL"
        "STORAGE_ROOT_FOLDER"
      ];
      prodFallback = { };
    };
    "catalyrst-credits" = {
      secret = [
        "CATALYRST_CREDITS_ADMIN_TOKEN"
        "CATALYRST_ECONOMY_ADMIN_TOKEN"
        "CREDITS_CAPTCHA_SECRET"
        "CREDITS_PG_CONNECTION_STRING"
        "PROGRESS_PRESENCE_PG_CONNECTION_STRING"
        "STRIPE_SECRET_KEY"
        "STRIPE_WEBHOOK_SECRET"
        "USAGE_GRANTS_PG_CONNECTION_STRING"
      ];
      config = [
        "CHECKOUT_FULFILLMENT_MODE"
        "CHECKOUT_MAX_ATTEMPTS"
        "CHECKOUT_WORKER_INTERVAL_SECS"
        "CREDITS_CAPTCHA_VERIFY_URL"
        "CREDITS_CURRENCY"
        "CREDITS_MOCK_CARD"
        "CREDITS_MOCK_FULFILLMENT"
        "CREDITS_REQUIRE_PURCHASE_INTENT"
        "ECONOMY_BASE_URL"
        "ESCROW_LOCK_DAYS"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LANDILER_ESCROW_ADDRESS"
        "MANA_PRICE_MAX_STALENESS_SECS"
        "MARKETPLACE_MARKUP_BPS"
        "MARKET_BASE_URL"
        "PRICE_BASE_URL"
        "RUST_LOG"
        "STRIPE_API_BASE"
      ];
      prodFallback = { };
    };
    "catalyrst-economy" = {
      secret = [
        "CATALYRST_ECONOMY_ADMIN_TOKEN"
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "ETH_RPC_URL"
        "OZ_RELAYER_API_KEY"
        "RELAYER_PRIVATE_KEY"
        "RPC_URL"
      ];
      config = [
        "API_VERSION"
        "BROKER_RECONCILE_INTERVAL_MS"
        "COLLECTIONS_CHAIN_ID"
        "COLLECTIONS_FETCH_INTERVAL_MS"
        "CONTRACT_ADDRESSES_CHAIN_KEY"
        "CONTRACT_ADDRESSES_URL"
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LANDILER_ESCROW_ADDRESS"
        "MANA_USD_AGGREGATOR_ADDRESS"
        "MAX_GAS_LIMIT"
        "MAX_GAS_PRICE_ALLOWED_IN_WEI"
        "MAX_TRANSACTIONS_PER_DAY"
        "META_TX_BROADCAST_ENABLED"
        "MIN_SALE_VALUE_IN_WEI"
        "NAMES_CHAIN_ID"
        "NAMES_MAX_PRICE_WEI"
        "OZ_MAX_STATUS_CHECKS"
        "OZ_RELAYER_ID"
        "OZ_RELAYER_SPEED"
        "OZ_RELAYER_URL"
        "OZ_SLEEP_TIME_BETWEEN_CHECKS_MS"
        "RECEIPT_POLL_INTERVAL_MS"
        "RECEIPT_TIMEOUT_MS"
        "RUST_LOG"
        "SQUID_PG_COMPONENT_PSQL_SCHEMA"
        "USD_PEGGED_ORACLE_MAX_AGE_SECS"
        "USD_PEGGED_SLIPPAGE_BPS"
      ];
      prodFallback = { };
    };
    "catalyrst-events" = {
      secret = [
        "CATALYRST_EVENTS_ADMIN_TOKEN"
        "PLACES_EVENTS_PG_CONNECTION_STRING"
      ];
      config = [
        "CATALYRST_EVENTS_CONTENT_DIR"
        "COMMS_GATEKEEPER_URL"
        "EVENTS_BASE_URL"
        "EVENTS_MIRROR_UPSTREAM"
        "EVENTS_UPSTREAM_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-explorer-api" = {
      secret = [
        "CATALYRST_EXPLORER_API_ADMIN_TOKEN"
        "ONBOARDING_API_KEY"
      ];
      config = [
        "BFF_URL"
        "BLOCKLIST_PATH"
        "CATALYST_URL"
        "COMMS_ADAPTER"
        "COMMS_FIXED_ADAPTER"
        "COMMS_URL"
        "ENV_NAME"
        "FEATURE_FLAGS_CONFIG_PATH"
        "HOT_SCENES_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAMBDAS_URL"
        "MAP_PARCEL_VIEW_URL"
        "MAP_SATELLITE_BASE_URL"
        "NETWORK_ID"
        "PUBLIC_REALM_URL"
        "REALM_NAME"
        "RUST_LOG"
        "UPSTREAM_BUILDER_URL"
        "UPSTREAM_MARKETPLACE_URL"
        "UPSTREAM_WORLDS_CONTENT_URL"
        "UPSTREAM_WORLDS_URL"
      ];
      prodFallback = { };
    };
    "catalyrst-governance" = {
      secret = [
        "DISCOURSE_DATABASE_URL"
        "GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "SNAPSHOT_DATABASE_URL"
      ];
      config = [
        "GOVERNANCE_API_URL"
        "GOVERNANCE_POLL_ENABLED"
        "GOVERNANCE_SYNC_WINDOW_HOURS"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-map" = {
      secret = [
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING"
      ];
      config = [
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA"
        "DISSOLVED_ESTATE_URL"
        "ESTATE_CONTRACT_ADDRESS"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAND_CONTRACT_ADDRESS"
        "MAP_EXTERNAL_BASE_URL"
        "MAP_IMAGE_BASE_URL"
        "MAP_REFRESH_INTERVAL_SECS"
        "MAP_TILES_TTL_SECONDS"
        "RENTALS_SIGNATURES_SERVER_URL"
        "RUST_LOG"
        "SATELLITE_OUTPUT_ENTRIES"
        "SATELLITE_SCAN_SECONDS"
        "SATELLITE_SOURCE_BUDGET_MB"
        "SATELLITE_TILES_DIR"
        "SIGNATURES_SERVER_URL"
      ];
      prodFallback = { };
    };
    "catalyrst-market" = {
      secret = [
        "CATALYRST_MARKET_ADMIN_TOKEN"
        "CONTENT_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "TRADE_RPC_URLS"
      ];
      config = [
        "CATALYRST_MARKET_HTTP_CACHE_TTL_SECS"
        "CATALYRST_MARKET_TRADES_PAGINATION"
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA"
        "DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA"
        "ETHEREUM_CHAIN_ID"
        "FAVORITES_PG_COMPONENT_PSQL_SCHEMA"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "MANA_ORACLE_MAX_STALENESS_SECONDS"
        "MANA_RATE_REFRESH_INTERVAL_MS"
        "MANA_RATE_STARTUP_TIMEOUT_MS"
        "MANA_USD_FALLBACK_RATE"
        "PEER_BASE_URL"
        "POLYGON_CHAIN_ID"
        "PRICE_BASE_URL"
        "RUST_LOG"
        "TRADES_SYNC_INTERVAL_SECS"
        "TRADES_SYNC_UPSTREAM_URL"
      ];
      prodFallback = { };
    };
    "catalyrst-media" = {
      secret = [
        "MEDIA_PG_CONNECTION_STRING"
        "TRANSLATE_BACKEND_API_KEY"
      ];
      config = [
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
        "TRANSLATE_BACKEND"
        "TRANSLATE_BACKEND_URL"
        "TRANSLATE_LLM_BASE_URL"
        "TRANSLATE_LLM_MODEL"
      ];
      prodFallback = { };
    };
    "catalyrst-notifications" = {
      secret = [
        "CATALYRST_NOTIFICATIONS_ADMIN_TOKEN"
        "CONTENT_PG_CONNECTION_STRING"
        "NOTIFICATIONS_PG_CONNECTION_STRING"
        "SENDGRID_API_KEY"
        "SOCIAL_PG_CONNECTION_STRING"
        "SQUID_PG_CONNECTION_STRING"
        "TELEMETRY_PG_CONNECTION_STRING"
        "TURNSTILE_SECRET_KEY"
      ];
      config = [
        "ACCOUNT_BASE_URL"
        "EMAIL_DOMAIN_BLACKLIST"
        "FIRST_WEAR_IMAGE_BASE"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "MARKETPLACE_BASE_URL"
        "RUST_LOG"
        "SENDGRID_FROM_EMAIL"
        "SENDGRID_VALIDATE_CREDITS_EMAIL_TEMPLATE_ID"
        "SENDGRID_VALIDATE_EMAIL_TEMPLATE_ID"
        "SHOP_ITEM_BASE_URL"
      ];
      prodFallback = {
        FIRST_WEAR_IMAGE_BASE = "https://peer.decentraland.org";
      };
    };
    "catalyrst-places" = {
      secret = [
        "AWS_ACCESS_KEY"
        "AWS_ACCESS_SECRET"
        "CONTENT_PG_CONNECTION_STRING"
        "DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "DATA_TEAM_AUTH_TOKEN"
        "PLACES_ADMIN_AUTH_TOKEN"
        "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING"
        "PLACES_PG_COMPONENT_WRITER_PSQL_CONNECTION_STRING"
      ];
      config = [
        "AWS_BUCKET_NAME"
        "AWS_ENDPOINT"
        "AWS_REGION"
        "BUCKET_HOSTNAME"
        "COMMS_GATEKEEPER_URL"
        "CONTENT_PUBLIC_URL"
        "DAPPS_PG_COMPONENT_PSQL_SCHEMA"
        "EVENTS_API_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PLACES_ADMIN_ADDRESSES"
        "PLACES_DERIVE_FROM_CONTENT"
        "PLACES_MIRROR_UPSTREAM"
        "PLACES_REPORT_LOCAL_FALLBACK"
        "PLACES_UPSTREAM_URL"
        "PRESENCE_URL"
        "RUST_LOG"
      ];
      prodFallback = {
        COMMS_GATEKEEPER_URL = "https://comms-gatekeeper.decentraland.zone";
        EVENTS_API_URL = "https://events.decentraland.zone/api";
      };
    };
    "catalyrst-presence" = {
      secret = [
        "PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING"
      ];
      config = [
        "ARCHIPELAGO_URL"
        "COMMS_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PRESENCE_GENESIS_REALM"
        "PRESENCE_SNAPSHOT_INTERVAL_SECS"
        "RUST_LOG"
        "WORLDS_SERVER_URL"
      ];
      prodFallback = { };
    };
    "catalyrst-price" = {
      secret = [
        "CATALYRST_PRICE_ADMIN_TOKEN"
        "PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING"
      ];
      config = [
        "COINGECKO_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PRICE_POLL_ENABLED"
        "PRICE_POLL_INTERVAL_SECS"
        "RUST_LOG"
      ];
      prodFallback = { };
    };
    "catalyrst-scene-state" = {
      secret = [
        "CATALYRST_SCENE_STATE_ADMIN_TOKEN"
        "DEBUGGING_SECRET"
        "DELEGATION_MINTER_TOKEN"
      ];
      config = [
        "AUTH_TIMEOUT_SECS"
        "CLIENT_INBOUND_MAX"
        "CLIENT_OUTBOUND_MAX"
        "COMMIT_HASH"
        "CRDT_MAX_COMPONENTS"
        "DELEGATION_MINTER_URL"
        "DISABLE_JS_RUNTIME"
        "FETCH_MAX_BODY_BYTES"
        "HTTP_BASE_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "JS_HEAP_LIMIT_MB"
        "JS_SHUTDOWN_JOIN_MS"
        "JS_TICK_BUDGET_MS"
        "JS_UPDATE_FAILURE_CAP"
        "LOCAL_SCENE_PATH"
        "REALM_NAME"
        "RUST_LOG"
        "SIGNED_FETCH_MAX_BODY_BYTES"
        "SIGNED_FETCH_MAX_IN_FLIGHT"
        "SIGNED_FETCH_MAX_RESPONSE_BYTES"
        "SIGNED_FETCH_TIMEOUT_MS"
        "STORAGE_ALLOW_HTTP"
        "STORAGE_DELEGATION"
        "STORAGE_URL"
        "WORLD_SERVER_URL"
        "WS_MAX_FRAME_BYTES"
      ];
      prodFallback = { };
    };
    "catalyrst-social-rpc" = {
      secret = [
        "CATALYRST_SOCIAL_RPC_ADMIN_TOKEN"
        "COMMS_GATEKEEPER_AUTH_TOKEN"
        "CONTENT_PG_CONNECTION_STRING"
        "DATABASE_URL"
      ];
      config = [
        "AUTH_WINDOW_SECS"
        "COMMS_GATEKEEPER_URL"
        "CONTENT_SERVER_ADDRESS"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PRIVATE_VOICE_CHAT_EXPIRATION_BATCH_SIZE"
        "PRIVATE_VOICE_CHAT_EXPIRATION_TIME"
        "PRIVATE_VOICE_CHAT_JOB_INTERVAL"
        "RUST_LOG"
        "WS_MAX_CONCURRENT_CONNECTIONS"
        "WS_MAX_PAYLOAD_LENGTH"
      ];
      prodFallback = { };
    };
    "catalyrst-sync" = {
      secret = [
        "POSTGRES_CONTENT_PASSWORD"
        "SQUID_DB_PASSWORD"
      ];
      config = [
        "ADDITIONAL_DECENTRALAND_ADDRESS"
        "BLOCKS_L2_SUBGRAPH_URL"
        "COMMIT_HASH"
        "CONCURRENT_SYNC_DOWNLOADS"
        "CONNECTIONS_MAX_IDLE"
        "CONTENT_SERVER_ADDRESS"
        "CONTENT_URL"
        "CONTENT_VERSION"
        "ENABLE_DEPLOYMENTS"
        "ENTITIES_CACHE_CONTROL_MAX_AGE"
        "ETH_RPC_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "IGNORE_BLOCKCHAIN_ACCESS_CHECKS"
        "LAMBDAS_URL"
        "LAMBDAS_VERSION"
        "LAND_IMAGE_BASE_URL"
        "LOG_LEVEL"
        "MAP_PARCEL_VIEW_URL"
        "MAP_SATELLITE_BASE_URL"
        "MAP_SATELLITE_SUFFIX"
        "PG_POOL_SIZE"
        "PHASED_SYNC"
        "POSTGRES_CONTENT_DB"
        "POSTGRES_CONTENT_USER"
        "POSTGRES_HOST"
        "POSTGRES_PORT"
        "PUBLIC_URL"
        "READ_ONLY"
        "RETRY_FAILED_ENABLED"
        "RETRY_FAILED_PRUNE_TTL_DAYS"
        "RPC_ENDPOINT_ETH"
        "RPC_ENDPOINT_POLYGON"
        "RUST_LOG"
        "SNAPSHOT_GENERATION_INTERVAL_HOURS"
        "SQUID_DB_HOST"
        "SQUID_DB_NAME"
        "SQUID_DB_PORT"
        "SQUID_DB_USER"
        "SQUID_PG_POOL_SIZE"
        "STORAGE_ROOT_FOLDER"
        "SYNC_DB_NAME"
        "SYNC_ENABLED"
        "SYNC_PG_POOL_SIZE"
        "SYNC_SOURCE"
        "SYNC_STORAGE_ROOT"
        "THIRD_PARTY_REFRESH_HOURS"
        "THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL"
        "THIRD_PARTY_ROOT_SOURCE"
      ];
      prodFallback = { };
    };
    "catalyrst-telemetry" = {
      secret = [
        "CATALYRST_TELEMETRY_ADMIN_TOKEN"
        "TELEMETRY_PG_CONNECTION_STRING"
      ];
      config = [
        "FLAGS_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "RUST_LOG"
        "TELEMETRY_BASE_PATH"
        "TELEMETRY_CONTRACT_PATH"
      ];
      required = [
        "CATALYRST_TELEMETRY_ADMIN_TOKEN"
        "TELEMETRY_BASE_PATH"
        "TELEMETRY_PG_CONNECTION_STRING"
      ];
      prodFallback = { };
    };
    "catalyrst-world-storage" = {
      secret = [
        "ENCRYPTION_KEY"
        "WORLD_STORAGE_PG_CONNECTION_STRING"
      ];
      config = [
        "AUTHORITATIVE_SERVER_ADDRESS"
        "AUTHORIZED_ADDRESSES"
        "CORS_ALLOWED_ORIGIN_SUFFIXES"
        "ENV_STORAGE_MAX_TOTAL_SIZE_BYTES"
        "ENV_STORAGE_MAX_VALUE_SIZE_BYTES"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAMBDAS_URL"
        "PLACES_CACHE_TTL_SECONDS"
        "PLACES_URL"
        "PLAYER_STORAGE_MAX_TOTAL_SIZE_BYTES"
        "PLAYER_STORAGE_MAX_VALUE_SIZE_BYTES"
        "RPC_ENDPOINT_ETH"
        "RUST_LOG"
        "STORAGE_CACHE_ENABLED"
        "STORAGE_CACHE_MAX"
        "STORAGE_CACHE_MAX_VALUE_BYTES"
        "STORAGE_CACHE_TTL_SECONDS"
        "WORLDS_CONTENT_SERVER_URL"
        "WORLD_PERMISSIONS_CACHE_TTL_SECONDS"
        "WORLD_STORAGE_MAX_TOTAL_SIZE_BYTES"
        "WORLD_STORAGE_MAX_VALUE_SIZE_BYTES"
      ];
      prodFallback = {
        LAMBDAS_URL = "https://peer.decentraland.org/lambdas";
        PLACES_URL = "https://places.decentraland.org";
        WORLDS_CONTENT_SERVER_URL = "https://worlds-content-server.decentraland.org";
      };
    };
    "catalyrst-worlds" = {
      secret = [
        "CATALYRST_WORLDS_ADMIN_TOKEN"
        "COMMS_GATEKEEPER_AUTH_TOKEN"
        "LIVEKIT_API_KEY"
        "LIVEKIT_API_SECRET"
        "LIVEKIT_WEBHOOK_KEY"
        "SQUID_PG_CONNECTION_STRING"
        "WORLDS_PG_CONNECTION_STRING"
      ];
      config = [
        "COMMS_GATEKEEPER_URL"
        "CONTENTS_UPSTREAM_URL"
        "CONTENT_PUBLIC_URL"
        "DCL_LISTS_URL"
        "DENYLIST_JSON_URL"
        "DEPLOYMENT_PROCESSING_TIMEOUT_MS"
        "GLOBAL_SCENES_URN"
        "HTTP_BASE_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "LAMBDAS_PUBLIC_URL"
        "LIVEKIT_ALLOW_DEV_CREDS"
        "LIVEKIT_HOST"
        "LIVEKIT_WS_URL"
        "MAX_CONCURRENT_UPLOADS"
        "MAX_IN_FLIGHT_UPLOAD_BYTES"
        "MAX_IN_FLIGHT_UPLOAD_FILES"
        "MAX_USERS_PER_WORLD"
        "MULTIPART_UPLOAD_TIMEOUT_MS"
        "NETWORK_ID"
        "RUST_LOG"
        "WORLDS_CONTENT_DIR"
        # Worlds federation. All config, no secrets: the peer file holds only
        # public material (a DAO proposal URL and a pinned ROOT certificate, which
        # is public by construction), so its path is not sensitive either.
        "WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS"
        "WORLDS_FED_MAX_RESPONSE_BYTES"
        "WORLDS_FED_MAX_WORLDS_PER_PEER"
        "WORLDS_FED_PEERS_FILE"
        "WORLDS_FED_POLL_INTERVAL_SECS"
      ];
      prodFallback = { };
    };
    "common" = {
      secret = [
        "METABASE_HTPASSWD"
        "SLIDES_HTPASSWD"
      ];
      config = [
        "BEVY_WEB_DIR"
        "CATALYST_URL"
        "COMMS_PROTOCOL"
        "EMAIL"
        "ETH_NETWORK"
        "LOG_LEVEL"
        "LOG_REQUESTS"
        "MAX_USERS"
        "REALM_NAME"
      ];
      prodFallback = { };
    };
    "content" = {
      secret = [
        "POSTGRES_CONTENT_PASSWORD"
        "SQUID_DB_PASSWORD"
      ];
      config = [
        "ADDITIONAL_DECENTRALAND_ADDRESS"
        "BLOCKS_L2_SUBGRAPH_URL"
        "BOOTSTRAP_FROM_SCRATCH"
        "COMMIT_HASH"
        "COMMS_FIXED_ADAPTER"
        "COMMS_STATS_URL"
        "COMMS_VERSION"
        "COMMS_WS_CONNECTOR_URL"
        "CONCURRENT_SYNC_DOWNLOADS"
        "CONNECTIONS_MAX_IDLE"
        "CONTENT_SERVER_ADDRESS"
        "CONTENT_URL"
        "CONTENT_VERSION"
        "ENABLE_DEPLOYMENTS"
        "ENTITIES_CACHE_CONTROL_MAX_AGE"
        "ETH_NETWORK"
        "ETH_RPC_URL"
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "IGNORE_BLOCKCHAIN_ACCESS_CHECKS"
        "LAMBDAS_URL"
        "LAMBDAS_VERSION"
        "LAND_IMAGE_BASE_URL"
        "LOG_LEVEL"
        "LOG_REQUESTS"
        "MAP_PARCEL_VIEW_URL"
        "MAP_SATELLITE_BASE_URL"
        "MAP_SATELLITE_SUFFIX"
        "PG_POOL_SIZE"
        "PHASED_SYNC"
        "POSTGRES_CONTENT_DB"
        "POSTGRES_CONTENT_USER"
        "POSTGRES_HOST"
        "POSTGRES_PORT"
        "PROFILE_CDN_BASE_URL"
        "PUBLIC_URL"
        "READ_ONLY"
        "RETRY_FAILED_ENABLED"
        "RETRY_FAILED_PRUNE_TTL_DAYS"
        "RPC_ENDPOINT_ETH"
        "RPC_ENDPOINT_POLYGON"
        "RUST_LOG"
        "SNAPSHOT_GENERATION_INTERVAL_HOURS"
        "SQUID_DB_HOST"
        "SQUID_DB_NAME"
        "SQUID_DB_PORT"
        "SQUID_DB_USER"
        "SQUID_PG_POOL_SIZE"
        "STORAGE_ROOT_FOLDER"
        "SYNC_DB_NAME"
        "SYNC_ENABLED"
        "SYNC_PG_POOL_SIZE"
        "SYNC_SOURCE"
        "SYNC_STORAGE_ROOT"
        "THIRD_PARTY_REFRESH_HOURS"
        "THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL"
        "THIRD_PARTY_ROOT_SOURCE"
      ];
      prodFallback = { };
    };
    "db" = {
      secret = [
        "METABASE_ADMIN_PASSWORD"
        "POSTGRES_ARCHIPELAGO_PASSWORD"
        "POSTGRES_ARCHIPELAGO_RO_PASSWORD"
        "POSTGRES_ATLAS_PASSWORD"
        "POSTGRES_ATLAS_RO_PASSWORD"
        "POSTGRES_BADGES_PASSWORD"
        "POSTGRES_BUILDER_PASSWORD"
        "POSTGRES_CMS_PASSWORD"
        "POSTGRES_CMS_READ_PASSWORD"
        "POSTGRES_CODE_INTEL_PASSWORD"
        "POSTGRES_COMMS_PASSWORD"
        "POSTGRES_COMMS_RO_PASSWORD"
        "POSTGRES_COMMUNITIES_PASSWORD"
        "POSTGRES_COMMUNITIES_READ_PASSWORD"
        "POSTGRES_CONTENT_PASSWORD"
        "POSTGRES_CREDITS_PASSWORD"
        "POSTGRES_DISCOURSE_PASSWORD"
        "POSTGRES_DISCOURSE_RO_PASSWORD"
        "POSTGRES_EVT_API_PASSWORD"
        "POSTGRES_EVT_WRITER_PASSWORD"
        "POSTGRES_FORGEJO_PASSWORD"
        "POSTGRES_GENESIS_CITY_PASSWORD"
        "POSTGRES_GENESIS_CITY_RO_PASSWORD"
        "POSTGRES_GOVERNANCE_PASSWORD"
        "POSTGRES_GOVERNANCE_RO_PASSWORD"
        "POSTGRES_MANA_PRICE_PASSWORD"
        "POSTGRES_MANA_PRICE_RO_PASSWORD"
        "POSTGRES_MB_RO_PASSWORD"
        "POSTGRES_MEDIA_PASSWORD"
        "POSTGRES_METABASE_PASSWORD"
        "POSTGRES_MPS_API_PASSWORD"
        "POSTGRES_NOTIFICATIONS_PASSWORD"
        "POSTGRES_NOTION_PASSWORD"
        "POSTGRES_NOTION_RO_PASSWORD"
        "POSTGRES_PASSWORD"
        "POSTGRES_PE_PASSWORD"
        "POSTGRES_PLACES_API_PASSWORD"
        "POSTGRES_PRESENCE_PASSWORD"
        "POSTGRES_PRICE_PASSWORD"
        "POSTGRES_SENTRY_PASSWORD"
        "POSTGRES_SIGNATURES_PASSWORD"
        "POSTGRES_SIGNATURES_READ_PASSWORD"
        "POSTGRES_SITUATIONS_PASSWORD"
        "POSTGRES_SITUATIONS_RO_PASSWORD"
        "POSTGRES_SLACK_PASSWORD"
        "POSTGRES_SLACK_RO_PASSWORD"
        "POSTGRES_SNAPSHOT_PASSWORD"
        "POSTGRES_SNAPSHOT_RO_PASSWORD"
        "POSTGRES_SOCIAL_RPC_PASSWORD"
        "POSTGRES_SQUID_PASSWORD"
        "POSTGRES_TELEMETRY_PASSWORD"
        "POSTGRES_WORLDS_PASSWORD"
        "POSTGRES_WORLDS_RO_PASSWORD"
        "POSTGRES_WORLD_STORAGE_PASSWORD"
        "POSTGRES_WORLD_STORAGE_READ_PASSWORD"
        "WORLD_STORAGE_ENCRYPTION_KEY"
      ];
      config = [
        "METABASE_ADMIN_EMAIL"
        "POSTGRES_ARCHIPELAGO_DB"
        "POSTGRES_ARCHIPELAGO_RO_USER"
        "POSTGRES_ARCHIPELAGO_USER"
        "POSTGRES_ATLAS_DB"
        "POSTGRES_ATLAS_RO_USER"
        "POSTGRES_ATLAS_USER"
        "POSTGRES_BADGES_DB"
        "POSTGRES_BADGES_USER"
        "POSTGRES_BUILDER_DB"
        "POSTGRES_BUILDER_USER"
        "POSTGRES_CMS_DB"
        "POSTGRES_CMS_READ_USER"
        "POSTGRES_CMS_USER"
        "POSTGRES_CODE_INTEL_DB"
        "POSTGRES_CODE_INTEL_USER"
        "POSTGRES_COMMS_DB"
        "POSTGRES_COMMS_RO_USER"
        "POSTGRES_COMMS_USER"
        "POSTGRES_COMMUNITIES_DB"
        "POSTGRES_COMMUNITIES_READ_USER"
        "POSTGRES_COMMUNITIES_USER"
        "POSTGRES_CONTENT_DB"
        "POSTGRES_CONTENT_USER"
        "POSTGRES_CREDITS_DB"
        "POSTGRES_CREDITS_USER"
        "POSTGRES_DB"
        "POSTGRES_DISCOURSE_DB"
        "POSTGRES_DISCOURSE_RO_USER"
        "POSTGRES_DISCOURSE_USER"
        "POSTGRES_EVT_API_USER"
        "POSTGRES_EVT_WRITER_USER"
        "POSTGRES_FORGEJO_DB"
        "POSTGRES_FORGEJO_USER"
        "POSTGRES_GENESIS_CITY_DB"
        "POSTGRES_GENESIS_CITY_RO_USER"
        "POSTGRES_GENESIS_CITY_USER"
        "POSTGRES_GOVERNANCE_DB"
        "POSTGRES_GOVERNANCE_RO_USER"
        "POSTGRES_GOVERNANCE_USER"
        "POSTGRES_HOST"
        "POSTGRES_MANA_PRICE_DB"
        "POSTGRES_MANA_PRICE_RO_USER"
        "POSTGRES_MANA_PRICE_USER"
        "POSTGRES_MB_RO_USER"
        "POSTGRES_MEDIA_DB"
        "POSTGRES_MEDIA_USER"
        "POSTGRES_METABASE_DB"
        "POSTGRES_METABASE_USER"
        "POSTGRES_MPS_API_USER"
        "POSTGRES_NOTIFICATIONS_DB"
        "POSTGRES_NOTIFICATIONS_USER"
        "POSTGRES_NOTION_DB"
        "POSTGRES_NOTION_RO_USER"
        "POSTGRES_NOTION_USER"
        "POSTGRES_PE_DB"
        "POSTGRES_PE_USER"
        "POSTGRES_PLACES_API_USER"
        "POSTGRES_PORT"
        "POSTGRES_PRESENCE_DB"
        "POSTGRES_PRESENCE_USER"
        "POSTGRES_PRICE_DB"
        "POSTGRES_PRICE_USER"
        "POSTGRES_SENTRY_DB"
        "POSTGRES_SENTRY_USER"
        "POSTGRES_SIGNATURES_DB"
        "POSTGRES_SIGNATURES_READ_USER"
        "POSTGRES_SIGNATURES_USER"
        "POSTGRES_SITUATIONS_DB"
        "POSTGRES_SITUATIONS_RO_USER"
        "POSTGRES_SITUATIONS_USER"
        "POSTGRES_SLACK_DB"
        "POSTGRES_SLACK_PRIVATE_DB"
        "POSTGRES_SLACK_RO_USER"
        "POSTGRES_SLACK_USER"
        "POSTGRES_SNAPSHOT_DB"
        "POSTGRES_SNAPSHOT_RO_USER"
        "POSTGRES_SNAPSHOT_USER"
        "POSTGRES_SOCIAL_RPC_DB"
        "POSTGRES_SOCIAL_RPC_USER"
        "POSTGRES_SQUID_DB"
        "POSTGRES_SQUID_SCHEMA"
        "POSTGRES_SQUID_USER"
        "POSTGRES_TELEMETRY_DB"
        "POSTGRES_TELEMETRY_USER"
        "POSTGRES_USER"
        "POSTGRES_WORLDS_DB"
        "POSTGRES_WORLDS_RO_USER"
        "POSTGRES_WORLDS_USER"
        "POSTGRES_WORLD_STORAGE_DB"
        "POSTGRES_WORLD_STORAGE_READ_USER"
        "POSTGRES_WORLD_STORAGE_USER"
      ];
      prodFallback = { };
    };
    "discourse-archive" = {
      secret = [
        "DISCOURSE_API_KEY"
        "POSTGRES_DISCOURSE_PASSWORD"
      ];
      config = [
        "DISCOURSE_API_USERNAME"
        "DISCOURSE_BASE_URL"
        "POSTGRES_DISCOURSE_DB"
        "POSTGRES_DISCOURSE_USER"
        "POSTGRES_HOST"
        "POSTGRES_PORT"
      ];
      prodFallback = { };
    };
    "metabase" = {
      secret = [
        "MB_ADMIN_PASSWORD"
        "MB_DB_PASS"
        "MB_TELEMETRY_RO_PASS"
      ];
      config = [
        "MB_ADMIN_EMAIL"
        "MB_ADMIN_FIRST_NAME"
        "MB_ADMIN_LAST_NAME"
        "MB_DB_DBNAME"
        "MB_DB_HOST"
        "MB_DB_PORT"
        "MB_DB_TYPE"
        "MB_DB_USER"
        "MB_JETTY_HOST"
        "MB_JETTY_PORT"
        "MB_SITE_NAME"
        "MB_SITE_URL"
        "MB_TELEMETRY_DS_NAME"
        "MB_TELEMETRY_RO_DB"
        "MB_TELEMETRY_RO_HOST"
        "MB_TELEMETRY_RO_PORT"
        "MB_TELEMETRY_RO_USER"
        "MB_TELEMETRY_SCHEMA"
        "MB_URL"
      ];
      required = [
        "MB_DB_DBNAME"
        "MB_DB_HOST"
        "MB_DB_PASS"
        "MB_DB_PORT"
        "MB_DB_TYPE"
        "MB_DB_USER"
        "MB_SITE_URL"
      ];
      prodFallback = { };
    };
    "preview-tunnel" = {
      secret = [
        "TUNNEL_TOKENS"
      ];
      config = [
        "HTTP_SERVER_HOST"
        "HTTP_SERVER_PORT"
        "PUBLIC_BASE_URL"
        "RUST_LOG"
        "TUNNEL_ALLOW_IDS"
        "TUNNEL_BODY_MAX_BYTES"
        "TUNNEL_GRACE_SECS"
        "TUNNEL_OPEN_TIMEOUT_SECS"
        "TUNNEL_PING_SECS"
      ];
      prodFallback = { };
    };
    "sites" = {
      secret = [
        "CATALYRST_BUILDER_ADMIN_TOKEN"
        "CATALYST_DATABASE_URL"
        "THIRDWEB_SECRET_KEY"
      ];
      config = [
        "BEVY_EDITOR_SCENE_URL"
        "BEVY_PLAY_URL"
        "BEVY_PROJECT_REALM_URL"
        "CATALYST_URL"
        "GOVERNANCE_API_URL"
        "HOST"
        "PORT"
        "TELEMETRY_URL"
        "THIRDWEB_CLIENT_ID"
        "VITE_TELEMETRY_URL"
        "WALLETCONNECT_PROJECT_ID"
        "WORLDS_URL"
      ];
      required = [
        "TELEMETRY_URL"
      ];
      prodFallback = { };
    };
    "snapshot-archive" = {
      secret = [
        "POSTGRES_SNAPSHOT_PASSWORD"
      ];
      config = [
        "POSTGRES_HOST"
        "POSTGRES_PORT"
        "POSTGRES_SNAPSHOT_DB"
        "POSTGRES_SNAPSHOT_USER"
        "SNAPSHOT_HUB_URL"
        "SNAPSHOT_SPACE"
      ];
      prodFallback = { };
    };
    "squid" = {
      secret = [
        "DB_PASS"
        "SQUID_API_KEY"
      ];
      config = [
        "DB_HOST"
        "DB_NAME"
        "DB_PORT"
        "DB_SCHEMA"
        "DB_USER"
        "ETHEREUM_CHAIN_ID"
        "ETH_PROMETHEUS_PORT"
        "FINALITY_CONFIRMATION_ETH"
        "FINALITY_CONFIRMATION_POLYGON"
        "GQL_PORT"
        "POLYGON_CHAIN_ID"
        "POLYGON_PROMETHEUS_PORT"
        "RPC_ENDPOINT_ETH"
        "RPC_ENDPOINT_POLYGON"
      ];
      prodFallback = { };
    };
  };

  waivedProdFallback = [
    "catalyrst-archipelago.CONTENT_BASE_URL"
    "catalyrst-archipelago.DENY_LIST_URL"
    "catalyrst-notifications.FIRST_WEAR_IMAGE_BASE"
    "catalyrst-places.COMMS_GATEKEEPER_URL"
    "catalyrst-places.EVENTS_API_URL"
  ];

  waivedUndocumentedReads = [
    "catalyrst-comms.LIVEKIT_WS_URL"
    "catalyrst-explorer-api.WORLDS_URL"
    "catalyrst-market.CATALYRST_MARKET_CATALOG_CACHE_TTL_SECS"
    "catalyrst-market.ETHEREUM_CHAIN_ID"
    "catalyrst-market.MARKETPLACE_BASE_URL"
    "catalyrst-market.PEER_BASE_URL"
    "catalyrst-market.POLYGON_CHAIN_ID"
    "catalyrst-market.TRADE_RPC_URLS"
    "catalyrst-media.TRANSLATE_LLM_API_KEY"
    "catalyrst-media.TRANSLATE_LLM_BASE_URL"
    "catalyrst-media.TRANSLATE_LLM_MODEL"
    "catalyrst-notifications.FIRST_WEAR_IMAGE_BASE"
    "catalyrst-server.ADMIN_ADDRESSES"
    "catalyrst-server.ADMIN_COOKIE_INSECURE"
    "catalyrst-server.ADMIN_SESSION_TTL_SECS"
    "catalyrst-server.BLOOM_FILTER_EXPECTED_ELEMENTS"
    "catalyrst-server.CATALYRST_SERVICE_URLS"
    "catalyrst-server.COMMS_COMMIT_HASH"
    "catalyrst-server.COMMS_FIXED_ADAPTER"
    "catalyrst-server.COMMS_PROTOCOL"
    "catalyrst-server.COMMS_STATS_URL"
    "catalyrst-server.COMMS_VERSION"
    "catalyrst-server.COMMS_WS_CONNECTOR_URL"
    "catalyrst-server.DEPLOYMENTS_QUERY_TIMEOUT_SECS"
    "catalyrst-server.MAX_DEPLOYMENT_SIZE_BYTES"
    "catalyrst-server.MAX_USERS"
    "catalyrst-server.NFT_WORKER_BASE_URL"
    "catalyrst-server.PROFILE_CDN_BASE_URL"
    "catalyrst-server.REQUEST_TIMEOUT_SECS"
    "catalyrst-server.SESSION_SECRET"
    "catalyrst-server.STORAGE_X_ACCEL_BASE"
    "catalyrst-server.SYNC_FLUSH_CONCURRENCY"
    "catalyrst-server.SYNC_NONSCENE_CONCURRENCY"
    "catalyrst-server.SYNC_RETRY_CONCURRENCY"
    "catalyrst-server.SYNC_SCENE_CONCURRENCY"
    "catalyrst-server.SYNC_SNAPSHOT_CONCURRENCY"
    "catalyrst-worlds.MAP_ESTATE_VIEW_URL"
    "catalyrst-worlds.MAP_PARCEL_VIEW_URL"
  ];
}
