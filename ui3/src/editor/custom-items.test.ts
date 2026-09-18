import { describe, expect, it, vi } from "vitest";
import { customItemPath, customItemPayload, customItemStore, customItems } from "./custom-items";

export const COMPOSITE = JSON.stringify({ version: 1, components: [
  { name: "core::Name", data: { "512": { json: { value: "Chair" } }, "513": { json: { value: "Seat" } } } },
  { name: "core::Transform", data: { "512": { json: { parent: 0 } }, "513": { json: { parent: 512 } } } },
  { name: "inspector::Nodes", data: { "0": { json: { value: [{ entity: 0, children: [512] }, { entity: 512, children: [513] }] } } } },
] });

describe("custom item project storage", () => {
  it("uses the SDK project adapter directly, without copying storage features", async () => {
    const project = { id: "project", list: vi.fn(async () => []), read: vi.fn(async () => COMPOSITE), write: vi.fn(async () => {}) };
    expect(await customItemStore({ project })).toBe(project);
  });
  it("persists browser draft composites and reads them through the same contract after reopening", async () => {
    const files: Record<string, string> = {};
    const code = { hydrate: async () => files, persist: async (path: string, content: string) => { files[path] = content; } };
    const store = await customItemStore(code);
    const path = customItemPath("Chair", await store.list());
    await store.write(path, COMPOSITE);
    const reopened = await customItemStore(code);
    expect(customItems(await reopened.list())).toEqual([{ path: "assets/custom-items/chair.composite", name: "chair" }]);
    expect(customItemPayload(await reopened.read(path)).roots).toEqual(["512"]);
  });
  it("uses safe unique filenames and imports standard upstream composites", () => {
    expect(customItemPath("../\u00c1 chair", ["assets/custom-items/a-chair.composite"])).toBe("assets/custom-items/a-chair-2.composite");
    expect(customItems(["assets/custom-items/../escape.composite", "src/index.ts"])).toEqual([]);
    expect(customItemPayload(COMPOSITE).nodes).toHaveLength(2);
    expect(() => customItemPayload('{"version":1,"components":[]}')).toThrow(/no authored entities/);
  });
});
