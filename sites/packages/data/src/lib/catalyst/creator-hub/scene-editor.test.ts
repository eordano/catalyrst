import { describe, expect, it, vi } from "vitest";
import { loadSceneEditorSeed } from "./scene-editor";

function entity(base: string, title: string) {
  return {
    type: "scene", pointers: [base],
    content: [{ file: "main.composite", hash: "composite-hash" }],
    metadata: { display: { title }, scene: { base } },
  };
}

describe("published scene loading", () => {
  it("loads the selected World parcel and its composite from the Worlds service", async () => {
    const fetchImpl = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(Response.json({ scenes: [
        { entityId: "other", baseParcel: "0,0", parcels: ["0,0"] },
        { entityId: "selected", baseParcel: "2,3", parcels: ["2,3"] },
      ] }))
      .mockResolvedValueOnce(Response.json(entity("2,3", "Selected scene")))
      .mockResolvedValueOnce(Response.json({ components: [] }));
    const seed = await loadSceneEditorSeed({ world: "mock.dcl.eth", pointer: "2,3", base: "https://worlds.test", fetchImpl });
    expect(fetchImpl.mock.calls[0][0]).toBe("https://worlds.test/world/mock.dcl.eth/scenes");
    expect(fetchImpl.mock.calls[1][0]).toBe("https://worlds.test/contents/selected");
    expect(fetchImpl.mock.calls[2][0]).toBe("https://worlds.test/contents/composite-hash");
    expect(seed.scene).toMatchObject({ pointer: "mock.dcl.eth", base: "2,3", title: "Selected scene", live: true });
  });

  it("does not substitute another World scene when the selected parcel is missing", async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json({ scenes: [{ entityId: "other", baseParcel: "0,0", parcels: ["0,0"] }] }));
    const seed = await loadSceneEditorSeed({ world: "mock.dcl.eth", pointer: "2,3", base: "https://worlds.test", fetchImpl });
    expect(seed.scene).toMatchObject({ pointer: "mock.dcl.eth", base: "2,3", live: false });
    expect(fetchImpl).toHaveBeenCalledOnce();
  });

  it("keeps Genesis content requests on the catalyst content service", async () => {
    const fetchImpl = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(Response.json([entity("2,3", "Genesis scene")]))
      .mockResolvedValueOnce(Response.json({ components: [] }));
    const seed = await loadSceneEditorSeed({ pointer: "2,3", base: "https://catalyst.test", fetchImpl });
    expect(fetchImpl.mock.calls[0][0]).toBe("https://catalyst.test/content/entities/active");
    expect(fetchImpl.mock.calls[1][0]).toBe("https://catalyst.test/content/contents/composite-hash");
    expect(seed.scene).toMatchObject({ pointer: "2,3", live: true });
  });
});
