import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const p = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));

export const siteAliases: Record<string, string> = {
  "@ui": p("../src"),
  three: p("../node_modules/three"),
  "msw/browser": p("../node_modules/msw/lib/browser/index.mjs"),
  msw: p("../node_modules/msw/lib/core/index.mjs"),
  ...(existsSync(p("../../sites/packages/routes/app"))
    ? {
        // The sites boundary aliases (mirroring sites/vite.config.ts): the dev
        // server's /@fs/ importers outside the ui3 root miss tsconfig-paths
        // discovery, so sites stories 404 on @core/@data/@features without them.
        "@core": p("../../sites/packages/core/src"),
        "@data": p("../../sites/packages/data/src"),
        "@features": p("../../sites/packages/features/src"),
        "@routes": p("../../sites/packages/routes/app"),
        "monaco-editor": p("../../sites/node_modules/monaco-editor"),
        "node:fs": p("../../sites/packages/routes/app/route-stories/shims/node-fs.ts"),
        "node:path": p("../../sites/packages/routes/app/route-stories/shims/node-path.ts"),
      }
    : {}),
};

export const mswStaticBuildAliases: Record<string, string> = {
  "msw/browser": p("../node_modules/msw/lib/browser/index.js"),
  msw: p("../node_modules/msw/lib/core/index.js"),
};
