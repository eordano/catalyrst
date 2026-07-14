import { defineConfig } from "vite";
import { validateAlias } from "./vite.validate.js";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "src/app",
  base: "/",
  publicDir: "../../public",
  resolve: { alias: validateAlias() },
  plugins: [react()],
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
    "process.env": "{}",
  },
  server: {
    fs: { strict: false },
  },
  build: {
    outDir: "../../dist-app",
    emptyOutDir: true,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("react-dom") || /\/react\/|\/react\/jsx/.test(id))
            return "vendor-react";
          if (id.includes("@tanstack")) return "vendor-query";
          if (id.includes("react-router")) return "vendor-router";
          if (id.includes("zod")) return "vendor-zod";
          return undefined;
        },
      },
    },
  },
});
