import { defineConfig } from "vitest/config";
// @ts-expect-error -- plain JS helper, shared with the vite and vitest configs so
// the perf-parity capture resolves the validation seam exactly as a build does.
import { validateAlias } from "./vite.validate.js";

// The capture half of the perf-parity gate. Run twice by
// scripts/check-perf-parity.mts, once with DCL_PERF=1, because vite.validate.js
// reads DCL_PERF at config time and one process therefore holds one mode.
//
// It reuses validateAlias() rather than restating the aliases: a config that
// wired the stubs its own way would be measuring a module graph no build ships.
//
// The capture file is `*.parity.ts`, not `*.test.ts`, so `npx vitest run` does
// not pick it up. It writes a file and asserts nothing on its own -- running it
// in one mode proves nothing, and a green tick next to the real suite would say
// otherwise.
export default defineConfig({
  resolve: { alias: validateAlias() },
  test: {
    environment: "jsdom",
    include: ["src/data/catalyst/perf-parity/capture.parity.ts"],
    execArgv: ["--no-experimental-webstorage"],
  },
});
