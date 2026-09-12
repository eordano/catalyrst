import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
// @ts-expect-error -- plain JS helper, shared with the two vite configs so the
import { validateAlias } from "./vite.validate.js";

export default defineConfig({
  resolve: { alias: validateAlias() },
  plugins: [react()],
  esbuild: { jsx: "automatic" },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: [".storybook/vitest.setup.ts"],
    css: true,
    execArgv: ["--no-experimental-webstorage"],
  },
});
