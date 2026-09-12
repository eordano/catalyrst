import type { Config } from "@react-router/dev/config";

export default {
  ssr: true,
  appDirectory: "packages/routes/app",
  routeDiscovery: { mode: "lazy", manifestPath: "/assets/__manifest" },
} satisfies Config;
