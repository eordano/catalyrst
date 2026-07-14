import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const SITES = resolve(here, "../../../../../..");

describe("when checking the generated scene-analytics wire schema", () => {
  it("should match a fresh run of the generator (run npm run gen:scene-analytics after model changes)", () => {
    const fresh = execFileSync(
      process.execPath,
      ["scripts/gen-scene-analytics-zod.mts", "--stdout"],
      { cwd: SITES, encoding: "utf8" },
    );
    const committed = readFileSync(
      resolve(here, "scene-analytics.gen.ts"),
      "utf8",
    );
    expect(committed).toBe(fresh);
  });
});
