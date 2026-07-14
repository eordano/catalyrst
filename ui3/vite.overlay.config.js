import { defineConfig } from "vite";
import { validateAlias } from "./vite.validate.js";
import react from "@vitejs/plugin-react";

export default defineConfig(({ command }) => ({
  base: "./",
  resolve: { alias: validateAlias() },
  plugins: [react()],
  define: {
    "process.env.NODE_ENV": JSON.stringify(
      command === "build" ? "production" : "development",
    ),
    "process.env": "{}",
  },
  server: {
    port: 5174,
    strictPort: true,
    cors: true,
    headers: { "Cross-Origin-Resource-Policy": "cross-origin" },
    hmr: { host: "localhost", port: 5174, protocol: "ws" },
  },
  build: {
    outDir: "dist-overlay",
    emptyOutDir: true,
    assetsInlineLimit: 0,
    cssCodeSplit: false,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      input: "src/overlay/overlay-main.tsx",
      output: {
        format: "es",
        entryFileNames: "overlay.js",
        chunkFileNames: "chunks/[name]-[hash].js",
        manualChunks(id) {
          if (/node_modules\/(react|react-dom|scheduler)\//.test(id)) {
            return "react-vendor";
          }
          return undefined;
        },
        assetFileNames: (info) =>
          info.names?.some((n) => n.endsWith(".css"))
            ? "overlay[extname]"
            : "assets/[name]-[hash][extname]",
      },
    },
  },
}));
