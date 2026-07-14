import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("../../ui3/public", import.meta.url));
const dst = fileURLToPath(new URL("../public", import.meta.url));

if (!existsSync(src)) {
  console.warn(`[sync:assets] ${src} not found \u{2014} skipping (ui3 public assets absent)`);
  process.exit(0);
}
mkdirSync(dst, { recursive: true });
for (const entry of readdirSync(dst)) {
  rmSync(join(dst, entry), { recursive: true, force: true });
}
cpSync(src, dst, { recursive: true });
console.log("[sync:assets] copied ui3/public -> sites/public");
