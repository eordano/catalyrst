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
          state("LOBBY", { sub: "“Welcome to Decentraland!”" }),
        ],
        branches: [
          {
            chain: "auto-jump",
            items: [
              auto("stored identity, ≥24h validity left"),
              node("jump", "loading", { href: "#boot", sub: "engine starts immediately" }),
            ],
          },
          {
            chain: "params",
            chips: true,
            note: "URL params the engine page honors — plus ?uidev=1, the sticky dev-overlay swap",
            items: [
              node("chip", "?realm="),
              node("chip", "?position=x,y"),
              node("chip", "?preview", { sub: "editor path — overlay does not mount" }),
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
            note: "all pre-engine — the choice is applied via SetAvatar once the engine reports identity",
            items: [
              node("chip", "name field", { sub: "random default" }),
              node("chip", "⟳ random name"),
              node("chip", "Masculine ⇄ Feminine"),
              node("chip", "Random", { sub: "random owned-catalog look" }),
              node("chip", "Terms checkbox", { sub: "gates JUMP IN" }),
            ],
          },
          {
            chain: "tos-nudge",
            items: [
              click("JUMP IN, terms unchecked"),
              outcome("nudge", { sub: "“Accept the terms above to jump in.”" }),
            ],
          },
          {
            chain: "pick",
            items: [
              click("JUMP IN"),
              state("PICKER", { sub: "“Where do you want to go?” · search · sort: Most active" }),
            ],
          },
        ],
      },
      {
        chain: "pick",
        items: [state("PICKER", { sub: "live places + worlds · occupancy · LIVE badges" })],
        branches: [
          {
            chain: "pick",
            items: [
              click("place / world card"),
              node("jump", "boot & loading", { href: "#boot", sub: "destination stored · engine starts" }),
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
          node("modal", "SIGNIN_MODAL", { sub: "“Sign in or sign up” · step machine: options" }),
        ],
        branches: [
          {
            chain: "social",
            items: [
              click("Continue with Google / Apple / Discord"),
              state("SHELL-WAIT", { busy: true }),
              load("full-page redirect · thirdweb hosted auth"),
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
              outcome("signed in", { sub: "SetIdentity → engine" }),
            ],
          },
          {
            chain: "qr",
            items: [
              click("Continue with wallet"),
              auto("no injected wallet"),
              load("“Starting phone sign-in…”"),
              state("QR", { sub: "LibreConnect — “no WalletConnect, no project ID”" }),
              auto("phone signs · session polled"),
              outcome("signed in"),
            ],
          },
          {
            chain: "pair",
            items: [
              route("/auth/pair/:session", {
                href: undefined,
                sub: "on the phone — the QR is a plain https link",
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
              outcome("“Signed in as 0x…”", { sub: "Sign out returns to guest, stays in the lobby" }),
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
          state("MOBILE GATE", { sub: "“not available on mobile” · App Store / Google Play" }),
        ],
        branches: [
          {
            chain: "browser-gate",
            items: [
              auto("non-Chrome desktop"),
              state("BROWSER GATE", { sub: "“Browser Not Supported” · Download Chrome" }),
              click("try anyway…"),
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
            sub: "“Booting engine” 0–50% · “Loading world” 50–100% · min 2.2s",
          }),
          auto("scene ready — or engine alive + 5s grace"),
          state("WORLD", { sub: "HUD mounts — engine draws, DOM chromes" }),
        ],
        branches: [
          {
            chain: "tips",
            items: [
              state("TIPS", { sub: "rotating carousel" }),
              { t: "edge", kind: "reversible", label: "‹ · ›" },
              outcome("manual browse"),
            ],
          },
          {
            chain: "stalled",
            items: [
              auto("20s, engine never came up"),
              state("STALLED", { sub: "“The world couldn’t start” · GPU hint" }),
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
          auto("panic · worker error · no frames ~16s"),
          node("modal", "CRASH OVERLAY", {
            sub: "“The world crashed” — cause-classified: WebGPU · disk · memory · hang",
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
              outcome("auto-dismiss", { sub: "watchdog self-heal — no click" }),
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
          node("modal", "FATAL ERROR", { sub: "“Something went wrong” · ErrorBoundary" }),
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
        items: [state("WORLD", { sub: "hash #/ — rail + minimap + chat + widgets" })],
        branches: [
          {
            chain: "rail-panels",
            chips: true,
            note: "rail top — full-screen panels, each a hash route; the same hint keys work from the world",
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
            note: "rail bottom — floating widgets over the world, one open at a time",
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
            note: "always-on: profile widget · notifications bell · minimap · connection dot · engine toasts",
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
          state("PANEL", { sub: "tabs: Events [X] · Places [Z] · Communities [O] · Map [M] · Backpack [I] · Gallery [K] · Settings [P]" }),
          click("Esc / ✕ / same key"),
          state("WORLD", { sub: "focus returns to the canvas" }),
        ],
      },
      {
        chain: "chat",
        items: [
          state("WORLD"),
          click("Enter / chat icon"),
          state("CHAT DOCK", { sub: "Nearby channel · @mentions · emoji + :shortcodes:" }),
          click("send"),
          outcome("SendChat → engine"),
        ],
        branches: [
          {
            chain: "chat-links",
            items: [
              click("coords / world link in a message"),
              node("jump", "jump-in", { href: "#go", sub: "Teleport · ChangeRealm" }),
            ],
          },
        ],
      },
      {
        chain: "minimap",
        items: [
          state("MINIMAP", { sub: "scene name + parcel — live position stream" }),
          click("expand"),
          state("MAP PANEL", { sub: "#/map" }),
        ],
        branches: [
          {
            chain: "minimap-menu",
            chips: true,
            note: "⋮ menu",
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
      "The wearables editor equips live through the engine; outfits save as slots. The profile widget is the in-world door to the same sign-in machine as the lobby — and to sign-out, which keeps you in-world as a guest.",
    tracks: [
      {
        chain: "backpack",
        items: [
          state("BACKPACK", { sub: "#/backpack · tabs: Wearables · Emotes · Outfits" }),
          click("category tile"),
          state("CATEGORY GRID", { sub: "owned catalog · paginated" }),
          click("item"),
          outcome("equipped live", { sub: "SetAvatar → engine preview" }),
        ],
        branches: [
          {
            chain: "recolor",
            items: [click("color swatches"), outcome("recolor", { sub: "skin · hair · eyes" })],
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
              outcome("profile persisted", { sub: "signed-in only — guest looks are session-local" }),
            ],
          },
        ],
      },
      {
        chain: "profile",
        items: [
          state("WORLD"),
          click("avatar chip"),
          state("PROFILE CARD", { sub: "name · wallet · copy address" }),
        ],
        branches: [
          {
            chain: "profile-passport",
            items: [click("VIEW PROFILE"), state("PASSPORT", { sub: "#/passport · badges · photos · equipped" })],
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
              outcome("guest session", { sub: "Logout → engine · you stay in-world" }),
            ],
          },
        ],
      },
      {
        chain: "logincode",
        note: "engine-gated — appears only when the engine pushes a login code (desktop-style external auth); rare on web",
        items: [
          auto("engine pushes login code"),
          node("modal", "LOGIN CODE", { sub: "code + “open on another device”" }),
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
          state("EVENTS", { sub: "#/events · day carousel · LIVE badges · featured rail" }),
          click("jump in — land event"),
          load("Teleport + scene prewarm"),
          state("JUMP LOADING", { sub: "“Teleporting to <name>…”" }),
          auto("3.5s timer"),
          state("WORLD", { sub: "minimap follows the position stream" }),
        ],
        branches: [
          {
            chain: "world-confirm",
            items: [
              click("jump in — world event"),
              node("modal", "VISIT WORLD?", { sub: "“Do you want to jump to the following realm?”" }),
              click("CONTINUE"),
              load("ChangeRealm"),
              state("JUMP LOADING", { sub: "“Teleporting to <name>…”" }),
            ],
          },
          {
            chain: "world-cancel",
            items: [auto("from VISIT WORLD?"), click("CANCEL / ✕"), node("end", "")],
          },
        ],
      },
      {
        chain: "places",
        items: [
          state("PLACES", { sub: "#/places · search · sort · likes" }),
          click("place card"),
          state("PLACE DETAIL"),
          click("JUMP IN"),
          node("jump", "same chain", { href: "#go", sub: "parcel → Teleport · world → confirm" }),
        ],
      },
      {
        chain: "mapjump",
        items: [
          state("MAP", { sub: "#/map · pan/zoom atlas · place sidebar" }),
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
            sub: "POST /entities/active · batches of 100, ×2 to a 1000 cap · farthest-first · engine-internal",
          }),
          auto("resolved"),
          outcome("parcel → scene hash | Nothing"),
        ],
        branches: [
          {
            chain: "pointer-fail",
            items: [
              auto("fetch fails"),
              state("BACKOFF", { sub: "batch halves · 0.5s·2ⁿ up to 32s · engine-internal" }),
              auto("10 consecutive failures"),
              outcome("error toast", { sub: "AppError → engine toast — the only surface" }),
            ],
          },
        ],
      },
      {
        chain: "boot-scene",
        items: [
          state("SPAWNED", { sub: "“spawning scene” — parcels within 50m of you" }),
          auto("entity definition fetched"),
          state("SCENE ENTITY", { sub: "scene.json + content map · scene-pack prefetch" }),
          auto("main.crdt, if any"),
          state("MAIN CRDT"),
          auto("js module ready"),
          state("JAVASCRIPT", {
            sub: "“started scene” · per-scene worker · SDK6 scenes get the adaption layer",
          }),
          auto("tick 5"),
          outcome("scene appears", {
            sub: "parked at y −1000 behind glowing boundary walls until now",
          }),
        ],
        branches: [
          {
            chain: "boot-fail",
            items: [
              auto("any step fails"),
              state("FAILED", { sub: "terminal — no retry until despawn + respawn" }),
            ],
          },
          {
            chain: "defer",
            items: [
              auto("you stand in a booting scene"),
              outcome("neighbors deferred", {
                sub: "≤15s, then spawned anyway — “to avoid an empty (green-ground) world”",
              }),
            ],
          },
          {
            chain: "asset-fetch",
            note: "per-asset fetches: 30s header / 10s stall timeouts, ≤3 retries, failed URLs muted 10s — engine-internal; misses show as missing meshes",
            items: [],
          },
        ],
      },
      {
        chain: "tick",
        items: [
          state("RUNNING", { sub: "CRDT tick loop ⇄ renderer" }),
          auto("no reply for 10s while in flight"),
          state("BROKEN", {
            sub: "“has not responded for 10s, marking broken” — freezes as-is, updates discarded",
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
                sub: "the “not responding… timeout in Ns” stream exists engine-side; the web overlay doesn’t render it",
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
          auto("scene ready · or FAILED · or 60s cap"),
          outcome("dropped into the world"),
        ],
      },
      {
        chain: "unload",
        items: [
          auto("scene beyond the load radius"),
          outcome("despawned", { sub: "scene pack released · see the hysteresis note in the footer" }),
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
            sub: "baked tiles · mip rings at 100 · 200 · 400 · 800 · 1600m · same-origin /bvimposters",
          }),
        ],
        branches: [
          {
            chain: "imposter-load",
            items: [
              auto("tile requested"),
              state("PENDING", {
                busy: true,
                sub: "≤6 downloads in flight on web · a coarser parent tile stands in",
              }),
              auto("zip fetched"),
              outcome("rendered"),
            ],
          },
          {
            chain: "imposter-miss",
            items: [
              auto("no bake exists"),
              outcome("nothing — sky and fog", {
                sub: "no placeholder · re-polled every frame · failed assets refetched ×2, then given up",
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
      "Friends and communities run on the node’s social services; voice runs through the engine. The floating rail widgets and the full-screen panels share the same data.",
    tracks: [
      {
        chain: "friends",
        items: [
          state("FRIENDS", { sub: "rail widget or full panel · friends · requests · blocked" }),
          click("request / accept / cancel / reject / delete / block / unblock"),
          outcome("SignRequest upsert_friendship", { sub: "engine signs → social service" }),
        ],
      },
      {
        chain: "communities",
        items: [
          state("COMMUNITIES", { sub: "#/communities · browse · search" }),
          click("join / leave"),
          outcome("membership updated"),
        ],
        branches: [
          {
            chain: "community-detail",
            items: [
              click("community card"),
              state("COMMUNITY", { sub: "members · stream" }),
            ],
          },
        ],
      },
      {
        chain: "voice",
        items: [
          state("VOICE", { sub: "rail widget" }),
          { t: "edge", kind: "reversible", label: "mic on / off" },
          outcome("SetMic → engine"),
        ],
      },
      {
        chain: "notifications",
        items: [
          state("WORLD"),
          click("bell"),
          state("NOTIFICATIONS", { sub: "unread badge · floating panel" }),
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
          state("CAMERA", { sub: "rail widget — SetCameraMode detached" }),
          click("shutter / Space"),
          load("CapturePhoto — engine renders"),
          outcome("saved to reel", { sub: "signed-in only" }),
        ],
      },
      {
        chain: "gallery",
        items: [
          state("GALLERY", { sub: "#/gallery — your photos" }),
          click("photo"),
          state("PHOTO DETAIL", { sub: "visible people · jump to location" }),
        ],
      },
      {
        chain: "settings",
        items: [
          state("SETTINGS", { sub: "#/settings · pill sections" }),
          click("toggle / pick"),
          outcome("SetSetting → engine, live", {
            sub: "fps cap · antialiasing · shadows · fog · scene load distance · voice volume · chat privacy",
          }),
        ],
      },
      {
        chain: "skybox",
        items: [
          state("SKYBOX", { sub: "rail widget" }),
          click("time of day"),
          outcome("SetTimeOfDay → engine"),
        ],
      },
      {
        chain: "permission",
        items: [
          auto("scene requests a capability"),
          node("modal", "PERMISSION", { sub: "scene name + capability" }),
          click("Allow / Deny"),
          outcome("ResolvePermission → engine"),
        ],
      },
      {
        chain: "connection",
        items: [
          state("WORLD"),
          click("status dot"),
          state("CONNECTION", { sub: "realm · rooms · scene health" }),
        ],
      },
    ],
  },

  {
    id: "meta",
    num: "09",
    title: "Exits & Meta",
    blurb:
      "There is no in-product door back to the lobby from a healthy world — exits are sign-out (stay in-world as guest), the stalled card, or the browser itself. And the map maps itself.",
    tracks: [
      {
        chain: "exit",
        items: [
          state("WORLD"),
          click("browser reload / close"),
          outcome("cold boot", { sub: "identity persists — auto-jump inside 24h, lobby after" }),
        ],
      },
      {
        chain: "map-meta",
        items: [
          route("/explorer-map", { sub: "this page" }),
          sep(),
          route("/creator-hub/map", { sub: "sibling — the Creator Hub, mapped the same way" }),
          sep(),
          PLAY({ sub: "the product" }),
        ],
      },
    ],
  },
];

export const STATS = computeStats(SECTIONS);

export const ASCII_SOURCE = `# Explorer (dcl.one/play) — click/state sitemap

Every edge is one user CLICK, or a LOAD⏳ when the transition invokes work >100ms
(engine boot, signed deploys, teleports). States in CAPS are component/machine
states; \`/paths\` are URLs. Surveyed against the live overlay + engine page +
engine crates.

\`\`\`
/play/  (nginx: engine page + ui3 DOM overlay · BootGate)
├── gates: mobile ▶ MOBILE GATE (App Store/Google Play) · non-Chrome ▶ BROWSER
│          GATE ("Browser Not Supported") ─[try anyway…]▶ bypass cookie + reload
├── auto: stored identity ≥24h left ──▶ LOADING (skips lobby + picker)
├── LOBBY  "Welcome to Decentraland!"   (LobbyNew — engine avatar preview)
│    ├─ name field · ⟳ random name · Masculine⇄Feminine · Random look ·
│    │  Terms checkbox (unchecked ▶ "Accept the terms above to jump in.")
│    ├─[JUMP IN]▶ PICKER "Where do you want to go?" (live places+worlds,
│    │   search, sort Most active) ─[card]/[Skip to Genesis Plaza]▶ LOADING
│    └─[Sign in]▶ SIGNIN_MODAL "Sign in or sign up"  (SignInModalView)
│         ├─ Google/Apple/Discord ▶ SHELL-WAIT ⏳ redirect ▶ back w/ ?authResult=
│         ├─ wallet: injected ▶ WALLET ─[Continue w/ <wallet>]⏳ sign ▶ signed in
│         ├─ no wallet ▶ ⏳ "Starting phone sign-in…" ▶ QR (LibreConnect — "no
│         │   WalletConnect, no project ID") · phone: /auth/pair/:session
│         │   ─[approve]⏳ sign ▶ desktop completes
│         ├─ email ▶ CODE ─[verify]⏳ OTP ▶ signed in
│         └─ "Signed in as 0x…" ─[Sign out]▶ guest, stays in lobby
├── LOADING  "Booting engine" 0–50% ▶ "Loading world" 50–100% · min 2.2s ·
│    tips ‹⇄› · 20s cap ▶ STALLED "The world couldn't start"
│    ─[Try again]▶ reload · ─[Back to lobby]▶ LOBBY
├── crash (native page): panic/worker error/hang ~16s ▶ CRASH OVERLAY "The
│    world crashed" (cause-classified: WebGPU · disk · memory · hang)
│    ─[Reload] · ─[Dismiss] · frames resume ~4s ▶ auto-dismiss (self-heal)
│    stop-64 teleport wedges: prevented engine-side (drop guard, no UI)
│    overlay render error ▶ FATAL ERROR "Something went wrong" ─[Reload]
│
├── WORLD  (hash #/ — AppLayout: rail + minimap + chat + widgets)
│    ├─ rail panels (hash routes + hint keys): Backpack[I] · Places[Z] ·
│    │  Communities[O] · Camera Reel[K] · Settings[P] · Map[M] · Events[X]
│    │  (+ Marketplace↗ · Help↗) — Esc/✕/same key ▶ back to WORLD
│    ├─ rail widgets: Voice · Portables · Skybox · Camera · Emotes[B] ·
│    │  Friends · Chat[Enter]  (one at a time)
│    ├─ CHAT: Nearby channel · @mentions · emoji ─[send]▶ SendChat
│    │   message coords/world links ─[click]▶ jump-in chain
│    ├─ MINIMAP: live parcel readout ─[expand]▶ MAP · ⋮ Jump to coordinates ·
│    │   Copy coordinates · Copy Link
│    └─ EMOTE WHEEL [B]: 10 slots ─[slot]▶ PlayEmote
│
├── BACKPACK  #/backpack · Wearables/Emotes/Outfits · category ▶ grid ▶
│    [item]▶ equipped live (SetAvatar) · recolor · SAVE OUTFIT ·
│    ⏳ signed profile deploy (signed-in; guest looks are session-local)
├── PROFILE: [avatar chip]▶ card ─[VIEW PROFILE]▶ PASSPORT · ─[Sign in]▶
│    SIGNIN_MODAL (same machine) · ─[Sign out]▶ guest, stays in-world
│    LOGIN CODE modal: engine-pushed external auth (engine-gated, rare on web)
│
├── JUMP-IN (Events #/events · Places #/places · Map #/map · chat links · reel)
│    land parcel ─[jump in]⏳ Teleport + scene prewarm ▶ JUMP LOADING
│    "Teleporting to <name>…" ──3.5s timer──▶ WORLD
│    world realm ─[jump in]▶ VISIT WORLD? "Do you want to jump to the following
│    realm?" ─[CONTINUE]⏳ ChangeRealm ▶ JUMP LOADING · ─[CANCEL]▶ stay
│
├── SCENE LIFECYCLE (engine — what runs under every teleport)
│    pointers: player moves ▶ POST /entities/active (batch 100 ×2 → cap 1000,
│    farthest-first) ▶ parcel → hash | Nothing · fail ▶ backoff 0.5s·2ⁿ ≤32s,
│    batch halves · 10 consecutive fails ▶ AppError → engine toast
│    per scene: SPAWNED "spawning scene" ▶ SCENE ENTITY (scene.json + content
│    map) ▶ MAIN CRDT ▶ JAVASCRIPT "started scene" (per-scene worker; SDK6 ▶
│    adaption layer) ─tick 5─▶ visible (parked at y −1000 behind glowing
│    loading walls until then) · any step fails ▶ FAILED (terminal) ·
│    current-scene boot defers neighbors ≤15s ("green-ground" guard)
│    ticking: RUNNING ⇄ CRDT · no reply 10s ▶ BROKEN "marking broken" —
│    freezes as-is; js exception ▶ BROKEN + scene-console error · web shows
│    NO banner (the not-responding countdown stream is engine-side only)
│    teleport: OUT OF WORLD until scene ready | FAILED | 60s cap ▶ dropped in
│    unload: beyond 50m ▶ despawn (see hysteresis note) · ChangeRealm ▶ purge
│    imposters: beyond load radius ▶ baked tiles, mip rings 100/200/400/800/
│    1600m from same-origin /bvimposters (≤6 dl on web; coarser parent stands
│    in; missing ▶ sky+fog, no placeholder; failed assets refetched ×2)
│
├── SOCIAL: FRIENDS (requests/search) · COMMUNITIES (join/leave/detail) ·
│    VOICE mic⇄ (SetMic) · NOTIFICATIONS bell
├── SYSTEM: CAMERA [shutter/Space]⏳ CapturePhoto (signed-in) ▶ reel ·
│    GALLERY ▶ PHOTO DETAIL ▶ jump to location · SETTINGS ▶ SetSetting live ·
│    SKYBOX ▶ SetTimeOfDay · PERMISSION [Allow/Deny]▶ ResolvePermission ·
│    CONNECTION dot ▶ realm/rooms/scene health
│
└── exits: no lobby door from a healthy world — sign-out stays in-world;
     reload/close ▶ cold boot (auto-jump inside 24h) · /explorer-map — this map
     · sibling /creator-hub/map
\`\`\`

Legend: \`[x]\` = click edge · \`⏳\` = >100ms invoked load · \`⇄\` = reversible
pair · \`↗\` = external tab. /explorer/map 308s here where it isn't shadowed by
the catalyst explorer API. Deliberately excluded:
the /bevy-overlay/* routes (the same views served standalone for the native
client + dev harness), the engine page's own dev launcher form, and ?preview
editor mode — none are part of the /play web surface.
`;
