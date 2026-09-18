export interface DeviceEntity { scene: number; id: number; parent: number; components: Record<string, unknown> }
export interface DeviceTelemetry {
  entities: Record<string, DeviceEntity>;
  performance: Record<string, number> | null;
  fps: number[];
  limited: boolean;
}
export const emptyDeviceTelemetry = (): DeviceTelemetry => ({ entities: {}, performance: null, fps: [], limited: false });
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const integer = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

export function applyDeviceEntries(previous: DeviceTelemetry, entries: unknown[]): DeviceTelemetry {
  const next = { ...previous, entities: { ...previous.entities } };
  let count = Object.keys(next.entities).length;
  for (const raw of entries) {
    const entry = object(raw);
    if (entry.type === "perf") {
      next.performance = Object.fromEntries(Object.entries(entry).filter((value): value is [string, number] => typeof value[1] === "number" && Number.isFinite(value[1])));
      if (typeof next.performance.fps === "number") next.fps = [...next.fps, next.performance.fps].slice(-60);
      continue;
    }
    if (entry.type === "scene_lifecycle" && ["scene_init", "scene_dispose"].includes(String(entry.event)) && integer(entry.scene_id)) {
      for (const [key, entity] of Object.entries(next.entities)) if (entity.scene === entry.scene_id) { delete next.entities[key]; count--; }
      continue;
    }
    if (entry.type !== "crdt" || !integer(entry.sid) || !integer(entry.e)) continue;
    const key = `${entry.sid}:${entry.e}`;
    if (entry.op === "de") { if (next.entities[key]) count--; delete next.entities[key]; continue; }
    if (typeof entry.c !== "string" || ["__proto__", "constructor", "prototype"].includes(entry.c)) continue;
    if (!["p", "d", "a"].includes(String(entry.op))) continue;
    const previousEntity = next.entities[key];
    if (entry.op === "d" && !previousEntity) continue;
    if (!previousEntity && count >= 10000) { next.limited = true; continue; }
    if (!previousEntity) count++;
    const entity: DeviceEntity = previousEntity
      ? previousEntity === previous.entities[key] ? { ...previousEntity, components: { ...previousEntity.components } } : previousEntity
      : { scene: entry.sid, id: entry.e, parent: 0, components: {} };
    if (entry.op === "d") {
      delete entity.components[entry.c];
    } else if (entry.op === "a") {
      const old = entity.components[entry.c];
      entity.components[entry.c] = [...(Array.isArray(old) ? old : []), entry.payload ?? { binary: entry.bin }].slice(-100);
    } else {
      entity.components[entry.c] = entry.payload ?? { binary: entry.bin };
    }
    const transform = object(entity.components.Transform ?? entity.components["core::Transform"] ?? entity.components.UiTransform ?? entity.components["core::UiTransform"]);
    const parent = transform.parent ?? transform.parent_entity;
    entity.parent = integer(parent) ? parent : 0;
    next.entities[key] = entity;
  }
  return next;
}

export function deviceEntityDepth(entity: DeviceEntity, entities: DeviceTelemetry["entities"]): number {
  const seen = new Set([entity.id]);
  let depth = 0, parent = entities[`${entity.scene}:${entity.parent}`];
  while (parent && !seen.has(parent.id) && depth < 20) {
    seen.add(parent.id); depth++;
    parent = entities[`${entity.scene}:${parent.parent}`];
  }
  return depth;
}
