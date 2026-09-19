export type SpawnAreaDraft = {
  source: Record<string, unknown>;
  name: string;
  default: boolean;
  x: string;
  y: string;
  z: string;
  cameraX: string;
  cameraY: string;
  cameraZ: string;
};

const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const coordinate = (value: unknown, fallback = "") => typeof value === "number" ? String(value) : Array.isArray(value) ? value.join(", ") : fallback;

export function readSpawnAreas(value: unknown): SpawnAreaDraft[] {
  return (Array.isArray(value) ? value : []).map(value => {
    const source = structuredClone(object(value));
    const position = object(source.position), camera = object(source.cameraTarget);
    return {
      source, name: typeof source.name === "string" ? source.name : "", default: source.default === true,
      x: coordinate(position.x, "8"), y: coordinate(position.y, "0"), z: coordinate(position.z, "8"),
      cameraX: coordinate(camera.x), cameraY: coordinate(camera.y), cameraZ: coordinate(camera.z),
    };
  });
}

export function newSpawnArea(areas: SpawnAreaDraft[], duplicate?: SpawnAreaDraft): SpawnAreaDraft {
  let number = 1;
  while (areas.some(area => area.name === `SpawnArea${number}`)) number++;
  return { source: {}, x: "2", y: "0", z: "2", cameraX: "8", cameraY: "1", cameraZ: "8", ...structuredClone(duplicate), name: `SpawnArea${number}`, default: areas.length === 0 };
}

export function writeSpawnAreas(areas: SpawnAreaDraft[]): Record<string, unknown>[] {
  const names = new Set<string>();
  if (areas.filter(area => area.default).length > 1) throw new Error("Choose only one default spawn area.");
  return areas.map(area => {
    const before = readSpawnAreas([area.source])[0]!;
    const result = structuredClone(area.source);
    if (area.name !== before.name || !Object.keys(area.source).length) {
      if (!/^[a-zA-Z0-9_-]+$/.test(area.name)) throw new Error("Spawn area names may contain letters, numbers, underscores and hyphens.");
      result.name = area.name;
    }
    if (names.has(area.name)) throw new Error("Give each spawn area a different name.");
    names.add(area.name);
    if (area.default !== before.default || !Object.keys(area.source).length) result.default = area.default;
    if ((["x", "y", "z"] as const).some(key => area[key] !== before[key]) || !Object.keys(area.source).length) {
      const values = [area.x, area.y, area.z].map(value => {
        const parts = value.split(",").map(part => part.trim());
        const numbers = parts.map(Number);
        if (parts.some(part => !part) || numbers.length > 2 || numbers.some(number => !Number.isFinite(number)) || (numbers.length === 2 && numbers[0]! > numbers[1]!)) throw new Error("Spawn coordinates must be a number or an ordered range, such as 0, 1.");
        return numbers.length === 1 ? numbers[0]! : numbers;
      });
      const ranged = values.some(Array.isArray);
      const [x, y, z] = values.map(value => ranged && !Array.isArray(value) ? [value, value] : value);
      result.position = { ...object(result.position), x, y, z };
    }
    if ((["cameraX", "cameraY", "cameraZ"] as const).some(key => area[key] !== before[key]) || !Object.keys(area.source).length) {
      const values = [area.cameraX, area.cameraY, area.cameraZ].map(value => value.trim());
      if (values.every(value => !value)) delete result.cameraTarget;
      else {
        if (values.some(value => !value || !Number.isFinite(Number(value)))) throw new Error("Enter all three camera target coordinates, or leave them all blank.");
        const [x, y, z] = values.map(Number);
        result.cameraTarget = { ...object(result.cameraTarget), x, y, z };
      }
    }
    return result;
  });
}
