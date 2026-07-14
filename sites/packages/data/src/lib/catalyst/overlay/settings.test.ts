import { describe, expect, it } from "vitest";

import {
  DEFAULT_TAB,
  defaultValues,
  groupsForTab,
  loadSettingsCatalog,
  parseSettingsFixture,
  parseTab,
  type SettingModule,
} from "./settings";

function allModules(): SettingModule[] {
  return Object.values(loadSettingsCatalog().sections).flatMap((groups) =>
    groups.flatMap((g) => g.modules),
  );
}

function fixtureWith(module: Record<string, unknown>) {
  return {
    tabs: [{ id: "graphics", label: "Graphics" }],
    sections: { graphics: [{ title: "Group", modules: [module] }] },
  };
}

describe("loadSettingsCatalog", () => {
  it("parses the shipped catalog through the zod refinements", () => {
    const catalog = loadSettingsCatalog();
    expect(catalog.tabs.length).toBeGreaterThan(0);
    for (const m of allModules()) {
      if (m.fullscreen) continue;
      expect(m.setting, `module ${m.key} names an engine setting`).toBeTruthy();
      expect(m.default, `module ${m.key} carries a seed default`).toBeDefined();
    }
  });

  it("seeds Marker Visibility at Friends, matching the engine default", () => {
    const modules = groupsForTab(loadSettingsCatalog(), "controls").flatMap(
      (g) => g.modules,
    );
    const marker = modules.find((m) => m.key === "markerVisibility");
    expect(marker).toMatchObject({
      kind: "dropdown",
      setting: "Point at marker visibility",
      default: 1,
    });
    expect(marker?.options?.[marker.default ?? -1]).toBe("Friends");
  });

  it("ships the engine-only graphics modules the SPA panel needs", () => {
    const modules = groupsForTab(loadSettingsCatalog(), "graphics").flatMap(
      (g) => g.modules,
    );
    expect(modules.find((m) => m.setting === "Scene Load Distance")).toBeTruthy();
    expect(modules.find((m) => m.setting === "Empty Parcel Props")).toBeTruthy();
    expect(modules.find((m) => m.setting === "Light Count")).toBeTruthy();
  });

  it("ships Avatar Cel Shading and Avatar Outline as separate engine-backed modules", () => {
    const modules = groupsForTab(loadSettingsCatalog(), "graphics").flatMap(
      (g) => g.modules,
    );
    expect(modules.find((m) => m.key === "avatarCelShading")).toMatchObject({
      kind: "toggle",
      title: "Avatar Cel Shading",
      setting: "Avatar Cel Shading",
      default: 1,
    });
    const outline = modules.find((m) => m.key === "avatarOutline");
    expect(outline).toMatchObject({
      kind: "dropdown",
      title: "Avatar Outline",
      setting: "Avatar Outline",
      default: 1,
    });
    expect(outline?.options).toEqual(["Always", "Focus", "Off"]);
  });
});

describe("parseSettingsFixture refinements", () => {
  it("rejects a module that names no engine setting", () => {
    expect(() =>
      parseSettingsFixture(
        fixtureWith({
          key: "mystery",
          kind: "toggle",
          title: "Mystery",
          options: ["Off", "On"],
          default: 1,
        }),
      ),
    ).toThrow(/names no engine setting/);
  });

  it("rejects an engine-backed module without a seed default", () => {
    expect(() =>
      parseSettingsFixture(
        fixtureWith({
          key: "avatarOutline",
          kind: "dropdown",
          title: "Avatar Outline",
          setting: "Avatar Outline",
          options: ["Always", "Focus", "Off"],
        }),
      ),
    ).toThrow(/needs a seed default/);
  });

  it("accepts the fullscreen special without an engine setting", () => {
    const parsed = parseSettingsFixture(
      fixtureWith({ key: "fullscreen", kind: "toggle", title: "Fullscreen", fullscreen: true }),
    );
    expect(parsed.sections.graphics?.[0]?.modules).toHaveLength(1);
  });
});

describe("parseTab", () => {
  it("accepts known tabs, normalizing case and whitespace", () => {
    expect(parseTab("graphics")).toBe("graphics");
    expect(parseTab(" Sounds ")).toBe("sounds");
    expect(parseTab("CHAT")).toBe("chat");
    expect(parseTab("controls")).toBe("controls");
  });

  it("falls back to the default tab on junk", () => {
    expect(parseTab(null)).toBe(DEFAULT_TAB);
    expect(parseTab(undefined)).toBe(DEFAULT_TAB);
    expect(parseTab("")).toBe(DEFAULT_TAB);
    expect(parseTab("bogus")).toBe(DEFAULT_TAB);
  });
});

describe("defaultValues", () => {
  it("seeds every engine-backed setting, with Avatar Outline at the engine default (Focus)", () => {
    const defaults = defaultValues(loadSettingsCatalog());
    expect(defaults["Avatar Outline"]).toBe(1);
    expect(defaults["Avatar Cel Shading"]).toBe(1);
    expect(defaults["Point at marker visibility"]).toBe(1);
    for (const m of allModules()) {
      if (m.setting) expect(defaults[m.setting], m.setting).toBeDefined();
    }
  });
});
