// GENERATED from catalyrst/ui3/src/generated/catalyst/builder by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { BuilderCollectionOut } from "@ui/generated/catalyst/builder/BuilderCollectionOut";
import type { BulkItemStatusPatchOut } from "@ui/generated/catalyst/builder/BulkItemStatusPatchOut";
import type { CollectionStatus } from "@ui/generated/catalyst/builder/CollectionStatus";
import type { CollectionType } from "@ui/generated/catalyst/builder/CollectionType";
import type { CommitteeMemberOut } from "@ui/generated/catalyst/builder/CommitteeMemberOut";
import type { CurationCollectionsOut } from "@ui/generated/catalyst/builder/CurationCollectionsOut";
import type { CurationStatus } from "@ui/generated/catalyst/builder/CurationStatus";
import type { ItemKind } from "@ui/generated/catalyst/builder/ItemKind";
import type { ItemStatusPatchOut } from "@ui/generated/catalyst/builder/ItemStatusPatchOut";
import type { OrphanItemOut } from "@ui/generated/catalyst/builder/OrphanItemOut";
import type { ReviewCurationOut } from "@ui/generated/catalyst/builder/ReviewCurationOut";
import type { ReviewRowOut } from "@ui/generated/catalyst/builder/ReviewRowOut";

export const CollectionStatusSchema = z.enum(["synced", "under_review", "unsynced"]);

export const CollectionTypeSchema = z.literal("standard");

export const BuilderCollectionOutSchema = z.object({
  chain_id: z.number().nullable(),
  contract_address: z.string(),
  count: z.number(),
  created_at: z.number().nullable(),
  creator: z.string().nullable(),
  id: z.string(),
  is_approved: z.boolean(),
  is_published: z.boolean(),
  name: z.string(),
  network: z.string().nullable(),
  owner: z.string().nullable(),
  pending: z.boolean(),
  reviewed_at: z.number().nullable(),
  status: CollectionStatusSchema,
  third_party_id: z.string().nullable(),
  thumbs: z.array(z.string()),
  type: CollectionTypeSchema,
  updated_at: z.number().nullable(),
  urn: z.string().nullable(),
});

export const BulkItemStatusPatchOutSchema = z.object({
  collection_id: z.string(),
  requested: z.number(),
  status: z.string(),
  updated: z.number(),
});

export const CommitteeMemberOutSchema = z.object({
  address: z.string(),
  name: z.string(),
});

export const CurationStatusSchema = z.enum(["approved", "rejected"]);

export const ReviewCurationOutSchema = z.object({
  assignee: z.string(),
  collection_id: z.string(),
  created_at: z.number().nullable(),
  id: z.string(),
  status: CurationStatusSchema,
  updated_at: z.number().nullable(),
});

export const ReviewRowOutSchema = z.object({
  created_at: z.number().nullable(),
  curation: ReviewCurationOutSchema.nullable(),
  has_reviews: z.boolean(),
  id: z.string(),
  is_approved: z.boolean(),
  is_programmatic: z.boolean(),
  item_count: z.number(),
  name: z.string(),
  owner: z.string().nullable(),
  reviewed_at: z.number().nullable(),
  status: CollectionStatusSchema,
  type: CollectionTypeSchema,
});

export const CurationCollectionsOutSchema = z.object({
  collections: z.array(ReviewRowOutSchema),
  committee: z.array(CommitteeMemberOutSchema),
});

export const ItemKindSchema = z.enum(["smart_wearable", "emote", "wearable"]);

export const ItemStatusPatchOutSchema = z.object({
  collection_id: z.string(),
  id: z.string(),
  status: z.string(),
  updated: z.number(),
});

export const OrphanItemOutSchema = z.object({
  beneficiary: z.string().nullable(),
  category: z.string().nullable(),
  collection_id: z.string().nullable(),
  createdAt: z.number().nullable(),
  grad: z.string().nullable(),
  id: z.string(),
  image: z.string().nullable(),
  is_published: z.boolean(),
  max_supply: z.number().nullable(),
  name: z.string(),
  network: z.string().nullable(),
  price: z.string().nullable(),
  rarity: z.string().nullable(),
  status: CollectionStatusSchema,
  total_supply: z.number().nullable(),
  type: ItemKindSchema,
  updatedAt: z.number().nullable(),
  urn: z.string().nullable(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertBuilderCollectionOut = Assert<Mutual<BuilderCollectionOut, z.infer<typeof BuilderCollectionOutSchema>>>;
export type _AssertBulkItemStatusPatchOut = Assert<Mutual<BulkItemStatusPatchOut, z.infer<typeof BulkItemStatusPatchOutSchema>>>;
export type _AssertCollectionStatus = Assert<Mutual<CollectionStatus, z.infer<typeof CollectionStatusSchema>>>;
export type _AssertCollectionType = Assert<Mutual<CollectionType, z.infer<typeof CollectionTypeSchema>>>;
export type _AssertCommitteeMemberOut = Assert<Mutual<CommitteeMemberOut, z.infer<typeof CommitteeMemberOutSchema>>>;
export type _AssertCurationCollectionsOut = Assert<Mutual<CurationCollectionsOut, z.infer<typeof CurationCollectionsOutSchema>>>;
export type _AssertCurationStatus = Assert<Mutual<CurationStatus, z.infer<typeof CurationStatusSchema>>>;
export type _AssertItemKind = Assert<Mutual<ItemKind, z.infer<typeof ItemKindSchema>>>;
export type _AssertItemStatusPatchOut = Assert<Mutual<ItemStatusPatchOut, z.infer<typeof ItemStatusPatchOutSchema>>>;
export type _AssertOrphanItemOut = Assert<Mutual<OrphanItemOut, z.infer<typeof OrphanItemOutSchema>>>;
export type _AssertReviewCurationOut = Assert<Mutual<ReviewCurationOut, z.infer<typeof ReviewCurationOutSchema>>>;
export type _AssertReviewRowOut = Assert<Mutual<ReviewRowOut, z.infer<typeof ReviewRowOutSchema>>>;
