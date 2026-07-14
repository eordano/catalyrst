import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const ui = fileURLToPath(new URL("../ui3/src", import.meta.url));
const cssStub = fileURLToPath(new URL("./test/e2e/css-stub.ts", import.meta.url));
const pkg = (name: string) => fileURLToPath(new URL(`./packages/${name}`, import.meta.url));

export default defineConfig({
  resolve: {
    alias: [
      { find: /^.*\.css$/, replacement: cssStub },
      { find: "@ui", replacement: ui },
      { find: "@core", replacement: pkg("core/src") },
      { find: "@data", replacement: pkg("data/src") },
      { find: "@features", replacement: pkg("features/src") },
      { find: "@routes", replacement: pkg("routes/app") },
    ],
    dedupe: ["react", "react-dom", "react-router"],
  },
  test: {
    include: ["test/e2e/**/*.e2e.test.ts"],
    globalSetup: ["./test/e2e/globalSetup.ts"],
    environment: "node",
    testTimeout: 30_000,
    hookTimeout: 90_000,
    fileParallelism: false,
    pool: "forks",
  },
});
