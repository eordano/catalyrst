export type RouteAuth = "both" | "out";

export type Route = {
  path: string;
  auth: RouteAuth;
  allowConsole?: RegExp[];
};

const AUTH_PROBE = /Failed to load resource.*40[13]/;
// Headless chromium exposes the WebGPU API without a real adapter, so the
// engine passes the browser-gate then panics at wgpu adapter selection.
// Real browsers with GPUs are unaffected; keep this ONLY on engine routes.
const HEADLESS_NO_GPU = [
  /Unable to find a GPU/,
  /panicked at .*bevy_render/,
  /RuntimeError: unreachable/,
];

export const ROUTES: Route[] = [
  { path: "/create", auth: "both" },
  { path: "/create/scenes", auth: "both" },
  { path: "/create/templates", auth: "out" },
  { path: "/create/wearables", auth: "out", allowConsole: [AUTH_PROBE] },
  { path: "/create/curate", auth: "out", allowConsole: [AUTH_PROBE] },
  { path: "/create/learn", auth: "out" },

  // Creator Hub: every route must render clean signed-out (gates, not
  // crashes) and signed-in (burner). scene-editor boots the engine iframe,
  // so give its console the auth-probe latitude only.
  { path: "/creator-hub/my-scenes", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/create-project", auth: "both" },
  { path: "/creator-hub/scene-editor?new=1", auth: "both", allowConsole: [AUTH_PROBE, ...HEADLESS_NO_GPU] },
  { path: "/creator-hub/deploy-world", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/manage", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/world-settings", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/world-permissions", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/worlds-storage", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/metrics", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/settings", auth: "both" },
  { path: "/creator-hub/map", auth: "out", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/claim-name", auth: "both", allowConsole: [AUTH_PROBE] },
  // Activity must render clean signed-out too: it is a scoping surface, not a
  // gate, and signed-out is the no-address state rather than a wall.
  { path: "/creator-hub/activity", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/activity/palkia.dcl.eth", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/creator-hub/data-sources", auth: "both", allowConsole: [AUTH_PROBE] },

  { path: "/marketplace/shop", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/marketplace/names", auth: "out", allowConsole: [AUTH_PROBE] },
  { path: "/marketplace/credits", auth: "out", allowConsole: [AUTH_PROBE] },
  { path: "/marketplace/lists", auth: "out", allowConsole: [AUTH_PROBE] },

  { path: "/", auth: "out" },
  { path: "/discover", auth: "out" },
  { path: "/governance", auth: "out" },
  { path: "/marketplace/account", auth: "both", allowConsole: [AUTH_PROBE] },
  { path: "/blog", auth: "out" },
  { path: "/docs", auth: "out" },
  { path: "/seasons", auth: "out" },
];
