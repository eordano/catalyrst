import { execSync } from "node:child_process";
import { globSync, readFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import { playwright } from "@vitest/browser-playwright";
import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";

const nixChromium = () => {
  try {
    return execSync("ls -d /nix/store/*chromium*/bin/chromium 2>/dev/null | head -1", {
      encoding: "utf8",
    }).trim();
  } catch {
    return "";
  }
};

const CHROMIUM = process.env.CHROMIUM_BIN || nixChromium();

const UI3_ROOT = dirname(fileURLToPath(import.meta.url));
const storyFiles = globSync(
  [
    "src/**/*.stories.{js,jsx,ts,tsx}",
    "../sites/packages/routes/app/route-stories/**/*.stories.{ts,tsx}",
    "../sites/packages/features/src/stories/**/*.stories.{ts,tsx}",
  ],
  { cwd: UI3_ROOT, absolute: true },
).sort();

// Static stories remain Storybook documentation, but do not become tests merely
// because they render. The browser gate is reserved for explicit interaction
// contracts and the deliberately consolidated Catalog accessibility stories.
const contractStoryFiles = new Set(
  storyFiles.filter((path) => {
    const source = readFileSync(path, "utf8");
    return path.includes(".interactions.stories.")
      || /\bplay\s*:/.test(source)
      || /\bexport\s+(?:const|function|class)\s+Catalog\b/.test(source);
  }),
);

const documentationOnlyStoryFiles = storyFiles.filter((path) => !contractStoryFiles.has(path));

export default defineConfig({
  plugins: [
    storybookTest({ configDir: ".storybook", tags: { exclude: ["no-test"] } }),
  ],
  optimizeDeps: {
    exclude: [
      "monaco-editor/esm/vs/editor/editor.worker?worker&url",
      "monaco-editor/esm/vs/language/typescript/ts.worker?worker&url",
    ],
    include: [
      "monaco-editor",
      "@storybook/addon-a11y/preview",
      "@storybook/addon-links",
      "@storybook/react",
      "@testing-library/jest-dom/vitest",
      "@xstate/react",
      "gray-matter",
      "msw-storybook-addon/csf3",
      "storybook/test",
      "xstate",
      "three/examples/jsm/controls/OrbitControls.js",
      "three/examples/jsm/loaders/GLTFLoader.js",
    ],
  },
  test: {
    name: "storybook-browser",
    setupFiles: [".storybook/vitest.setup.ts"],
    exclude: documentationOnlyStoryFiles,
    coverage: {
      enabled: false,
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.stories.{ts,tsx}", "src/**/*.d.ts"],
      reportsDirectory: "coverage",
    },
    browser: {
      enabled: true,
      provider: playwright({
        launchOptions: {
          ...(CHROMIUM ? { executablePath: CHROMIUM } : {}),
          args: ["--no-sandbox"],
        },
      }),
      headless: true,
      instances: [{ browser: "chromium" }],
    },
  },
});
