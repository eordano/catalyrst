import { expect, it, vi } from "vitest";
import { collectionDraftWriter } from "./collection-drafts";

it("retries an interrupted collection import without duplicating items or erasing saved content", async () => {
  const collectionIds: string[] = [];
  const itemIds = new Set<string>();
  const items = new Map<string, Record<string, unknown>>();
  let fail = true;
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    const path = new URL(url).pathname;
    if (path.includes('/collections/')) {
      const collection = JSON.parse(init!.body as string).collection;
      collectionIds.push(collection.id);
      return Response.json({ data: { ...collection, is_published: false } });
    }
    const id = path.split('/')[3];
    itemIds.add(id);
    if (init?.method === 'PUT') {
      const item = JSON.parse(init.body as string).item;
      items.set(id, item);
      return Response.json({ data: item });
    }
    if (init?.method === 'POST') {
      if (fail) { fail = false; return Response.json({ message: 'Upload offline' }, { status: 503 }); }
      return Response.json({ data: { 'item.gltf': 'stored-cid' } });
    }
    return items.has(id) ? Response.json({ data: items.get(id) }) : Response.json({ message: 'Item not found' }, { status: 404 });
  });
  const writer = collectionDraftWriter(() => ({ fetch, base: 'https://builder.test' }));
  const args = { name: 'Saved collection', type: 'standard' as const, items: [{ file: new File([JSON.stringify({ asset: { version: '2.0' } })], 'item.gltf') }] };
  await expect(writer(args)).rejects.toThrow('Upload offline');
  const result = await writer(args);
  expect(new Set(collectionIds).size).toBe(1);
  expect(itemIds.size).toBe(1);
  expect(result).toMatchObject({ collectionId: collectionIds[0], contractAddress: '', simulated: false });
  expect([...items.values()][0].contents).toEqual({ 'item.gltf': 'stored-cid' });
  expect(fetch.mock.calls.filter(([url, init]) => url.includes('/items/') && init?.method === 'PUT')).toHaveLength(2);
});

it("validates every imported item before creating any server records", async () => {
  const fetch = vi.fn();
  const writer = collectionDraftWriter(() => ({ fetch }));
  await expect(writer({ name: 'Draft', type: 'standard', items: [{ file: new File([JSON.stringify({ asset: { version: '2.0' } })], 'valid.gltf') }, {}] })).rejects.toThrow('original item files');
  expect(fetch).not.toHaveBeenCalled();
});

it("saves linked drafts with the selected managed provider and refuses revoked access before writing", async () => {
  const provider = "urn:decentraland:matic:collections-thirdparty:example";
  let managed = true;
  const collections: Record<string, string>[] = [];
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith("/thirdParties")) return Response.json({ data: [{ id: provider, name: "Example", managers: managed ? ["0xowner"] : ["0xother"], published: true }] });
    if (url.includes("/collections/")) {
      const collection = JSON.parse(init!.body as string).collection;
      collections.push(collection);
      return Response.json({ data: { ...collection, is_published: false } });
    }
    if (init?.method === "POST") return Response.json({ data: { "item.gltf": "stored-cid" } });
    return init?.method === "PUT" ? Response.json({ data: {} }) : Response.json({ error: "Missing" }, { status: 404 });
  });
  const writer = collectionDraftWriter(() => ({ fetch, address: "0xOwner", base: "https://local.test" }));
  const args = { name: "Linked draft", type: "linked" as const, thirdPartyId: provider, items: [{ file: new File([JSON.stringify({ asset: { version: "2.0" } })], "item.gltf") }] };
  const result = await writer(args);
  expect(result.simulated).toBe(false);
  expect(collections[0].urn).toBe(`${provider}:${result.collectionId}`);
  managed = false;
  await expect(writer(args)).rejects.toThrow("no longer manages");
  expect(collections).toHaveLength(1);
  await expect(writer({ ...args, thirdPartyId: "https://evil.test" })).rejects.toThrow("Choose a provider");
});
