import { describe, expect, it, vi } from "vitest";
import { loadWearableDraft, saveWearableDraft, type WearableDraft } from "./drafts";

const id = "fd84234c-55d4-40a8-afbc-ea5a6fe00761";
const stored = {
  id, name: "Jacket", type: "wearable", collection_id: null, description: "Keep this",
  thumbnail: "thumbnail.png", rarity: "epic", price: "1500000000000000000",
  beneficiary: "0x1111111111111111111111111111111111111111", is_published: false,
  data: { category: "upper_body", tags: ["original"], representations: [{ mainFile: "jacket.glb", bodyShapes: ["female"] }] },
  contents: { "jacket.glb": "old-cid", "thumbnail.png": "image-cid" }, urn: null,
};
const draft: WearableDraft = {
  itemId: id, collectionId: "", name: "Edited jacket", modelFile: "jacket.glb",
  category: "upper_body", rarity: "rare", price: "2.000000000000000001", free: false,
};
const ok = (data: unknown) => Response.json({ data });
function model(): File {
  const bytes = new Uint8Array(12);
  const header = new DataView(bytes.buffer);
  header.setUint32(0, 0x46546c67, true); header.setUint32(4, 2, true); header.setUint32(8, bytes.length, true);
  return new File([bytes], "new.glb");
}

describe("wearable persistence", () => {
  it("reads stored values with exact MANA conversion", async () => {
    const fetch = vi.fn().mockResolvedValue(ok(stored));
    expect(await loadWearableDraft(id, { fetch, base: "https://builder.test" })).toEqual({
      itemId: id, collectionId: "", name: "Jacket", modelFile: "jacket.glb", category: "upper_body",
      rarity: "epic", price: "1.5", free: false,
    });
    expect(fetch).toHaveBeenCalledWith(`https://builder.test/v1/items/${id}`, expect.objectContaining({ cache: "no-store" }));
  });

  it("preserves unrelated metadata and content on an existing-item edit", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(ok(stored)).mockImplementationOnce(async (_url, init) => ok({ ...stored, ...JSON.parse(init.body).item }));
    const result = await saveWearableDraft(draft, { fetch });
    expect(result).toEqual({ itemId: id, urn: "" });
    const item = JSON.parse(fetch.mock.calls[1][1].body).item;
    expect(item).toMatchObject({ description: stored.description, thumbnail: stored.thumbnail, beneficiary: stored.beneficiary,
      contents: stored.contents, data: stored.data, price: "2000000000000000001", name: draft.name, rarity: "rare" });
  });

  it("uploads actual bytes before attaching content to a new item", async () => {
    let saved: unknown;
    const file = model();
    const fetch = vi.fn(async (_url: string, init?: RequestInit): Promise<Response> => {
      if (!init?.method) return Response.json({ message: "Item not found" }, { status: 404 });
      if (init.method === "POST") {
        const uploaded = (init.body as FormData).get("files") as File;
        expect(await uploaded.arrayBuffer()).toEqual(await file.arrayBuffer());
        return ok({ "new.glb": "new-cid" });
      }
      saved = JSON.parse(init.body as string).item;
      return ok({ ...stored, ...saved as object, is_published: false });
    });
    await saveWearableDraft({ ...draft, model: file, modelFile: file.name }, { fetch });
    const methods = fetch.mock.calls.map(([, init]) => init?.method ?? "GET");
    expect(methods).toContain("POST");
    expect(methods.indexOf("POST")).toBeLessThan(methods.lastIndexOf("PUT"));
    expect(saved).toMatchObject({ contents: { "new.glb": "new-cid" }, data: { representations: [{ mainFile: "new.glb" }] } });
  });

  it("does not report success or attach a model when upload fails", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(ok(stored)).mockResolvedValueOnce(Response.json({ message: "Upload unavailable" }, { status: 503 }));
    await expect(saveWearableDraft({ ...draft, model: model() }, { fetch })).rejects.toThrow("Upload unavailable");
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(fetch.mock.calls[1][1].method).toBe("POST");
  });

  it("keeps body-shape and override metadata when replacing an existing model", async () => {
    const fetch = vi.fn().mockResolvedValueOnce(ok(stored))
      .mockResolvedValueOnce(ok({ "new.glb": "new-cid" }))
      .mockImplementationOnce(async (_url, init) => ok({ ...stored, ...JSON.parse(init.body).item }));
    await saveWearableDraft({ ...draft, model: model() }, { fetch });
    const item = JSON.parse(fetch.mock.calls[2][1].body).item;
    expect(item.data.representations[0]).toMatchObject({ bodyShapes: ["female"], mainFile: "new.glb" });
    expect(item.contents["thumbnail.png"]).toBe("image-cid");
  });

  it("rejects glTF external assets instead of saving a broken model", async () => {
    const fetch = vi.fn();
    const file = new File([JSON.stringify({ asset: { version: "2.0" }, buffers: [{ uri: "missing.bin" }] })], "scene.gltf");
    await expect(saveWearableDraft({ ...draft, model: file }, { fetch })).rejects.toThrow("references separate files");
    expect(fetch).not.toHaveBeenCalled();
  });

  it.each(["-1", "1e9", "1.1234567890123456789", "NaN"])("rejects invalid price %s before writing", async price => {
    const fetch = vi.fn();
    await expect(saveWearableDraft({ ...draft, price }, { fetch })).rejects.toThrow("MANA price");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("rejects fake models and does not create an item after a permission error", async () => {
    const fetch = vi.fn().mockResolvedValue(Response.json({ message: "Not authorized" }, { status: 403 }));
    await expect(saveWearableDraft({ ...draft, model: new File(["not a model"], "fake.glb") }, { fetch })).rejects.toThrow("GLB");
    expect(fetch).not.toHaveBeenCalled();
    await expect(saveWearableDraft(draft, { fetch })).rejects.toThrow("Not authorized");
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
