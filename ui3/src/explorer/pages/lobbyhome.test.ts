import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

test("lobby uses the full-screen panel layer above the engine canvas", () => {
  const css = readFileSync(join(__dirname, "lobbyhome.css"), "utf8");
  const lobby = css.match(/\.lh\s*\{([^}]*)\}/)?.[1] ?? "";
  expect(lobby).toMatch(/position:\s*fixed\s*;/);
  expect(lobby).toMatch(/z-index:\s*var\(--z-panel\)\s*;/);
  expect(lobby).toMatch(/inset:\s*0\s*;/);
  expect(lobby).toMatch(/pointer-events:\s*auto\s*;/);
});
