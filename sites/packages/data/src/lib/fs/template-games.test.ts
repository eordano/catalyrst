import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import {
  TEMPLATE_COMPOSITE_IDS,
  buildTemplateComposite,
  templateIndexTs,
} from "./template-composites";
import { entityName, listEntities } from "../catalyst/creator-hub/scene-composite";

const GAMES_DIR = join(__dirname, "../../../../../template-games/src/games");
const BUNDLE = join(__dirname, "../../../../../../ui3/public/template-bundles/games.js");

const REQUIRED_NAMES: Record<string, string[]> = {
  "tower-defense": ["Creep Spider A", "Creep Spider B"],
  "nft-art-wall": ["Canvas 1", "Canvas 2", "Canvas 3"],
  "escape-room": ["Locked Door", "Escape Lever", "Brass Key"],
  "memory-game": ["Pad Red", "Pad Green", "Pad Blue", "Pad Yellow"],
  "castaway-2048": ["Tile 2", "Tile 4", "Tile 8", "Tile 16"],
};

function compositeNames(template: string): string[] {
  const comp = buildTemplateComposite(template)!;
  return listEntities(comp)
    .filter((id) => id >= 512)
    .map((id) => entityName(comp, id))
    .filter((n): n is string => typeof n === "string");
}

describe("template games \u{2014} name/registry/artifact sync", () => {
  it("covers every curated template (and no others), each registered in games/index.ts", () => {
    expect(Object.keys(REQUIRED_NAMES).sort()).toEqual(TEMPLATE_COMPOSITE_IDS.slice().sort());
    const registry = readFileSync(join(GAMES_DIR, "index.ts"), "utf8");
    expect(
      TEMPLATE_COMPOSITE_IDS.filter((id) => !registry.includes(`'${id}'`)),
    ).toEqual([]);
  });

  it("every template's composite, starter code and game bundle source drive the SAME entity names", () => {
    const offenders: string[] = [];
    for (const [template, names] of Object.entries(REQUIRED_NAMES)) {
      const authored = compositeNames(template);
      const starter = templateIndexTs(template) ?? "";
      const gameSrc = readFileSync(join(GAMES_DIR, `${template}.ts`), "utf8");
      for (const name of names) {
        if (!authored.includes(name)) offenders.push(`${template} composite lacks '${name}'`);
        if (!starter.includes(name)) offenders.push(`${template} starter index.ts lacks '${name}'`);
        if (!gameSrc.includes(name)) offenders.push(`${template} bundle source lacks '${name}'`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("the committed bundle artifact exists and carries every game, the play-state contract and the step-debugger telemetry contract", () => {
    expect(existsSync(BUNDLE), "run `npm run build` in catalyrst/sites/template-games").toBe(true);
    const js = readFileSync(BUNDLE, "utf8");
    expect(js).toContain("one-play-state");
    expect(js).toContain("one_play");
    expect(js).toContain("one-dbg ");
    expect(js).toContain("march creeps");
    expect(TEMPLATE_COMPOSITE_IDS.filter((id) => !js.includes(id))).toEqual([]);
  });

  it("boots @dcl/sdk before asset-packs so its provider wrapper is registered last", () => {
    const entry = readFileSync(join(GAMES_DIR, "../index.ts"), "utf8");
    expect(entry.trimStart().startsWith("import '@dcl/sdk'\n")).toBe(true);
    expect(entry).not.toContain("setCompositeProvider");
    expect(entry).toContain("initAssetPacks(engine)");
    const js = readFileSync(BUNDLE, "utf8");
    const sdkBoot = js.indexOf(".addTransport(");
    const packsBoot = js.indexOf("Ensure @dcl/sdk boots before initAssetPacks");
    expect(sdkBoot).toBeGreaterThan(-1);
    expect(packsBoot).toBeGreaterThan(sdkBoot);
    expect(js.indexOf(".addTransport(", sdkBoot + 1)).toBe(-1);
    expect(js).toContain("compositeProvider: failed to decode composite");
  });
});
