import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  esbuild: { jsx: "automatic" },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: [".storybook/vitest.setup.ts"],
    css: true,
    // Node ships its own Web Storage globals, and without --localstorage-file
    // their getters yield undefined. They still occupy the names on globalThis,
    // so jsdom never installs its working implementation over them and every
    // test sees `localStorage` as undefined. Drop Node's version in the workers.
    execArgv: ["--no-experimental-webstorage"],
  },
});
