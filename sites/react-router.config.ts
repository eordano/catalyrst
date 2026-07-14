import type { Config } from "@react-router/dev/config";

export default {
  ssr: true,
  appDirectory: "packages/routes/app",
  // Ship the whole route manifest up front so the client never depends on a
  // runtime /__manifest endpoint — foreign-domain fronts (interconnected.online)
  // do not proxy that internal route, and lazy discovery there throws
  // "Failed to fetch manifest patches" on every navigation.
  routeDiscovery: { mode: "initial" },
} satisfies Config;
