import { defineConfig } from "vitest/config";
// @ts-expect-error -- plain JS helper, shared with the vite and vitest configs so
import { validateAlias } from "./vite.validate.js";

export default defineConfig({
  resolve: { alias: validateAlias() },
  test: {
    environment: "jsdom",
    include: ["src/data/catalyst/perf-parity/capture.parity.ts"],
    execArgv: ["--no-experimental-webstorage"],
  },
});
