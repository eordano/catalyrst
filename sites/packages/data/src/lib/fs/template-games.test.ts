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
  it("covers every curated template (and no others)", () => {
    expect(Object.keys(REQUIRED_NAMES).sort()).toEqual(TEMPLATE_COMPOSITE_IDS.slice().sort());
    const registry = readFileSync(join(GAMES_DIR, "index.ts"), "utf8");
    for (const id of TEMPLATE_COMPOSITE_IDS) {
      expect(registry, `games/index.ts must register '${id}'`).toContain(`'${id}'`);
    }
  });

  for (const [template, names] of Object.entries(REQUIRED_NAMES)) {
    it(`${template}: composite, starter code and game bundle drive the SAME entity names`, () => {
      const authored = compositeNames(template);
      const starter = templateIndexTs(template) ?? "";
      const gameSrc = readFileSync(join(GAMES_DIR, `${template}.ts`), "utf8");
      for (const name of names) {
        expect(authored, `'${name}' must exist in the ${template} composite`).toContain(name);
        expect(starter, `starter index.ts must reference '${name}'`).toContain(name);
        expect(gameSrc, `game bundle source must reference '${name}'`).toContain(name);
      }
    });
  }

  it("the committed bundle artifact exists and carries the play-state contract", () => {
    expect(existsSync(BUNDLE), "run `npm run build` in catalyrst/sites/template-games").toBe(true);
    const js = readFileSync(BUNDLE, "utf8");
    expect(js).toContain("one-play-state");
    expect(js).toContain("one_play");
    for (const id of TEMPLATE_COMPOSITE_IDS) {
      expect(js, `bundle must contain the '${id}' game`).toContain(id);
    }
  });

  it("the committed bundle artifact carries the step-debugger telemetry contract", () => {
    const js = readFileSync(BUNDLE, "utf8");
    expect(js).toContain("one-dbg ");
    expect(js).toContain("march creeps");
  });
});
