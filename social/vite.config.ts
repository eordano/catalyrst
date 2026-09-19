import { defineConfig } from "vite";
export default defineConfig({
  base: "./",
  server: { proxy: { "/api": "http://127.0.0.1:5191" } },
  build: { target: "es2022" },
});
