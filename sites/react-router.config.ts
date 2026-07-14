import type { Config } from "@react-router/dev/config";

export default {
  ssr: true,
  appDirectory: "packages/routes/app",
  // Lazy discovery under /assets/ — probed 2026-08-17: GET
  // https://interconnected.online/assets/__manifest?p=/governance returned a 404
  // rendered by this app's own RR server (full modulepreload HTML through the
  // front's nginx), proving the foreign front proxies dynamic /assets/* to the
  // node server. The old "Failed to fetch manifest patches" regression was about
  // the unproxied default /__manifest path, which manifestPath avoids. Saves the
  // whole-app manifest-*.js (~267KB raw) preload+parse on every cold load; the
  // initial HTML inlines only the matched branch.
  routeDiscovery: { mode: "lazy", manifestPath: "/assets/__manifest" },
} satisfies Config;
