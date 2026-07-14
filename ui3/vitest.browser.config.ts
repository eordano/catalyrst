import { execSync } from "node:child_process";
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

export default defineConfig({
  plugins: [
    storybookTest({ configDir: ".storybook", tags: { exclude: ["no-test"] } }),
  ],
  optimizeDeps: {
    // pre-bundle everything the suite imports: a cold cache re-optimizing
    // mid-run reloads the tester page and aborts in-flight tests
    include: [
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
