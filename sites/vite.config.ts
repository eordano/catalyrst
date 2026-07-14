import { reactRouter } from "@react-router/dev/vite";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
// Plain JS helper shared with ui3, so both trees resolve the validation seam
// identically and a perf build means the same thing in each.
import { validateAliasObject } from "../ui3/vite.validate.js";

const ui = fileURLToPath(new URL("../ui3/src", import.meta.url));
const pkg = (name: string) => fileURLToPath(new URL(`./packages/${name}`, import.meta.url));

function watchUi3(ui3Src: string): Plugin {
  return {
    name: "watch-ui3",
    apply: "serve",
    configureServer(server) {
      server.watcher.add(ui3Src);
      server.watcher.on("change", (file) => {
        if (!file.startsWith(ui3Src)) return;
        const mods = server.moduleGraph.getModulesByFile(file);
        if (!mods || mods.size === 0) return;
        for (const mod of mods) void server.reloadModule(mod).catch(() => {});
      });
    },
  };
}
const monaco = fileURLToPath(new URL("./node_modules/monaco-editor", import.meta.url));
const three = fileURLToPath(new URL("./node_modules/three", import.meta.url));
const qrcode = fileURLToPath(new URL("./node_modules/qrcode-generator", import.meta.url));
const reactQuery = fileURLToPath(new URL("./node_modules/@tanstack/react-query", import.meta.url));

const CATALYST_PROXY_TARGET =
  process.env.DCL_CATALYST_PROXY_TARGET || "https://catalyst.example.com";

export default defineConfig({
  plugins: [reactRouter(), watchUi3(ui)],
  server: {
    allowedHosts: [".catalyst.example.com", "localhost", "127.0.0.1"],
    hmr: { protocol: "wss", clientPort: 443 },
    strictPort: true,
    // ui3 sits outside this project root, so its own assets (the wearable-preview
    // emote GLBs, served straight off disk via /@fs in dev) 403 under Vite's
    // default fs.allow without this.
    fs: { allow: [".", "../ui3"] },
    // Dev-only: the vite origin has no /lambdas or /content of its own, so
    // proxy them to a catalyst. The target is env-overridable per deployment
    // (portability gate: no baked foreign domain) and defaults to the public one.
    proxy: {
      "/lambdas": { target: CATALYST_PROXY_TARGET, changeOrigin: true },
      "/content": { target: CATALYST_PROXY_TARGET, changeOrigin: true },
      "/auth-api": { target: CATALYST_PROXY_TARGET, changeOrigin: true },
    },
  },
  resolve: {
    alias: {
      ...validateAliasObject(),
      "@ui": ui,
      "@core": pkg("core/src"),
      "@data": pkg("data/src"),
      "@features": pkg("features/src"),
      "@routes": pkg("routes/app"),
      "monaco-editor": monaco,
      three,
      "qrcode-generator": qrcode,
      "@tanstack/react-query": reactQuery,
    },
    dedupe: ["viem", "react", "react-dom", "zod"],
  },
  optimizeDeps: {
    include: ["monaco-editor", "@xstate/react", "xstate"],
    exclude: [
      "monaco-editor/esm/vs/editor/editor.worker?worker&url",
      "monaco-editor/esm/vs/language/typescript/ts.worker?worker&url",
    ],
  },
  test: {
    // The e2e specs belong to vitest.e2e.config.ts, which is the only config that
    // loads test/e2e/globalSetup.ts -- the thing that provisions their Postgres.
    // Vitest's default discovery swept them in here too, where that setup never
    // runs, so `npm test` failed five armed suites every time on any machine. That
    // is worse than not running them: a suite that is always red is a suite nobody
    // reads. Run them with `npm run test:e2e`, which CI does via
    // scripts/no-silent-skips.sh.
    exclude: ["**/node_modules/**", "**/dist/**", "test/e2e/**"],
  },
});
