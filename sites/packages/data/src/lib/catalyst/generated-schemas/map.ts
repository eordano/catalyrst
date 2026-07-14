// GENERATED from catalyrst/ui3/src/generated/catalyst/map by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { LegacyTile } from "@ui/generated/catalyst/map/LegacyTile";
import type { NftAttribute } from "@ui/generated/catalyst/map/NftAttribute";
import type { NftMetadata } from "@ui/generated/catalyst/map/NftMetadata";
import type { RentalPeriod } from "@ui/generated/catalyst/map/RentalPeriod";
import type { Tile } from "@ui/generated/catalyst/map/Tile";
import type { TileRentalListing } from "@ui/generated/catalyst/map/TileRentalListing";
import type { TileType } from "@ui/generated/catalyst/map/TileType";

export const LegacyTileSchema = z.object({
  type: z.number(),
  x: z.number(),
  y: z.number(),
  top: z.number().optional(),
  left: z.number().optional(),
  topLeft: z.number().optional(),
  owner: z.string().optional(),
  name: z.string().optional(),
  estate_id: z.string().optional(),
  price: z.number().optional(),
  rentalPricePerDay: z.string().optional(),
});

export const NftAttributeSchema = z.object({
  trait_type: z.string(),
  value: z.number(),
  display_type: z.string(),
});

export const NftMetadataSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  image: z.string(),
  external_url: z.string().nullable(),
  background_color: z.string(),
  attributes: z.array(NftAttributeSchema),
});

export const RentalPeriodSchema = z.object({
  minDays: z.number(),
  maxDays: z.number(),
  pricePerDay: z.string(),
});

export const TileRentalListingSchema = z.object({
  expiration: z.number(),
  periods: z.array(RentalPeriodSchema),
  updatedAt: z.number(),
});

export const TileTypeSchema = z.enum(["owned", "unowned", "plaza", "road", "district"]);

export const TileSchema = z.object({
  id: z.string(),
  x: z.number(),
  y: z.number(),
  nftId: z.string().optional(),
  type: TileTypeSchema,
  top: z.boolean(),
  left: z.boolean(),
  topLeft: z.boolean(),
  updatedAt: z.number(),
  name: z.string().optional(),
  owner: z.string().optional(),
  estateId: z.string().optional(),
  tokenId: z.string().optional(),
  price: z.number().optional(),
  expiresAt: z.number().optional(),
  rentalListing: TileRentalListingSchema.optional(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertLegacyTile = Assert<Mutual<LegacyTile, z.infer<typeof LegacyTileSchema>>>;
export type _AssertNftAttribute = Assert<Mutual<NftAttribute, z.infer<typeof NftAttributeSchema>>>;
export type _AssertNftMetadata = Assert<Mutual<NftMetadata, z.infer<typeof NftMetadataSchema>>>;
export type _AssertRentalPeriod = Assert<Mutual<RentalPeriod, z.infer<typeof RentalPeriodSchema>>>;
export type _AssertTile = Assert<Mutual<Tile, z.infer<typeof TileSchema>>>;
export type _AssertTileRentalListing = Assert<Mutual<TileRentalListing, z.infer<typeof TileRentalListingSchema>>>;
export type _AssertTileType = Assert<Mutual<TileType, z.infer<typeof TileTypeSchema>>>;
