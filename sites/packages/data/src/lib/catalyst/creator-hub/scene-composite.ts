
export type CompositeEnvelope = { json: unknown };

export type CompositeComponentBlock = {
  name: string;
  data: Record<string, CompositeEnvelope>;
};

export type SceneComposite = {
  version: number;
  components: CompositeComponentBlock[];
};

export const TRANSFORM = "core::Transform";
export const NAME = "core-schema::Name";

export const ROOT_ENTITY = 0;
export const FIRST_AUTHORED_ENTITY = 512;

export type Vec3 = { x: number; y: number; z: number };
export type Quat = { x: number; y: number; z: number; w: number };
export type TransformValue = {
  position: Vec3;
  scale: Vec3;
  rotation: Quat;
  parent?: number;
};

export function identityTransform(parent = ROOT_ENTITY): TransformValue {
  return {
    position: { x: 0, y: 0, z: 0 },
    scale: { x: 1, y: 1, z: 1 },
    rotation: { x: 0, y: 0, z: 0, w: 1 },
    parent,
  };
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

function envelope(value: unknown): CompositeEnvelope {
  if (isObj(value) && "json" in value && Object.keys(value).length === 1) {
    return { json: (value as CompositeEnvelope).json };
  }
  return { json: value };
}

export function emptyComposite(): SceneComposite {
  return { version: 1, components: [] };
}

export function parseComposite(input: unknown): SceneComposite {
  if (!isObj(input)) return emptyComposite();
  const version = typeof input.version === "number" ? input.version : 1;
  const rawComponents = Array.isArray(input.components) ? input.components : [];
  const components: CompositeComponentBlock[] = [];
  for (const raw of rawComponents) {
    if (!isObj(raw) || typeof raw.name !== "string") continue;
    const data: Record<string, CompositeEnvelope> = {};
    if (isObj(raw.data)) {
      for (const [id, value] of Object.entries(raw.data)) {
        data[id] = envelope(value);
      }
    }
    components.push({ name: raw.name, data });
  }
  return { version, components };
}

export function serializeSceneComposite(c: SceneComposite): string {
  const out = {
    version: c.version,
    components: c.components.map((block) => ({
      name: block.name,
      data: Object.fromEntries(
        Object.entries(block.data).map(([id, env]) => [id, { json: env.json }]),
      ),
    })),
  };
  return JSON.stringify(out);
}

export function getComponentBlock(
  c: SceneComposite,
  name: string,
): CompositeComponentBlock | undefined {
  return c.components.find((b) => b.name === name);
}

export function getComponentValue(
  c: SceneComposite,
  entityId: number,
  name: string,
): unknown {
  const block = getComponentBlock(c, name);
  if (!block) return undefined;
  const env = block.data[String(entityId)];
  return env ? env.json : undefined;
}

export function listEntities(c: SceneComposite): number[] {
  const ids = new Set<number>();
  for (const block of c.components) {
    for (const key of Object.keys(block.data)) {
      const n = Number(key);
      if (Number.isFinite(n)) ids.add(n);
    }
  }
  return Array.from(ids).sort((a, b) => a - b);
}

export function parentOf(c: SceneComposite, entityId: number): number {
  const t = getComponentValue(c, entityId, TRANSFORM);
  if (isObj(t) && typeof t.parent === "number" && Number.isFinite(t.parent)) {
    return t.parent;
  }
  return ROOT_ENTITY;
}

export function childrenOf(c: SceneComposite, parent: number): number[] {
  return listEntities(c).filter((id) => id !== parent && parentOf(c, id) === parent);
}

export function nextEntityId(c: SceneComposite): number {
  let max = FIRST_AUTHORED_ENTITY - 1;
  for (const id of listEntities(c)) if (id > max) max = id;
  return max + 1;
}

export function entityName(c: SceneComposite, entityId: number): string {
  const n = getComponentValue(c, entityId, NAME);
  if (isObj(n)) {
    if (typeof n.value === "string" && n.value) return n.value;
    if (typeof n.name === "string" && n.name) return n.name;
  }
  return entityId === ROOT_ENTITY ? "Scene" : `Entity ${entityId}`;
}

export function setComponentValue(
  c: SceneComposite,
  entityId: number,
  name: string,
  value: unknown,
): SceneComposite {
  const next = clone(c);
  let block = next.components.find((b) => b.name === name);
  if (!block) {
    block = { name, data: {} };
    next.components.push(block);
  }
  block.data[String(entityId)] = { json: value };
  return next;
}

export function removeComponentValue(
  c: SceneComposite,
  entityId: number,
  name: string,
): SceneComposite {
  const next = clone(c);
  const block = next.components.find((b) => b.name === name);
  if (block) {
    delete block.data[String(entityId)];
    if (Object.keys(block.data).length === 0) {
      next.components = next.components.filter((b) => b !== block);
    }
  }
  return next;
}

export type AddEntityInput = {
  id?: number;
  name?: string;
  parent?: number;
  components?: Record<string, unknown>;
};

export function addEntity(
  c: SceneComposite,
  input: AddEntityInput = {},
): { composite: SceneComposite; entity: number } {
  const entity = input.id ?? nextEntityId(c);
  let next = setComponentValue(
    c,
    entity,
    TRANSFORM,
    identityTransform(input.parent ?? ROOT_ENTITY),
  );
  if (input.name) {
    next = setComponentValue(next, entity, NAME, { value: input.name });
  }
  if (input.components) {
    for (const [name, value] of Object.entries(input.components)) {
      next = setComponentValue(next, entity, name, value);
    }
  }
  return { composite: next, entity };
}

export function renameEntity(
  c: SceneComposite,
  entityId: number,
  name: string,
): SceneComposite {
  return setComponentValue(c, entityId, NAME, { value: name });
}

export function reparentEntity(
  c: SceneComposite,
  entityId: number,
  parent: number,
): SceneComposite {
  const existing = getComponentValue(c, entityId, TRANSFORM);
  const base: TransformValue = isObj(existing)
    ? (clone(existing) as TransformValue)
    : identityTransform();
  base.parent = parent;
  return setComponentValue(c, entityId, TRANSFORM, base);
}

export function descendantsOf(c: SceneComposite, entityId: number): number[] {
  const out: number[] = [];
  const stack = [entityId];
  const seen = new Set<number>();
  while (stack.length) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(id);
    for (const child of childrenOf(c, id)) stack.push(child);
  }
  return out;
}

export function deleteEntity(
  c: SceneComposite,
  entityId: number,
  opts: { recursive?: boolean } = {},
): SceneComposite {
  if (entityId === ROOT_ENTITY) return clone(c);
  const recursive = opts.recursive ?? true;
  const targets = new Set(recursive ? descendantsOf(c, entityId) : [entityId]);
  targets.delete(ROOT_ENTITY);

  const next = clone(c);
  for (const block of next.components) {
    for (const id of targets) delete block.data[String(id)];
  }
  next.components = next.components.filter((b) => Object.keys(b.data).length > 0);
  return next;
}

const DEFAULT_COMPONENT_VALUES: Readonly<Record<string, unknown>> = {
  "core::MeshRenderer": { mesh: { $case: "box", box: { uvs: [] } } },
  "core::MeshCollider": { mesh: { $case: "box", box: {} } },
  "core::Material": {
    material: { $case: "pbr", pbr: { albedoColor: { r: 0.8, g: 0.8, b: 0.8, a: 1 } } },
  },
  "core::GltfContainer": { src: "" },
  "core::VisibilityComponent": { visible: true },
};

export function buildCompositeFromHierarchy(
  nodes: ReadonlyArray<{
    entity: number;
    name: string;
    parent: number;
    components?: readonly string[];
  }>,
): SceneComposite {
  let c = emptyComposite();
  for (const n of nodes) {
    if (n.entity === ROOT_ENTITY) continue;
    c = setComponentValue(c, n.entity, TRANSFORM, identityTransform(n.parent));
    if (n.name) c = setComponentValue(c, n.entity, NAME, { value: n.name });
    for (const comp of n.components ?? []) {
      if (comp === TRANSFORM || comp === NAME) continue;
      const value = DEFAULT_COMPONENT_VALUES[comp];
      if (value !== undefined) c = setComponentValue(c, n.entity, comp, value);
    }
  }
  return c;
}

