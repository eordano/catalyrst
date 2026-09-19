import { readSpawnAreas, writeSpawnAreas, type SpawnAreaDraft } from "./spawn-areas";

export type SceneDocument = Record<string, unknown>;
export type SceneSettingsDraft = { title: string; description: string; thumbnail: string; tags: string; base: string; parcels: string; spawnAreas: SpawnAreaDraft[]; fixedTime: string; transition: string; terrain: string; voice: string; nearbyVoice: string; portables: string; rating: string };
export interface SceneSettingsFile { content: string; destination: string; save(content: string): Promise<void> }

const object = (value: unknown): SceneDocument => value !== null && typeof value === "object" && !Array.isArray(value) ? value as SceneDocument : {};
const string = (value: unknown) => typeof value === "string" ? value : "";

export function parseSceneDocument(content: string): SceneDocument {
  const value: unknown = JSON.parse(content);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Scene settings must contain a JSON object.");
  return value as SceneDocument;
}

export function readSceneSettings(scene: SceneDocument): SceneSettingsDraft {
  const display = object(scene.display), layout = object(scene.scene);
  const skybox = object(scene.skyboxConfig), toggles = object(scene.featureToggles);
  return {
    title: string(display.title), description: string(display.description), thumbnail: string(display.navmapThumbnail),
    tags: Array.isArray(scene.tags) ? scene.tags.join(", ") : "", base: string(layout.base),
    parcels: Array.isArray(layout.parcels) ? layout.parcels.join("\n") : "",
    spawnAreas: readSpawnAreas(scene.spawnPoints),
    fixedTime: typeof skybox.fixedTime === "number" ? String(skybox.fixedTime) : "",
    transition: typeof skybox.transitionMode === "number" ? String(skybox.transitionMode) : "",
    terrain: typeof scene.landscapeTerrain === "boolean" ? String(scene.landscapeTerrain) : "",
    voice: string(toggles.voiceChat), nearbyVoice: string(toggles.nearbyVoiceChat), portables: string(toggles.portableExperiences), rating: string(scene.rating),
  };
}

export function updateSceneSettings(scene: SceneDocument, next: SceneSettingsDraft): SceneDocument {
  const before = readSceneSettings(scene);
  const result = structuredClone(scene);
  const changed = (key: keyof SceneSettingsDraft) => next[key] !== before[key];
  for (const [key, field] of [["title", "title"], ["description", "description"], ["thumbnail", "navmapThumbnail"]] as const) {
    if (!changed(key)) continue;
    if (key === "title" && !next.title.trim()) throw new Error("Enter a scene name.");
    result.display = { ...object(result.display), [field]: next[key].trim() };
  }
  if (changed("tags")) result.tags = [...new Set(next.tags.split(",").map(tag => tag.trim()).filter(Boolean))];
  const optional = (target: SceneDocument, key: string, value: unknown) => { if (value === undefined) delete target[key]; else target[key] = value; };
  if (changed("fixedTime") || changed("transition")) {
    const skybox = { ...object(result.skyboxConfig) };
    if (changed("fixedTime")) {
      const value = next.fixedTime.trim() === "" ? undefined : Number(next.fixedTime);
      if (value !== undefined && (!Number.isFinite(value) || value < 0 || value > 86400)) throw new Error("Sky time must be between 0 and 86400 seconds, or blank for the normal day cycle.");
      optional(skybox, "fixedTime", value);
    }
    if (changed("transition")) {
      if (!["", "0", "1"].includes(next.transition)) throw new Error("Choose a supported sky transition.");
      optional(skybox, "transitionMode", next.transition === "" ? undefined : Number(next.transition));
    }
    result.skyboxConfig = skybox;
  }
  for (const [key, field] of [["voice", "voiceChat"], ["nearbyVoice", "nearbyVoiceChat"], ["portables", "portableExperiences"]] as const) {
    if (!changed(key)) continue;
    if (!["", "enabled", "disabled", ...(key === "portables" ? ["hideUi"] : [])].includes(next[key])) throw new Error("Choose a supported scene restriction.");
    const toggles = { ...object(result.featureToggles) };
    optional(toggles, field, next[key] || undefined);
    result.featureToggles = toggles;
  }
  if (changed("terrain")) {
    if (!["", "true", "false"].includes(next.terrain)) throw new Error("Choose a supported terrain setting.");
    optional(result, "landscapeTerrain", next.terrain === "" ? undefined : next.terrain === "true");
  }
  if (changed("rating")) {
    if (!["", "A"].includes(next.rating)) throw new Error("Choose a supported age rating.");
    optional(result, "rating", next.rating || undefined);
  }
  if (changed("base") || changed("parcels")) {
    const coordinate = (value: string) => {
      if (!/^-?\d+\s*,\s*-?\d+$/.test(value.trim())) throw new Error("Use parcel coordinates such as 0,0, one parcel per line.");
      return value.split(",").map(part => String(Number(part))).join(",");
    };
    const base = coordinate(next.base);
    const parcels = [...new Set(next.parcels.split("\n").map(line => line.trim()).filter(Boolean).map(coordinate))];
    if (!parcels.includes(base)) throw new Error("The base parcel must be included in the parcel list.");
    result.scene = { ...object(result.scene), base, parcels };
  }
  if (JSON.stringify(next.spawnAreas) !== JSON.stringify(before.spawnAreas)) {
    result.spawnPoints = writeSpawnAreas(next.spawnAreas);
  }
  return result;
}
