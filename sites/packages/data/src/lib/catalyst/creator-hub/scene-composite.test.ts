import { describe, expect, it } from "vitest";

import {
  addEntity,
  buildCompositeFromHierarchy,
  childrenOf,
  deleteEntity,
  descendantsOf,
  emptyComposite,
  entityName,
  getComponentValue,
  listEntities,
  NAME,
  nextEntityId,
  parentOf,
  parseComposite,
  removeComponentValue,
  renameEntity,
  reparentEntity,
  serializeSceneComposite,
  setComponentValue,
  TRANSFORM,
  type SceneComposite,
} from "./scene-composite";

const SAMPLE = {
  version: 1,
  components: [
    {
      name: "core::Transform",
      data: {
        "512": { json: { position: { x: 1, y: 0, z: 2 }, scale: { x: 1, y: 1, z: 1 }, rotation: { x: 0, y: 0, z: 0, w: 1 }, parent: 0 } },
        "513": { json: { position: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 }, rotation: { x: 0, y: 0, z: 0, w: 1 }, parent: 512 } },
      },
    },
    {
      name: "core-schema::Name",
      data: {
        "512": { json: { value: "Parent" } },
        "513": { json: { value: "Child" } },
      },
    },
    {
      name: "core::MeshRenderer",
      data: { "513": { json: { mesh: { $case: "box", box: {} } } } },
    },
  ],
};

describe("parse / serialize round-trip", () => {
  it("round-trips the wire composite and serializes to the exact { json }-enveloped wire JSON", () => {
    const parsed = parseComposite(SAMPLE);
    const reparsed = parseComposite(JSON.parse(serializeSceneComposite(parsed)));
    expect(reparsed).toEqual(parsed);

    const wire = JSON.parse(serializeSceneComposite(parsed)) as {
      version: number;
      components: { name: string; data: Record<string, { json: unknown }> }[];
    };
    const tf = wire.components.find((b) => b.name === "core::Transform")!;
    expect(tf.data["512"]).toEqual({ json: getComponentValue(parsed, 512, TRANSFORM) });
    expect(wire.version).toBe(1);
  });

  it("is lenient: bare (un-enveloped) values are normalized, junk -> empty", () => {
    const bare = parseComposite({
      version: 2,
      components: [{ name: "core-schema::Name", data: { "7": { value: "Bare" } } }],
    });
    expect(getComponentValue(bare, 7, NAME)).toEqual({ value: "Bare" });
    expect(parseComposite(null)).toEqual(emptyComposite());
    expect(parseComposite(42)).toEqual(emptyComposite());
  });
});

describe("read accessors", () => {
  const c = parseComposite(SAMPLE);
  it("lists entity ids, reads parent + children from Transform, names with fallback, and allocates the next authored id", () => {
    expect(listEntities(c)).toEqual([512, 513]);

    expect(parentOf(c, 513)).toBe(512);
    expect(parentOf(c, 512)).toBe(0);
    expect(childrenOf(c, 512)).toEqual([513]);
    expect(childrenOf(c, 0)).toEqual([512]);

    expect(entityName(c, 512)).toBe("Parent");
    expect(entityName(c, 999)).toBe("Entity 999");
    expect(entityName(c, 0)).toBe("Scene");

    expect(nextEntityId(c)).toBe(514);
    expect(nextEntityId(emptyComposite())).toBe(512);
  });
});

describe("edit operations are pure + produce updated composites", () => {
  it("addEntity, renameEntity and reparentEntity update the copy and leave the source untouched", () => {
    const c = parseComposite(SAMPLE);
    const { composite, entity } = addEntity(c, { name: "New", parent: 512 });
    expect(entity).toBe(514);
    expect(parentOf(composite, 514)).toBe(512);
    expect(entityName(composite, 514)).toBe("New");
    expect(listEntities(c)).toEqual([512, 513]);
    expect(listEntities(composite)).toEqual([512, 513, 514]);

    const renamed = renameEntity(c, 513, "Renamed");
    expect(entityName(renamed, 513)).toBe("Renamed");
    expect(entityName(c, 513)).toBe("Child");

    const reparented = reparentEntity(c, 513, 0);
    expect(parentOf(reparented, 513)).toBe(0);
    expect(getComponentValue(reparented, 513, TRANSFORM)).toMatchObject({
      position: { x: 0, y: 0, z: 0 },
      parent: 0,
    });
    expect(parentOf(c, 513)).toBe(512);
  });

  it("setComponentValue upserts (creating the block) and removeComponentValue drops it (emptying dead blocks)", () => {
    const c = parseComposite(SAMPLE);
    const set = setComponentValue(c, 512, "core::VisibilityComponent", { visible: false });
    expect(getComponentValue(set, 512, "core::VisibilityComponent")).toEqual({ visible: false });
    expect(getComponentValue(c, 512, "core::VisibilityComponent")).toBeUndefined();

    const removed = removeComponentValue(c, 513, "core::MeshRenderer");
    expect(getComponentValue(removed, 513, "core::MeshRenderer")).toBeUndefined();
    expect(removed.components.some((b) => b.name === "core::MeshRenderer")).toBe(false);
  });

  it("deleteEntity removes the entity + descendants from EVERY block and never removes the scene root", () => {
    const c = parseComposite(SAMPLE);
    expect(descendantsOf(c, 512)).toEqual(expect.arrayContaining([512, 513]));
    const next = deleteEntity(c, 512);
    expect(listEntities(next)).toEqual([]);
    const onlyParent = deleteEntity(c, 512, { recursive: false });
    expect(listEntities(onlyParent)).toEqual([513]);
    expect(getComponentValue(onlyParent, 512, NAME)).toBeUndefined();
    expect(deleteEntity(c, 0)).toEqual(c);
  });
});

describe("buildCompositeFromHierarchy", () => {
  it("builds an editable composite from the flat editor seed shape", () => {
    const c: SceneComposite = buildCompositeFromHierarchy([
      { entity: 0, name: "Scene", parent: 0 },
      { entity: 512, name: "Sign", parent: 0 },
      { entity: 513, name: "Light", parent: 512 },
    ]);
    expect(listEntities(c)).toEqual([512, 513]);
    expect(entityName(c, 512)).toBe("Sign");
    expect(parentOf(c, 513)).toBe(512);
    expect(parseComposite(JSON.parse(serializeSceneComposite(c)))).toEqual(c);
  });
});
