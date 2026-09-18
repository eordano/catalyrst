import { defineConfig } from "vite";
export default defineConfig({
  base: "./",
  resolve: { alias: { "@ui": new URL("../ui3/src", import.meta.url).pathname }, dedupe: ["react", "react-dom"] },
  server: { host: "127.0.0.1", port: 5192, strictPort: true, proxy: { "/api": "http://127.0.0.1:5191" } },
  build: { target: "es2022" },
});
