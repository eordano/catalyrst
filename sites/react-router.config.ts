import type { Config } from "@react-router/dev/config";

export default {
  ssr: true,
  appDirectory: "packages/routes/app",
  // Lazy discovery under /assets/: the foreign front's nginx proxies dynamic
  // /assets/* through to this app's own RR server, but not the unproxied
  // default /__manifest path -- manifestPath must stay under /assets/. Saves
  // the whole-app manifest-*.js (~267KB raw) preload+parse on every cold
  // load; the initial HTML inlines only the matched branch.
  routeDiscovery: { mode: "lazy", manifestPath: "/assets/__manifest" },
} satisfies Config;
