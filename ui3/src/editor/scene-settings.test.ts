import { describe, expect, it } from "vitest";
import { parseSceneDocument, readSceneSettings, updateSceneSettings } from "./scene-settings";
import { newSpawnArea } from "./spawn-areas";

const scene = {
  display: { title: "Beach", description: "Sand", custom: { retained: true } },
  scene: { base: "0,0", parcels: ["0,0"], extension: true },
  spawnPoints: [{ name: "dock", default: true, position: { x: 8, y: [0, 1], z: 8 }, cameraTarget: { x: 0, y: 2, z: 0 } }, { name: "other", position: { x: 1, y: 0, z: 1 } }],
  requiredPermissions: ["ALLOW_TO_TRIGGER_AVATAR_EMOTE"], future: { version: 12 },
};
describe("shared scene settings", () => {
  it("preserves all unknown metadata and additional spawn points when editing fields", () => {
    const updated = updateSceneSettings(scene, { ...readSceneSettings(scene), title: "New beach", spawnAreas: readSceneSettings(scene).spawnAreas.map((area, i) => i === 0 ? { ...area, y: "2, 3" } : area) });
    expect(updated).toEqual({ ...scene, display: { ...scene.display, title: "New beach" }, spawnPoints: [{ ...scene.spawnPoints[0], position: { x: [8, 8], y: [2, 3], z: [8, 8] } }, scene.spawnPoints[1]] });
    expect(scene.display.title).toBe("Beach");
  });
  it("does not create optional fields when opening and saving an unchanged document", () => {
    expect(updateSceneSettings(scene, readSceneSettings(scene))).toEqual(scene);
    expect(() => parseSceneDocument("[]")).toThrow(/object/);
  });
  it("rejects invalid layout and spawn ranges before persistence", () => {
    const draft = readSceneSettings(scene);
    expect(() => updateSceneSettings(scene, { ...draft, base: "3,0" })).toThrow(/base parcel/);
    expect(() => updateSceneSettings(scene, { ...draft, parcels: "0,0\ninvalid" })).toThrow(/coordinates/);
    expect(() => updateSceneSettings(scene, { ...draft, spawnAreas: draft.spawnAreas.map(area => ({ ...area, y: "5,2" })) })).toThrow(/ordered range/);
  });
  it("persists upstream environment and restriction fields while preserving extensions", () => {
    const original = { ...scene, skyboxConfig: { extra: true, fixedTime: 100 }, featureToggles: { customToggle: "kept" } };
    const updated = updateSceneSettings(original, { ...readSceneSettings(original), fixedTime: "43200", transition: "1", terrain: "false", voice: "disabled", nearbyVoice: "disabled", portables: "hideUi", rating: "A" });
    expect(updated.skyboxConfig).toEqual({ extra: true, fixedTime: 43200, transitionMode: 1 });
    expect(updated.featureToggles).toEqual({ customToggle: "kept", voiceChat: "disabled", nearbyVoiceChat: "disabled", portableExperiences: "hideUi" });
    expect(updated.landscapeTerrain).toBe(false);
    expect(updated.rating).toBe("A");
    expect(updateSceneSettings(updated, { ...readSceneSettings(updated), fixedTime: "" }).skyboxConfig).toEqual({ extra: true, transitionMode: 1 });
  });
  it("duplicates and edits secondary spawn areas without dropping camera or extension fields", () => {
    const original = { ...scene, spawnPoints: scene.spawnPoints.map(point => ({ ...point, extension: { keep: true } })) };
    const draft = readSceneSettings(original);
    const copy = newSpawnArea(draft.spawnAreas, draft.spawnAreas[0]);
    const updated = updateSceneSettings(original, { ...draft, spawnAreas: [draft.spawnAreas[1]!, { ...copy, y: "4", cameraX: "3" }] });
    expect(updated.spawnPoints).toEqual([original.spawnPoints[1], { ...original.spawnPoints[0], name: "SpawnArea1", default: false, position: { x: 8, y: 4, z: 8 }, cameraTarget: { x: 3, y: 2, z: 0 } }]);
    expect(updateSceneSettings(updated, readSceneSettings(updated))).toEqual(updated);
    expect(updateSceneSettings(updated, { ...readSceneSettings(updated), spawnAreas: [] }).spawnPoints).toEqual([]);
  });
  it("rejects incomplete camera targets, duplicate names and multiple defaults", () => {
    const draft = readSceneSettings(scene);
    expect(() => updateSceneSettings(scene, { ...draft, spawnAreas: draft.spawnAreas.map(area => ({ ...area, cameraY: "" })) })).toThrow(/all three camera/);
    expect(() => updateSceneSettings(scene, { ...draft, spawnAreas: draft.spawnAreas.map(area => ({ ...area, name: "same" })) })).toThrow(/different name/);
    expect(() => updateSceneSettings(scene, { ...draft, spawnAreas: draft.spawnAreas.map(area => ({ ...area, default: true })) })).toThrow(/one default/);
  });
});
