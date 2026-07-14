import {
  auto,
  click,
  computeStats,
  load,
  node,
  outcome,
  route,
  sep,
  state,
} from "../../flowmap/flowmapdata";
import type { FlowSection } from "../../flowmap/flowmapdata";

export const MACHINE_PATHS: Record<string, string> = {
  BootGate: "catalyrst/ui3/src/app/BootGate.tsx",
  LobbyNew: "catalyrst/ui3/src/explorer/workflows/LobbyNew.tsx",
  SignInModalView: "catalyrst/ui3/src/components/SignInModalView.tsx",
  PlacesPicker: "catalyrst/ui3/src/explorer/workflows/PlacesPicker.tsx",
  Loading: "catalyrst/ui3/src/explorer/workflows/Loading.tsx",
  "crash-overlay": "bevy-explorer/web/src/inline/crash-overlay.ts",
  AppLayout: "catalyrst/ui3/src/app/AppLayout.tsx",
  Sidebar: "catalyrst/ui3/src/explorer/frames/Sidebar.tsx",
  Chat: "catalyrst/ui3/src/explorer/frames/Chat.tsx",
  Minimap: "catalyrst/ui3/src/explorer/frames/Minimap.tsx",
  Backpack: "catalyrst/ui3/src/explorer/pages/Backpack.tsx",
  ProfileWidget: "catalyrst/ui3/src/explorer/components/ProfileWidget.tsx",
  "Events.route": "catalyrst/ui3/src/app/panels/Events.route.tsx",
  "Places.route": "catalyrst/ui3/src/app/panels/Places.route.tsx",
  "Map.route": "catalyrst/ui3/src/app/panels/Map.route.tsx",
  WorldVisitModal: "catalyrst/ui3/src/components/WorldVisitModal.tsx",
  Friends: "catalyrst/ui3/src/explorer/pages/Friends.tsx",
  Communities: "catalyrst/ui3/src/explorer/pages/Communities.tsx",
  Passport: "catalyrst/ui3/src/explorer/pages/Passport.tsx",
  Camera: "catalyrst/ui3/src/explorer/pages/Camera.tsx",
  Settings: "catalyrst/ui3/src/explorer/pages/Settings.tsx",
  PermissionPrompt: "catalyrst/ui3/src/explorer/components/PermissionPrompt.tsx",
  bridge: "catalyrst/ui3/src/overlay/bridge.ts",
  "lifecycle.rs": "bevy-explorer/crates/scene_runner/src/initialize_scene/lifecycle.rs",
  "load.rs": "bevy-explorer/crates/scene_runner/src/initialize_scene/load.rs",
  "scene_loop.rs": "bevy-explorer/crates/scene_runner/src/scene_loop.rs",
  "loading_quads.rs": "bevy-explorer/crates/scene_runner/src/initialize_scene/loading_quads.rs",
  imposters: "bevy-explorer/crates/imposters/src/render",
};

const PLAY = (o: Parameters<typeof route>[1] = {}) =>
  route("/play/", { plain: true, ...o });

export const SECTIONS: FlowSection[] = [
  {
    id: "entry",
    num: "01",
    title: "Entry & Lobby",
    machines: ["BootGate", "LobbyNew", "SignInModalView", "PlacesPicker"],
    blurb:
      "One nginx-served page hosts the wasm engine and the DOM overlay. BootGate decides: fresh visitors get the guest lobby; a stored identity still valid for 24h skips straight to loading. Sign-in is an escape hatch, not a gate.",
    tracks: [
      {
        chain: "arrive",
        items: [
          PLAY({ sub: "engine page + ui3 overlay" }),
          auto("fresh session"),
          state("LOBBY", { sub: "\u{201C}Welcome to Decentraland!\u{201D}" }),
        ],
        branches: [
          {
            chain: "auto-jump",
            items: [
              auto("stored identity, \u{2265}24h validity left"),
              node("jump", "loading", { href: "#boot", sub: "engine starts immediately" }),
            ],
          },
          {
            chain: "params",
            chips: true,
            note: "URL params the engine page honors \u{2014} plus ?uidev=1, the sticky dev-overlay swap",
            items: [
              node("chip", "?realm="),
              node("chip", "?position=x,y"),
              node("chip", "?preview", { sub: "editor path \u{2014} overlay does not mount" }),
            ],
          },
        ],
      },
      {
        chain: "avatar",
        items: [state("LOBBY", { sub: "live engine avatar preview at right" })],
        branches: [
          {
            chain: "avatar",
            chips: true,
            note: "all pre-engine \u{2014} the choice is applied via SetAvatar once the engine reports identity",
            items: [
              node("chip", "name field", { sub: "random default" }),
              node("chip", "\u{27F3} random name"),
              node("chip", "Masculine \u{21C4} Feminine"),
              node("chip", "Random", { sub: "random owned-catalog look" }),
              node("chip", "Terms checkbox", { sub: "gates JUMP IN" }),
            ],
          },
          {
            chain: "tos-nudge",
            items: [
              click("JUMP IN, terms unchecked"),
              outcome("nudge", { sub: "\u{201C}Accept the terms above to jump in.\u{201D}" }),
            ],
          },
          {
            chain: "pick",
            items: [
              click("JUMP IN"),
              state("PICKER", { sub: "\u{201C}Where do you want to go?\u{201D} \u{B7} search \u{B7} sort: Most active" }),
            ],
          },
        ],
      },
      {
        chain: "pick",
        items: [state("PICKER", { sub: "live places + worlds \u{B7} occupancy \u{B7} LIVE badges" })],
        branches: [
          {
            chain: "pick",
            items: [
              click("place / world card"),
              node("jump", "boot & loading", { href: "#boot", sub: "destination stored \u{B7} engine starts" }),
            ],
          },
          {
            chain: "skip",
            items: [
              click("Skip to Genesis Plaza"),
              node("jump", "boot & loading", { href: "#boot", sub: "default spawn" }),
            ],
          },
        ],
      },
      {
        chain: "signin",
        items: [
          state("LOBBY"),
          click("Sign in"),
          node("modal", "SIGNIN_MODAL", { sub: "\u{201C}Sign in or sign up\u{201D} \u{B7} step machine: options" }),
        ],
        branches: [
          {
            chain: "social",
            items: [
              click("Continue with Google / Apple / Discord"),
              state("SHELL-WAIT", { busy: true }),
              load("full-page redirect \u{B7} thirdweb hosted auth"),
              outcome("returns with ?authResult=", { sub: "sign-in completes on the way back in" }),
            ],
          },
          {
            chain: "wallet",
            items: [
              click("Continue with wallet"),
              auto("injected wallet found"),
              state("WALLET", { sub: "one button per detected wallet" }),
              load("request accounts + personal_sign", { label: "Continue with <wallet>" }),
              outcome("signed in", { sub: "SetIdentity \u{2192} engine" }),
            ],
          },
          {
            chain: "qr",
            items: [
              click("Continue with wallet"),
              auto("no injected wallet"),
              load("\u{201C}Starting phone sign-in\u{2026}\u{201D}"),
              state("QR", { sub: "LibreConnect \u{2014} \u{201C}no WalletConnect, no project ID\u{201D}" }),
              auto("phone signs \u{B7} session polled"),
              outcome("signed in"),
            ],
          },
          {
            chain: "pair",
            items: [
              route("/auth/pair/:session", {
                href: undefined,
                sub: "on the phone \u{2014} the QR is a plain https link",
              }),
              click("approve in the wallet"),
              load("personal_sign on the phone"),
              outcome("desktop session completes"),
            ],
          },
          {
            chain: "email",
            items: [
              click("Continue"),
              state("CODE", { sub: "e-mail one-time code" }),
              load("thirdweb OTP verify", { label: "code entry" }),
              outcome("signed in"),
            ],
          },
          {
            chain: "signed",
            items: [
              auto("signed in"),
              outcome("\u{201C}Signed in as 0x\u{2026}\u{201D}", { sub: "Sign out returns to guest, stays in the lobby" }),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "boot",
    num: "02",
    title: "Boot, Loading & Recovery",
    machines: ["BootGate", "Loading", "crash-overlay"],
    blurb:
      "The engine downloads and compiles on page load but waits for a destination (dclDeferStart). The overlay owns the loading screen; the native page owns the browser gates and the crash overlay. Every recovery path here is real.",
    tracks: [
      {
        chain: "gates",
        items: [
          PLAY(),
          auto("mobile browser"),
          state("MOBILE GATE", { sub: "\u{201C}not available on mobile\u{201D} \u{B7} App Store / Google Play" }),
        ],
        branches: [
          {
            chain: "browser-gate",
            items: [
              auto("non-Chrome desktop"),
              state("BROWSER GATE", { sub: "\u{201C}Browser Not Supported\u{201D} \u{B7} Download Chrome" }),
              click("try anyway\u{2026}"),
              outcome("bypass cookie + reload", { sub: "30-day cookie" }),
            ],
          },
        ],
      },
      {
        chain: "loading",
        items: [
          click("destination picked"),
          state("LOADING", {
            busy: true,
            sub: "\u{201C}Booting engine\u{201D} 0\u{2013}50% \u{B7} \u{201C}Loading world\u{201D} 50\u{2013}100% \u{B7} min 2.2s",
          }),
          auto("scene ready \u{2014} or engine alive + 5s grace"),
          state("WORLD", { sub: "HUD mounts \u{2014} engine draws, DOM chromes" }),
        ],
        branches: [
          {
            chain: "tips",
            items: [
              state("TIPS", { sub: "rotating carousel" }),
              { t: "edge", kind: "reversible", label: "\u{2039} \u{B7} \u{203A}" },
              outcome("manual browse"),
            ],
          },
          {
            chain: "stalled",
            items: [
              auto("20s, engine never came up"),
              state("STALLED", { sub: "\u{201C}The world couldn\u{2019}t start\u{201D} \u{B7} GPU hint" }),
              click("Try again"),
              outcome("page reload"),
            ],
          },
          {
            chain: "stalled-lobby",
            items: [auto("from STALLED"), click("Back to lobby"), node("jump", "lobby", { href: "#entry" })],
          },
        ],
      },
      {
        chain: "crash",
        items: [
          state("WORLD"),
          auto("panic \u{B7} worker error \u{B7} no frames ~16s"),
          node("modal", "CRASH OVERLAY", {
            sub: "\u{201C}The world crashed\u{201D} \u{2014} cause-classified: WebGPU \u{B7} disk \u{B7} memory \u{B7} hang",
          }),
        ],
        branches: [
          { chain: "crash", items: [click("Reload"), outcome("page reload")] },
          {
            chain: "crash-dismiss",
            items: [click("Dismiss"), outcome("overlay hidden", { sub: "engine may still be down" })],
          },
          {
            chain: "self-heal",
            items: [
              auto("frames resume ~4s"),
              outcome("auto-dismiss", { sub: "watchdog self-heal \u{2014} no click" }),
            ],
          },
          {
            chain: "stop64",
            note: "stop-64-class teleport wedges are prevented engine-side (web-task cross-thread drop guard); the watchdog card is the fallback if one slips through",
            items: [auto("teleport into a heavy scene"), outcome("no wedge")],
          },
        ],
      },
      {
        chain: "react-fatal",
        items: [
          auto("overlay render error"),
          node("modal", "FATAL ERROR", { sub: "\u{201C}Something went wrong\u{201D} \u{B7} ErrorBoundary" }),
          click("Reload"),
          outcome("page reload"),
        ],
      },
    ],
  },

  {
    id: "hud",
    num: "03",
    title: "HUD Chrome & Chat",
    machines: ["AppLayout", "Sidebar", "Chat", "Minimap"],
    blurb:
      "Everything in-world is DOM over the canvas: a left rail, minimap, profile widget, chat dock, floating widgets, toasts. Full-screen panels are hash routes inside the overlay; Esc always walks back to the world.",
    tracks: [
      {
        chain: "world",
        items: [state("WORLD", { sub: "hash #/ \u{2014} rail + minimap + chat + widgets" })],
        branches: [
          {
            chain: "rail-panels",
            chips: true,
            note: "rail top \u{2014} full-screen panels, each a hash route; the same hint keys work from the world",
            items: [
              node("chip", "Backpack [I]"),
              node("chip", "Places [Z]"),
              node("chip", "Communities [O]"),
              node("chip", "Camera Reel [K]"),
              node("chip", "Settings [P]"),
              node("external", "Marketplace"),
              node("external", "Help & Support"),
            ],
          },
          {
            chain: "rail-widgets",
            chips: true,
            note: "rail bottom \u{2014} floating widgets over the world, one open at a time",
            items: [
              node("chip", "Voice Chat"),
              node("chip", "Portable Experiences"),
              node("chip", "Skybox"),
              node("chip", "Camera"),
              node("chip", "Emotes [B]"),
              node("chip", "Friends"),
              node("chip", "Chat [Enter]"),
            ],
          },
          {
            chain: "always-on",
            chips: true,
            note: "always-on: profile widget \u{B7} notifications bell \u{B7} minimap \u{B7} connection dot \u{B7} engine toasts",
            items: [
              node("chip", "Profile"),
              node("chip", "Notifications"),
              node("chip", "Minimap"),
              node("chip", "Connection"),
              node("chip", "Toasts"),
            ],
          },
        ],
      },
      {
        chain: "panel-loop",
        items: [
          state("WORLD"),
          click("rail icon / hint key"),
          state("PANEL", { sub: "tabs: Events [X] \u{B7} Places [Z] \u{B7} Communities [O] \u{B7} Map [M] \u{B7} Backpack [I] \u{B7} Gallery [K] \u{B7} Settings [P]" }),
          click("Esc / \u{2715} / same key"),
          state("WORLD", { sub: "focus returns to the canvas" }),
        ],
      },
      {
        chain: "chat",
        items: [
          state("WORLD"),
          click("Enter / chat icon"),
          state("CHAT DOCK", { sub: "Nearby channel \u{B7} @mentions \u{B7} emoji + :shortcodes:" }),
          click("send"),
          outcome("SendChat \u{2192} engine"),
        ],
        branches: [
          {
            chain: "chat-links",
            items: [
              click("coords / world link in a message"),
              node("jump", "jump-in", { href: "#go", sub: "Teleport \u{B7} ChangeRealm" }),
            ],
          },
        ],
      },
      {
        chain: "minimap",
        items: [
          state("MINIMAP", { sub: "scene name + parcel \u{2014} live position stream" }),
          click("expand"),
          state("MAP PANEL", { sub: "#/map" }),
        ],
        branches: [
          {
            chain: "minimap-menu",
            chips: true,
            note: "\u{22EE} menu",
            items: [
              node("chip", "Jump to coordinates", { sub: "Teleport" }),
              node("chip", "Copy coordinates"),
              node("chip", "Copy Link"),
            ],
          },
        ],
      },
      {
        chain: "emote",
        items: [
          state("WORLD"),
          click("B / rail Emotes"),
          state("EMOTE WHEEL", { sub: "10 slots from your loadout" }),
          click("slot"),
          outcome("PlayEmote", { sub: "wheel closes" }),
        ],
      },
    ],
  },

  {
    id: "avatar",
    num: "04",
    title: "Backpack, Profile & Identity",
    machines: ["Backpack", "ProfileWidget"],
    blurb:
      "The wearables editor equips live through the engine; outfits save as slots. The profile widget is the in-world door to the same sign-in machine as the lobby \u{2014} and to sign-out, which keeps you in-world as a guest.",
    tracks: [
      {
        chain: "backpack",
        items: [
          state("BACKPACK", { sub: "#/backpack \u{B7} tabs: Wearables \u{B7} Emotes \u{B7} Outfits" }),
          click("category tile"),
          state("CATEGORY GRID", { sub: "owned catalog \u{B7} paginated" }),
          click("item"),
          outcome("equipped live", { sub: "SetAvatar \u{2192} engine preview" }),
        ],
        branches: [
          {
            chain: "recolor",
            items: [click("color swatches"), outcome("recolor", { sub: "skin \u{B7} hair \u{B7} eyes" })],
          },
          {
            chain: "bp-emotes",
            items: [
              click("Emotes tab"),
              state("EMOTE SLOTS", { sub: "equip the 10 wheel slots" }),
            ],
          },
          {
            chain: "outfits",
            items: [
              click("Outfits tab"),
              state("OUTFITS"),
              click("SAVE OUTFIT"),
              outcome("slot saved"),
            ],
          },
          {
            chain: "bp-save",
            items: [
              load("signed profile deploy", { label: "save" }),
              outcome("profile persisted", { sub: "signed-in only \u{2014} guest looks are session-local" }),
            ],
          },
        ],
      },
      {
        chain: "profile",
        items: [
          state("WORLD"),
          click("avatar chip"),
          state("PROFILE CARD", { sub: "name \u{B7} wallet \u{B7} copy address" }),
        ],
        branches: [
          {
            chain: "profile-passport",
            items: [click("VIEW PROFILE"), state("PASSPORT", { sub: "#/passport \u{B7} badges \u{B7} photos \u{B7} equipped" })],
          },
          {
            chain: "inworld-signin",
            items: [
              click("Sign in"),
              node("modal", "SIGNIN_MODAL", { sub: "same machine as the lobby" }),
              node("jump", "sign-in flow", { href: "#entry" }),
            ],
          },
          {
            chain: "signout",
            items: [
              click("Sign out"),
              outcome("guest session", { sub: "Logout \u{2192} engine \u{B7} you stay in-world" }),
            ],
          },
        ],
      },
      {
        chain: "logincode",
        note: "engine-gated \u{2014} appears only when the engine pushes a login code (desktop-style external auth); rare on web",
        items: [
          auto("engine pushes login code"),
          node("modal", "LOGIN CODE", { sub: "code + \u{201C}open on another device\u{201D}" }),
          click("open link"),
          outcome("external auth completes"),
        ],
      },
    ],
  },

  {
    id: "go",
    num: "05",
    title: "Explore & Jump-in",
    machines: ["Events.route", "Places.route", "Map.route", "WorldVisitModal"],
    blurb:
      "Events, Places and the Map feed one jump-in chain: land parcels teleport immediately (scene prewarmed over the wire), world realms confirm first. The overlay names the destination the whole way.",
    tracks: [
      {
        chain: "events",
        items: [
          state("EVENTS", { sub: "#/events \u{B7} day carousel \u{B7} LIVE badges \u{B7} featured rail" }),
          click("jump in \u{2014} land event"),
          load("Teleport + scene prewarm"),
          state("JUMP LOADING", { sub: "\u{201C}Teleporting to <name>\u{2026}\u{201D}" }),
          auto("3.5s timer"),
          state("WORLD", { sub: "minimap follows the position stream" }),
        ],
        branches: [
          {
            chain: "world-confirm",
            items: [
              click("jump in \u{2014} world event"),
              node("modal", "VISIT WORLD?", { sub: "\u{201C}Do you want to jump to the following realm?\u{201D}" }),
              click("CONTINUE"),
              load("ChangeRealm"),
              state("JUMP LOADING", { sub: "\u{201C}Teleporting to <name>\u{2026}\u{201D}" }),
            ],
          },
          {
            chain: "world-cancel",
            items: [auto("from VISIT WORLD?"), click("CANCEL / \u{2715}"), node("end", "")],
          },
        ],
      },
      {
        chain: "places",
        items: [
          state("PLACES", { sub: "#/places \u{B7} search \u{B7} sort \u{B7} likes" }),
          click("place card"),
          state("PLACE DETAIL"),
          click("JUMP IN"),
          node("jump", "same chain", { href: "#go", sub: "parcel \u{2192} Teleport \u{B7} world \u{2192} confirm" }),
        ],
      },
      {
        chain: "mapjump",
        items: [
          state("MAP", { sub: "#/map \u{B7} pan/zoom atlas \u{B7} place sidebar" }),
          click("parcel / place"),
          node("jump", "same chain", { href: "#go" }),
        ],
      },
      {
        chain: "reel-jump",
        items: [
          state("PHOTO DETAIL", { sub: "#/gallery" }),
          click("jump to photo location"),
          load("Teleport"),
          state("JUMP LOADING"),
        ],
      },
    ],
  },

  {
    id: "scene",
    num: "06",
    title: "Scene Lifecycle",
    machines: ["lifecycle.rs", "load.rs", "scene_loop.rs", "loading_quads.rs", "imposters"],
    blurb:
      "What the engine does under every teleport: parcels resolve to scene entities, each scene walks a five-state boot, runs a tick loop with a 10s watchdog, and beyond the load radius the world is baked imposters. What you actually see: glowing loading walls, the pop-in at tick 5, a scene that silently freezes when it breaks, and the imposter skyline.",
    tracks: [
      {
        chain: "pointers",
        items: [
          auto("player moves / realm set"),
          state("POINTER FETCH", {
            busy: true,
            sub: "POST /entities/active \u{B7} batches of 100, \u{D7}2 to a 1000 cap \u{B7} farthest-first \u{B7} engine-internal",
          }),
          auto("resolved"),
          outcome("parcel \u{2192} scene hash | Nothing"),
        ],
        branches: [
          {
            chain: "pointer-fail",
            items: [
              auto("fetch fails"),
              state("BACKOFF", { sub: "batch halves \u{B7} 0.5s\u{B7}2\u{207F} up to 32s \u{B7} engine-internal" }),
              auto("10 consecutive failures"),
              outcome("error toast", { sub: "AppError \u{2192} engine toast \u{2014} the only surface" }),
            ],
          },
        ],
      },
      {
        chain: "boot-scene",
        items: [
          state("SPAWNED", { sub: "\u{201C}spawning scene\u{201D} \u{2014} parcels within 50m of you" }),
          auto("entity definition fetched"),
          state("SCENE ENTITY", { sub: "scene.json + content map \u{B7} scene-pack prefetch" }),
          auto("main.crdt, if any"),
          state("MAIN CRDT"),
          auto("js module ready"),
          state("JAVASCRIPT", {
            sub: "\u{201C}started scene\u{201D} \u{B7} per-scene worker \u{B7} SDK6 scenes get the adaption layer",
          }),
          auto("tick 5"),
          outcome("scene appears", {
            sub: "parked at y \u{2212}1000 behind glowing boundary walls until now",
          }),
        ],
        branches: [
          {
            chain: "boot-fail",
            items: [
              auto("any step fails"),
              state("FAILED", { sub: "terminal \u{2014} no retry until despawn + respawn" }),
            ],
          },
          {
            chain: "defer",
            items: [
              auto("you stand in a booting scene"),
              outcome("neighbors deferred", {
                sub: "\u{2264}15s, then spawned anyway \u{2014} \u{201C}to avoid an empty (green-ground) world\u{201D}",
              }),
            ],
          },
          {
            chain: "asset-fetch",
            note: "per-asset fetches: 30s header / 10s stall timeouts, \u{2264}3 retries, failed URLs muted 10s \u{2014} engine-internal; misses show as missing meshes",
            items: [],
          },
        ],
      },
      {
        chain: "tick",
        items: [
          state("RUNNING", { sub: "CRDT tick loop \u{21C4} renderer" }),
          auto("no reply for 10s while in flight"),
          state("BROKEN", {
            sub: "\u{201C}has not responded for 10s, marking broken\u{201D} \u{2014} freezes as-is, updates discarded",
          }),
        ],
        branches: [
          {
            chain: "js-error",
            items: [
              auto("js exception"),
              state("BROKEN", { sub: "error logged to the scene console" }),
            ],
          },
          {
            chain: "broken-surface",
            items: [
              auto("what you see"),
              outcome("the scene simply freezes", {
                sub: "the \u{201C}not responding\u{2026} timeout in Ns\u{201D} stream exists engine-side; the web overlay doesn\u{2019}t render it",
              }),
            ],
          },
        ],
      },
      {
        chain: "arrive-wait",
        items: [
          auto("teleport / realm change"),
          state("OUT OF WORLD", { sub: "player held while the destination boots" }),
          auto("scene ready \u{B7} or FAILED \u{B7} or 60s cap"),
          outcome("dropped into the world"),
        ],
      },
      {
        chain: "unload",
        items: [
          auto("scene beyond the load radius"),
          outcome("despawned", { sub: "scene pack released \u{B7} see the hysteresis note in the footer" }),
        ],
        branches: [
          {
            chain: "realm-purge",
            items: [
              auto("ChangeRealm"),
              outcome("full purge", { sub: "all non-portable scenes + pointers + imposters" }),
            ],
          },
        ],
      },
      {
        chain: "imposter",
        items: [
          auto("beyond the load radius"),
          state("IMPOSTER", {
            sub: "baked tiles \u{B7} mip rings at 100 \u{B7} 200 \u{B7} 400 \u{B7} 800 \u{B7} 1600m \u{B7} same-origin /bvimposters",
          }),
        ],
        branches: [
          {
            chain: "imposter-load",
            items: [
              auto("tile requested"),
              state("PENDING", {
                busy: true,
                sub: "\u{2264}6 downloads in flight on web \u{B7} a coarser parent tile stands in",
              }),
              auto("zip fetched"),
              outcome("rendered"),
            ],
          },
          {
            chain: "imposter-miss",
            items: [
              auto("no bake exists"),
              outcome("nothing \u{2014} sky and fog", {
                sub: "no placeholder \u{B7} re-polled every frame \u{B7} failed assets refetched \u{D7}2, then given up",
              }),
            ],
          },
          {
            chain: "imposter-live",
            items: [
              auto("a live scene loads in"),
              outcome("imposter suppressed", { sub: "returns if the scene unloads again" }),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "social",
    num: "07",
    title: "Social & Comms",
    machines: ["Friends", "Communities", "Passport"],
    blurb:
      "Friends and communities run on the node\u{2019}s social services; voice runs through the engine. The floating rail widgets and the full-screen panels share the same data.",
    tracks: [
      {
        chain: "friends",
        items: [
          state("FRIENDS", { sub: "rail widget or full panel \u{B7} friends \u{B7} requests \u{B7} blocked" }),
          click("request / accept / cancel / reject / delete / block / unblock"),
          outcome("SignRequest upsert_friendship", { sub: "engine signs \u{2192} social service" }),
        ],
      },
      {
        chain: "communities",
        items: [
          state("COMMUNITIES", { sub: "#/communities \u{B7} browse \u{B7} search" }),
          click("join / leave"),
          outcome("membership updated"),
        ],
        branches: [
          {
            chain: "community-detail",
            items: [
              click("community card"),
              state("COMMUNITY", { sub: "members \u{B7} stream" }),
            ],
          },
        ],
      },
      {
        chain: "voice",
        items: [
          state("VOICE", { sub: "rail widget" }),
          { t: "edge", kind: "reversible", label: "mic on / off" },
          outcome("SetMic \u{2192} engine"),
        ],
      },
      {
        chain: "notifications",
        items: [
          state("WORLD"),
          click("bell"),
          state("NOTIFICATIONS", { sub: "unread badge \u{B7} floating panel" }),
        ],
      },
    ],
  },

  {
    id: "system",
    num: "08",
    title: "Camera, Settings & Permissions",
    machines: ["Camera", "Settings", "PermissionPrompt"],
    blurb:
      "The camera detaches through the engine and captures real renders; settings write straight to the engine and apply live; scenes ask for capabilities through one prompt.",
    tracks: [
      {
        chain: "camera",
        items: [
          state("CAMERA", { sub: "rail widget \u{2014} SetCameraMode detached" }),
          click("shutter / Space"),
          load("CapturePhoto \u{2014} engine renders"),
          outcome("saved to reel", { sub: "signed-in only" }),
        ],
      },
      {
        chain: "gallery",
        items: [
          state("GALLERY", { sub: "#/gallery \u{2014} your photos" }),
          click("photo"),
          state("PHOTO DETAIL", { sub: "visible people \u{B7} jump to location" }),
        ],
      },
      {
        chain: "settings",
        items: [
          state("SETTINGS", { sub: "#/settings \u{B7} pill sections" }),
          click("toggle / pick"),
          outcome("SetSetting \u{2192} engine, live", {
            sub: "fps cap \u{B7} antialiasing \u{B7} shadows \u{B7} fog \u{B7} scene load distance \u{B7} voice volume \u{B7} chat privacy",
          }),
        ],
      },
      {
        chain: "skybox",
        items: [
          state("SKYBOX", { sub: "rail widget" }),
          click("time of day"),
          outcome("SetTimeOfDay \u{2192} engine"),
        ],
      },
      {
        chain: "permission",
        items: [
          auto("scene requests a capability"),
          node("modal", "PERMISSION", { sub: "scene name + capability" }),
          click("Allow / Deny"),
          outcome("ResolvePermission \u{2192} engine"),
        ],
      },
      {
        chain: "connection",
        items: [
          state("WORLD"),
          click("status dot"),
          state("CONNECTION", { sub: "realm \u{B7} rooms \u{B7} scene health" }),
        ],
      },
    ],
  },

  {
    id: "meta",
    num: "09",
    title: "Exits & Meta",
    blurb:
      "There is no in-product door back to the lobby from a healthy world \u{2014} exits are sign-out (stay in-world as guest), the stalled card, or the browser itself. And the map maps itself.",
    tracks: [
      {
        chain: "exit",
        items: [
          state("WORLD"),
          click("browser reload / close"),
          outcome("cold boot", { sub: "identity persists \u{2014} auto-jump inside 24h, lobby after" }),
        ],
      },
      {
        chain: "map-meta",
        items: [
          route("/explorer-map", { sub: "this page" }),
          sep(),
          route("/creator-hub/map", { sub: "sibling \u{2014} the Creator Hub, mapped the same way" }),
          sep(),
          PLAY({ sub: "the product" }),
        ],
      },
    ],
  },
];

export const STATS = computeStats(SECTIONS);

export const ASCII_SOURCE = `# Explorer (catalyst.example.com/play) \u{2014} click/state sitemap

Every edge is one user CLICK, or a LOAD\u{23F3} when the transition invokes work >100ms
(engine boot, signed deploys, teleports). States in CAPS are component/machine
states; \`/paths\` are URLs. Surveyed against the live overlay + engine page +
engine crates.

\`\`\`
/play/  (nginx: engine page + ui3 DOM overlay \u{B7} BootGate)
\u{251C}\u{2500}\u{2500} gates: mobile \u{25B6} MOBILE GATE (App Store/Google Play) \u{B7} non-Chrome \u{25B6} BROWSER
\u{2502}          GATE ("Browser Not Supported") \u{2500}[try anyway\u{2026}]\u{25B6} bypass cookie + reload
\u{251C}\u{2500}\u{2500} auto: stored identity \u{2265}24h left \u{2500}\u{2500}\u{25B6} LOADING (skips lobby + picker)
\u{251C}\u{2500}\u{2500} LOBBY  "Welcome to Decentraland!"   (LobbyNew \u{2014} engine avatar preview)
\u{2502}    \u{251C}\u{2500} name field \u{B7} \u{27F3} random name \u{B7} Masculine\u{21C4}Feminine \u{B7} Random look \u{B7}
\u{2502}    \u{2502}  Terms checkbox (unchecked \u{25B6} "Accept the terms above to jump in.")
\u{2502}    \u{251C}\u{2500}[JUMP IN]\u{25B6} PICKER "Where do you want to go?" (live places+worlds,
\u{2502}    \u{2502}   search, sort Most active) \u{2500}[card]/[Skip to Genesis Plaza]\u{25B6} LOADING
\u{2502}    \u{2514}\u{2500}[Sign in]\u{25B6} SIGNIN_MODAL "Sign in or sign up"  (SignInModalView)
\u{2502}         \u{251C}\u{2500} Google/Apple/Discord \u{25B6} SHELL-WAIT \u{23F3} redirect \u{25B6} back w/ ?authResult=
\u{2502}         \u{251C}\u{2500} wallet: injected \u{25B6} WALLET \u{2500}[Continue w/ <wallet>]\u{23F3} sign \u{25B6} signed in
\u{2502}         \u{251C}\u{2500} no wallet \u{25B6} \u{23F3} "Starting phone sign-in\u{2026}" \u{25B6} QR (LibreConnect \u{2014} "no
\u{2502}         \u{2502}   WalletConnect, no project ID") \u{B7} phone: /auth/pair/:session
\u{2502}         \u{2502}   \u{2500}[approve]\u{23F3} sign \u{25B6} desktop completes
\u{2502}         \u{251C}\u{2500} email \u{25B6} CODE \u{2500}[verify]\u{23F3} OTP \u{25B6} signed in
\u{2502}         \u{2514}\u{2500} "Signed in as 0x\u{2026}" \u{2500}[Sign out]\u{25B6} guest, stays in lobby
\u{251C}\u{2500}\u{2500} LOADING  "Booting engine" 0\u{2013}50% \u{25B6} "Loading world" 50\u{2013}100% \u{B7} min 2.2s \u{B7}
\u{2502}    tips \u{2039}\u{21C4}\u{203A} \u{B7} 20s cap \u{25B6} STALLED "The world couldn't start"
\u{2502}    \u{2500}[Try again]\u{25B6} reload \u{B7} \u{2500}[Back to lobby]\u{25B6} LOBBY
\u{251C}\u{2500}\u{2500} crash (native page): panic/worker error/hang ~16s \u{25B6} CRASH OVERLAY "The
\u{2502}    world crashed" (cause-classified: WebGPU \u{B7} disk \u{B7} memory \u{B7} hang)
\u{2502}    \u{2500}[Reload] \u{B7} \u{2500}[Dismiss] \u{B7} frames resume ~4s \u{25B6} auto-dismiss (self-heal)
\u{2502}    stop-64 teleport wedges: prevented engine-side (drop guard, no UI)
\u{2502}    overlay render error \u{25B6} FATAL ERROR "Something went wrong" \u{2500}[Reload]
\u{2502}
\u{251C}\u{2500}\u{2500} WORLD  (hash #/ \u{2014} AppLayout: rail + minimap + chat + widgets)
\u{2502}    \u{251C}\u{2500} rail panels (hash routes + hint keys): Backpack[I] \u{B7} Places[Z] \u{B7}
\u{2502}    \u{2502}  Communities[O] \u{B7} Camera Reel[K] \u{B7} Settings[P] \u{B7} Map[M] \u{B7} Events[X]
\u{2502}    \u{2502}  (+ Marketplace\u{2197} \u{B7} Help\u{2197}) \u{2014} Esc/\u{2715}/same key \u{25B6} back to WORLD
\u{2502}    \u{251C}\u{2500} rail widgets: Voice \u{B7} Portables \u{B7} Skybox \u{B7} Camera \u{B7} Emotes[B] \u{B7}
\u{2502}    \u{2502}  Friends \u{B7} Chat[Enter]  (one at a time)
\u{2502}    \u{251C}\u{2500} CHAT: Nearby channel \u{B7} @mentions \u{B7} emoji \u{2500}[send]\u{25B6} SendChat
\u{2502}    \u{2502}   message coords/world links \u{2500}[click]\u{25B6} jump-in chain
\u{2502}    \u{251C}\u{2500} MINIMAP: live parcel readout \u{2500}[expand]\u{25B6} MAP \u{B7} \u{22EE} Jump to coordinates \u{B7}
\u{2502}    \u{2502}   Copy coordinates \u{B7} Copy Link
\u{2502}    \u{2514}\u{2500} EMOTE WHEEL [B]: 10 slots \u{2500}[slot]\u{25B6} PlayEmote
\u{2502}
\u{251C}\u{2500}\u{2500} BACKPACK  #/backpack \u{B7} Wearables/Emotes/Outfits \u{B7} category \u{25B6} grid \u{25B6}
\u{2502}    [item]\u{25B6} equipped live (SetAvatar) \u{B7} recolor \u{B7} SAVE OUTFIT \u{B7}
\u{2502}    \u{23F3} signed profile deploy (signed-in; guest looks are session-local)
\u{251C}\u{2500}\u{2500} PROFILE: [avatar chip]\u{25B6} card \u{2500}[VIEW PROFILE]\u{25B6} PASSPORT \u{B7} \u{2500}[Sign in]\u{25B6}
\u{2502}    SIGNIN_MODAL (same machine) \u{B7} \u{2500}[Sign out]\u{25B6} guest, stays in-world
\u{2502}    LOGIN CODE modal: engine-pushed external auth (engine-gated, rare on web)
\u{2502}
\u{251C}\u{2500}\u{2500} JUMP-IN (Events #/events \u{B7} Places #/places \u{B7} Map #/map \u{B7} chat links \u{B7} reel)
\u{2502}    land parcel \u{2500}[jump in]\u{23F3} Teleport + scene prewarm \u{25B6} JUMP LOADING
\u{2502}    "Teleporting to <name>\u{2026}" \u{2500}\u{2500}3.5s timer\u{2500}\u{2500}\u{25B6} WORLD
\u{2502}    world realm \u{2500}[jump in]\u{25B6} VISIT WORLD? "Do you want to jump to the following
\u{2502}    realm?" \u{2500}[CONTINUE]\u{23F3} ChangeRealm \u{25B6} JUMP LOADING \u{B7} \u{2500}[CANCEL]\u{25B6} stay
\u{2502}
\u{251C}\u{2500}\u{2500} SCENE LIFECYCLE (engine \u{2014} what runs under every teleport)
\u{2502}    pointers: player moves \u{25B6} POST /entities/active (batch 100 \u{D7}2 \u{2192} cap 1000,
\u{2502}    farthest-first) \u{25B6} parcel \u{2192} hash | Nothing \u{B7} fail \u{25B6} backoff 0.5s\u{B7}2\u{207F} \u{2264}32s,
\u{2502}    batch halves \u{B7} 10 consecutive fails \u{25B6} AppError \u{2192} engine toast
\u{2502}    per scene: SPAWNED "spawning scene" \u{25B6} SCENE ENTITY (scene.json + content
\u{2502}    map) \u{25B6} MAIN CRDT \u{25B6} JAVASCRIPT "started scene" (per-scene worker; SDK6 \u{25B6}
\u{2502}    adaption layer) \u{2500}tick 5\u{2500}\u{25B6} visible (parked at y \u{2212}1000 behind glowing
\u{2502}    loading walls until then) \u{B7} any step fails \u{25B6} FAILED (terminal) \u{B7}
\u{2502}    current-scene boot defers neighbors \u{2264}15s ("green-ground" guard)
\u{2502}    ticking: RUNNING \u{21C4} CRDT \u{B7} no reply 10s \u{25B6} BROKEN "marking broken" \u{2014}
\u{2502}    freezes as-is; js exception \u{25B6} BROKEN + scene-console error \u{B7} web shows
\u{2502}    NO banner (the not-responding countdown stream is engine-side only)
\u{2502}    teleport: OUT OF WORLD until scene ready | FAILED | 60s cap \u{25B6} dropped in
\u{2502}    unload: beyond 50m \u{25B6} despawn (see hysteresis note) \u{B7} ChangeRealm \u{25B6} purge
\u{2502}    imposters: beyond load radius \u{25B6} baked tiles, mip rings 100/200/400/800/
\u{2502}    1600m from same-origin /bvimposters (\u{2264}6 dl on web; coarser parent stands
\u{2502}    in; missing \u{25B6} sky+fog, no placeholder; failed assets refetched \u{D7}2)
\u{2502}
\u{251C}\u{2500}\u{2500} SOCIAL: FRIENDS (requests/search) \u{B7} COMMUNITIES (join/leave/detail) \u{B7}
\u{2502}    VOICE mic\u{21C4} (SetMic) \u{B7} NOTIFICATIONS bell
\u{251C}\u{2500}\u{2500} SYSTEM: CAMERA [shutter/Space]\u{23F3} CapturePhoto (signed-in) \u{25B6} reel \u{B7}
\u{2502}    GALLERY \u{25B6} PHOTO DETAIL \u{25B6} jump to location \u{B7} SETTINGS \u{25B6} SetSetting live \u{B7}
\u{2502}    SKYBOX \u{25B6} SetTimeOfDay \u{B7} PERMISSION [Allow/Deny]\u{25B6} ResolvePermission \u{B7}
\u{2502}    CONNECTION dot \u{25B6} realm/rooms/scene health
\u{2502}
\u{2514}\u{2500}\u{2500} exits: no lobby door from a healthy world \u{2014} sign-out stays in-world;
     reload/close \u{25B6} cold boot (auto-jump inside 24h) \u{B7} /explorer-map \u{2014} this map
     \u{B7} sibling /creator-hub/map
\`\`\`

Legend: \`[x]\` = click edge \u{B7} \`\u{23F3}\` = >100ms invoked load \u{B7} \`\u{21C4}\` = reversible
pair \u{B7} \`\u{2197}\` = external tab. /explorer/map 308s here where it isn't shadowed by
the catalyst explorer API. Deliberately excluded:
the /bevy-overlay/* routes (the same views served standalone for the native
client + dev harness), the engine page's own dev launcher form, and ?preview
editor mode \u{2014} none are part of the /play web surface.
`;
