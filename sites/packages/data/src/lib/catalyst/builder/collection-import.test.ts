import { describe, expect, it } from "vitest";
import { zipSync, strToU8 } from "fflate";
import { importCollectionItem } from "./collection-import";

function archive(entries: Record<string, Uint8Array>): File {
  return new File([Uint8Array.from(zipSync(entries))], "collection-item.zip");
}
function glb() {
  const json = strToU8(JSON.stringify({ asset: { version: "2.0" } }).padEnd(32, " "));
  const bytes = new Uint8Array(20 + json.length);
  const header = new DataView(bytes.buffer);
  header.setUint32(0, 0x46546c67, true); header.setUint32(4, 2, true); header.setUint32(8, bytes.length, true);
  header.setUint32(12, json.length, true); header.setUint32(16, 0x4e4f534a, true); bytes.set(json, 20);
  return bytes;
}
const shape = "urn:decentraland:off-chain:base-avatars:BaseFemale";

describe("collection imports", () => {
  it("preserves upstream wearable.json metadata and every referenced file", async () => {
    const file = archive({
      "wearable.json": strToU8(JSON.stringify({ name: "Jacket", rarity: "rare", data: { category: "upper_body", tags: ["blue"], representations: [{ mainFile: "female/jacket.gltf", contents: ["female/jacket.gltf", "female/mesh.bin"], bodyShapes: [shape], overrideHides: ["hands_wear"] }] } })),
      "female/jacket.gltf": strToU8(JSON.stringify({ asset: { version: "2.0" }, buffers: [{ uri: "mesh.bin" }] })),
      "female/mesh.bin": new Uint8Array([1, 2, 3]),
    });
    const item = await importCollectionItem(file);
    expect(item).toMatchObject({ name: "Jacket", rarity: "rare", type: "wearable", data: { tags: ["blue"], representations: [{ bodyShapes: [shape], overrideHides: ["hands_wear"] }] } });
    expect(item.files.map(file => file.name)).toEqual(["wearable.json", "female/jacket.gltf", "female/mesh.bin"]);
  });

  it("reads emote.json instead of misclassifying it as a wearable", async () => {
    const item = await importCollectionItem(archive({ "emote.json": strToU8(JSON.stringify({ name: "Wave", category: "greetings", play_mode: "loop" })), "wave.glb": glb() }));
    expect(item).toMatchObject({ name: "Wave", type: "emote", data: { category: "greetings", loop: true } });
  });

  it("keeps male/female representations distinct without a manifest", async () => {
    const item = await importCollectionItem(archive({ "male/model.glb": glb(), "female/model.glb": glb() }));
    expect(item.data.representations).toMatchObject([{ bodyShapes: [shape.replace("Female", "Male")] }, { bodyShapes: [shape] }]);
  });

  it("rejects traversal and expanded ZIP limits before a save", async () => {
    await expect(importCollectionItem(archive({ "../model.glb": glb() }))).rejects.toThrow("invalid");
    await expect(importCollectionItem(archive({ "model.glb": new Uint8Array(20 * 1024 * 1024 + 1) }))).rejects.toThrow("expand");
  });

  it("rejects missing glTF resources and ambiguous models", async () => {
    await expect(importCollectionItem(new File([JSON.stringify({ asset: { version: "2.0" }, buffers: [{ uri: "missing.bin" }] })], "model.gltf"))).rejects.toThrow("missing resources");
    await expect(importCollectionItem(archive({ "one.glb": glb(), "two.glb": glb() }))).rejects.toThrow("representations");
  });
});
