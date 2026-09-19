import { linkedProviderId, listLinkedProviders } from "./linked-providers";
import { CatalystError } from "../client";
import { z } from "zod";
import { draftRequest, type DraftOptions } from "./drafts";
import { importCollectionItem } from "./collection-import";

const Collection = z.object({ id: z.string().uuid(), name: z.string(), is_published: z.boolean(),
  is_approved: z.boolean().optional(), third_party_id: z.string().nullish(), item_count: z.number().int().nonnegative().optional() });
export type CollectionDraftSummary = z.infer<typeof Collection>;

export async function listCollectionDrafts(opts: DraftOptions): Promise<CollectionDraftSummary[]> {
  return z.array(Collection).parse(await draftRequest("/v1/collections", opts));
}

export function collectionDraftWriter(options: () => DraftOptions & { address?: string }) {
  let id: string | undefined;
  const ids = new WeakMap<File, string>();
  return async (args: { name: string; type: "standard" | "linked"; items: { file?: File }[]; thirdPartyId?: string; signal?: AbortSignal }) => {
    const provider = args.type === "linked" ? linkedProviderId.safeParse(args.thirdPartyId) : null;
    if (provider && !provider.success) throw new Error("Choose a provider managed by your wallet before saving.");
    if (!args.name.trim() || args.name.trim().length > 32) throw new Error("Enter a collection name of 1 to 32 characters.");
    if (!args.items.length) throw new Error("Add an item to this collection.");
    const imports = [];
    for (const item of args.items) {
      if (!item.file) throw new Error("Choose the original item files before saving.");
      args.signal?.throwIfAborted();
      const parsed = await importCollectionItem(item.file);
      if (!ids.has(item.file)) ids.set(item.file, crypto.randomUUID());
      imports.push({ ...parsed, id: ids.get(item.file)! });
    }
    const opts = { ...options(), signal: args.signal };
    if (provider?.success) {
      if (!opts.address || !(await listLinkedProviders(opts.address, opts)).some(row => row.id === provider.data)) {
        throw new Error("This wallet no longer manages the selected provider. Choose another provider and retry.");
      }
    }
    id ??= crypto.randomUUID();
    const saved = Collection.parse(await draftRequest(`/v1/collections/${id}`, opts, {
      method: "PUT", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ collection: { id, name: args.name.trim(), ...(provider?.success ? { urn: `${provider.data}:${id}` } : {}) } }),
    }));
    for (const item of imports) {
      const path = `/v1/items/${item.id}`;
      const body = { id: item.id, name: item.name, description: item.description, type: item.type,
        rarity: item.rarity, collection_id: id, price: "0", data: item.data, thumbnail: item.thumbnail,
        contents: {} as Record<string, string> };
      const write = () => draftRequest(path, opts, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ item: body }) });
      try { await draftRequest(path, opts); }
      catch (error) {
        if (!(error instanceof CatalystError && error.status === 404)) throw error;
        await write();
      }
      const form = new FormData();
      for (const file of item.files) form.append("files", file, file.name);
      body.contents = z.record(z.string(), z.string()).parse(await draftRequest(`${path}/files`, opts, { method: "POST", body: form }));
      if (item.files.some(file => !body.contents[file.name])) throw new Error("Builder did not store every item file. Retry the save.");
      await write();
    }
    return { collectionId: saved.id, contractAddress: "", simulated: false };
  };
}
