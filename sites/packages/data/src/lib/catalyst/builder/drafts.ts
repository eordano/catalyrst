import { validateModelFile } from "./model-file";
import { z } from "zod";
import { formatUnits, parseUnits } from "viem";
import { catalystBase, CatalystError } from "../client";

export type WearableDraft = {
  collectionId: string;
  itemId: string;
  name: string;
  modelFile: string;
  model?: File;
  category: string;
  rarity: string;
  price: string;
  free: boolean;
};

export type DraftOptions = {
  fetch: (url: string, init?: RequestInit) => Promise<Response>;
  base?: string;
  signal?: AbortSignal;
};

const Item = z.object({
  id: z.string().uuid(), name: z.string(), type: z.enum(["wearable", "emote"]),
  collection_id: z.string().nullable(), description: z.string().nullish().transform(value => value ?? ""), thumbnail: z.string().nullable(),
  rarity: z.string().nullable(), price: z.string().nullable(), beneficiary: z.string().nullable(),
  is_published: z.boolean(), data: z.record(z.string(), z.unknown()),
  contents: z.record(z.string(), z.string()), urn: z.string().nullable().optional(),
});

export async function draftRequest(path: string, opts: DraftOptions, init: RequestInit = {}): Promise<unknown> {
  const url = `${catalystBase(opts.base)}${path}`;
  const response = await opts.fetch(url, { ...init, signal: opts.signal, cache: "no-store" });
  const body = await response.json().catch(() => null) as { data?: unknown; message?: string; error?: string } | null;
  if (!response.ok) throw new CatalystError(body?.message ?? body?.error ?? `Could not save the draft (${response.status}).`, url, response.status);
  if (!body || !("data" in body)) throw new Error("Builder returned an invalid draft response.");
  return body.data;
}

function itemPath(id: string): string {
  if (!z.string().uuid().safeParse(id).success) throw new Error("Invalid wearable draft id.");
  return `/v1/items/${id}`;
}

export async function loadWearableDraft(id: string, opts: DraftOptions): Promise<WearableDraft> {
  const item = Item.parse(await draftRequest(itemPath(id), opts));
  if (item.type !== "wearable") throw new Error("Open emotes in the emote editor.");
  if (item.is_published) throw new Error("Published wearables cannot be edited as drafts.");
  const reps = z.array(z.object({ mainFile: z.string() })).safeParse(item.data.representations);
  return {
    itemId: item.id, collectionId: item.collection_id ?? "", name: item.name,
    modelFile: reps.success ? reps.data[0]?.mainFile ?? "" : "",
    category: typeof item.data.category === "string" ? item.data.category : "upper_body",
    rarity: item.rarity ?? "common", price: formatUnits(BigInt(item.price ?? "0"), 18),
    free: BigInt(item.price ?? "0") === 0n,
  };
}

export async function saveWearableDraft(draft: WearableDraft, opts: DraftOptions): Promise<{ itemId: string; urn: string }> {
  const path = itemPath(draft.itemId);
  if (!draft.name.trim() || draft.name.length > 64) throw new Error("Enter a wearable name of 1 to 64 characters.");
  if (!draft.free && !/^\d+(\.\d{1,18})?$/.test(draft.price)) throw new Error("Enter a non-negative MANA price with at most 18 decimal places.");
  const price = draft.free ? "0" : parseUnits(draft.price, 18).toString();
  const model = draft.model;
  if (model) await validateModelFile(model);
  let existing: z.infer<typeof Item> | undefined;
  try { existing = Item.parse(await draftRequest(path, opts)); }
  catch (error) { if (!(error instanceof CatalystError && error.status === 404)) throw error; }
  if (existing?.is_published) throw new Error("Published wearables cannot be edited as drafts.");
  if (existing && existing.type !== "wearable") throw new Error("Open emotes in the emote editor.");
  if (!model && !existing?.contents[draft.modelFile]) throw new Error("Choose a model before saving this wearable.");
  const write = (item: unknown) => draftRequest(path, opts, {
    method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ item }),
  });
  const item = {
    id: draft.itemId, name: draft.name.trim(), type: "wearable",
    collection_id: draft.collectionId || null, description: existing?.description ?? "",
    thumbnail: existing?.thumbnail ?? null, rarity: draft.rarity, price,
    beneficiary: existing?.beneficiary ?? null,
    data: { ...existing?.data, category: draft.category }, contents: { ...existing?.contents },
  };
  if (!existing) await write(item);
  if (model) {
    const form = new FormData();
    form.append("files", model, model.name);
    const uploaded = z.record(z.string(), z.string()).parse(await draftRequest(`${path}/files`, opts, { method: "POST", body: form }));
    if (!uploaded[model.name]) throw new Error("Builder did not store the selected model.");
    Object.assign(item.contents, uploaded);
    const representations = z.array(z.record(z.string(), z.unknown())).safeParse(existing?.data.representations);
    const previous = representations.success ? representations.data : [];
    const mainFile = previous[0]?.mainFile;
    Object.assign(item.data, { representations: previous.length ? previous.map(rep => rep.mainFile === mainFile ? {
      ...rep, mainFile: model.name,
      contents: [...(Array.isArray(rep.contents) ? rep.contents.filter(file => file !== mainFile && file !== model.name) : []), model.name],
    } : rep) : [{
      bodyShapes: ["urn:decentraland:off-chain:base-avatars:BaseMale", "urn:decentraland:off-chain:base-avatars:BaseFemale"],
      mainFile: model.name, contents: [model.name], overrideHides: [], overrideReplaces: [],
    }] });
  }
  const saved = Item.parse(await write(item));
  return { itemId: saved.id, urn: saved.urn ?? "" };
}

const ItemSummary = z.object({ id: z.string().uuid(), name: z.string(), type: z.enum(["wearable", "emote"]), collection_id: z.string().nullable() });
export type ItemDraftSummary = z.infer<typeof ItemSummary>;
export async function listItemDrafts(opts: DraftOptions): Promise<ItemDraftSummary[]> {
  return z.array(ItemSummary).parse(await draftRequest("/v1/items", opts));
}
