import { z } from "zod";
import { warnInvalid } from "../warn";

import type { BuilderCollectionOut as RsBuilderCollectionOut } from "@ui/generated/catalyst/builder/BuilderCollectionOut";
import type { OrphanItemOut as RsOrphanItemOut } from "@ui/generated/catalyst/builder/OrphanItemOut";

const nullableStr = z.string().nullish().transform((v) => v ?? null);
const nullableNum = z.number().nullish().transform((v) => v ?? null);

export const COLLECTION_TYPES = ["standard", "third_party"] as const;
export type CollectionType = (typeof COLLECTION_TYPES)[number];

export const COLLECTION_STATUSES = [
  "synced",
  "under_review",
  "unsynced",
  "loading",
] as const;
export type CollectionStatus = (typeof COLLECTION_STATUSES)[number];

export const COLLECTION_SORTS = [
  "MOST_RELEVANT",
  "CREATED_AT_DESC",
  "CREATED_AT_ASC",
  "NAME_DESC",
  "NAME_ASC",
  "UPDATED_AT_DESC",
  "UPDATED_AT_ASC",
] as const;
export type CollectionSort = (typeof COLLECTION_SORTS)[number];

export const BuilderCollectionSchema = z.object({
  id: z.string(),
  name: z.string(),
  type: z.enum(COLLECTION_TYPES),
  is_published: z.boolean(),
  is_approved: z.boolean(),
  reviewed_at: nullableNum,
  created_at: nullableNum,
  updated_at: nullableNum,
  contract_address: nullableStr,
  third_party_id: nullableStr,
  urn: nullableStr,
  status: z.enum(COLLECTION_STATUSES).nullish().transform((v) => v ?? null),
  pending: z.boolean().nullish().transform((v) => v ?? false),
  count: z.number(),
  thumbs: z.array(z.string()),
});
export type BuilderCollection = z.infer<typeof BuilderCollectionSchema>;

export const ORPHAN_ITEM_TYPES = ["wearable", "emote", "smart_wearable"] as const;

export const ORPHAN_ITEM_STATUSES = [
  "synced",
  "under_review",
  "unsynced",
  "loading",
  "unpublished",
] as const;

export const OrphanItemSchema = z.object({
  id: z.string(),
  name: z.string(),
  type: z.enum(ORPHAN_ITEM_TYPES),
  status: z.enum(ORPHAN_ITEM_STATUSES),
  createdAt: nullableNum,
  updatedAt: nullableNum,
  grad: nullableStr,
  image: nullableStr,
  collection_id: nullableStr,
});
export type OrphanItem = z.infer<typeof OrphanItemSchema>;

function parseRows<S extends z.ZodTypeAny>(
  schema: S,
  kind: string,
  raw: unknown,
): z.infer<S>[] {
  if (!Array.isArray(raw)) {
    warnInvalid(`${kind} list`, "not an array");
    return [];
  }
  const out: z.infer<S>[] = [];
  for (const row of raw) {
    const r = schema.safeParse(row);
    if (r.success) out.push(r.data);
    else warnInvalid(kind, r.error.issues);
  }
  return out;
}

export function parseCollections(raw: unknown): BuilderCollection[] {
  return parseRows(BuilderCollectionSchema, "BuilderCollection", raw);
}

export function parseOrphanItems(raw: unknown): OrphanItem[] {
  return parseRows(OrphanItemSchema, "OrphanItem", raw);
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export type _DriftBuilderCollection = Assert<
  AssignableTo<RsBuilderCollectionOut, z.input<typeof BuilderCollectionSchema>>
>;
export type _DriftOrphanItem = Assert<
  AssignableTo<RsOrphanItemOut, z.input<typeof OrphanItemSchema>>
>;
