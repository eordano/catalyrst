// GENERATED from catalyrst/ui3/src/generated/catalyst/places by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { ApiData } from "@ui/generated/catalyst/places/ApiData";
import type { ApiDataTotal } from "@ui/generated/catalyst/places/ApiDataTotal";
import type { CategoryOut } from "@ui/generated/catalyst/places/CategoryOut";
import type { I18n } from "@ui/generated/catalyst/places/I18n";
import type { PlaceRow } from "@ui/generated/catalyst/places/PlaceRow";

export const ApiDataSchema = <T extends z.ZodType>(t: T) =>
  z.object({
    ok: z.boolean(),
    data: t,
  });

export const ApiDataTotalSchema = <T extends z.ZodType>(t: T) =>
  z.object({
    ok: z.boolean(),
    data: z.array(t),
    total: z.number(),
  });

export const I18nSchema = z.object({
  en: z.string().nullable(),
});

export const CategoryOutSchema = z.object({
  name: z.string(),
  active: z.boolean(),
  count: z.number(),
  i18n: I18nSchema,
});

export const PlaceRowSchema = z.object({
  id: z.string(),
  title: z.string().nullable(),
  description: z.string().nullable(),
  image: z.string().nullable(),
  owner: z.string().nullable(),
  positions: z.array(z.string()),
  base_position: z.string(),
  contact_name: z.string().nullable(),
  contact_email: z.string().nullable(),
  content_rating: z.string().nullable(),
  disabled: z.boolean(),
  disabled_at: z.string().nullable(),
  disabled_reason: z.string().nullable(),
  created_at: z.string().nullable(),
  updated_at: z.string().nullable(),
  favorites: z.number(),
  likes: z.number(),
  dislikes: z.number(),
  categories: z.array(z.string()),
  tags: z.array(z.string()),
  highlighted: z.boolean(),
  highlighted_image: z.string().nullable(),
  ranking: z.number().nullable(),
  sdk: z.string().nullable(),
  creator_address: z.string().nullable(),
  world_id: z.string().nullable(),
  deployment_id: z.string().nullable(),
  deployed_at: z.string().nullable(),
  world: z.boolean(),
  world_name: z.string().nullable(),
  is_private: z.boolean(),
  user_favorite: z.boolean(),
  user_like: z.boolean(),
  user_dislike: z.boolean(),
  user_count: z.number().nullable(),
  user_visits: z.number(),
  like_rate: z.number().nullable(),
  like_score: z.number().nullable(),
  live: z.boolean().optional(),
  connected_addresses: z.array(z.string()).optional(),
  realms_detail: z.array(z.record(z.string(), z.unknown())).optional(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertApiData = Assert<Mutual<ApiData<unknown>, z.infer<ReturnType<typeof ApiDataSchema<z.ZodUnknown>>>>>;
export type _AssertApiDataTotal = Assert<Mutual<ApiDataTotal<unknown>, z.infer<ReturnType<typeof ApiDataTotalSchema<z.ZodUnknown>>>>>;
export type _AssertCategoryOut = Assert<Mutual<CategoryOut, z.infer<typeof CategoryOutSchema>>>;
export type _AssertI18n = Assert<Mutual<I18n, z.infer<typeof I18nSchema>>>;
export type _AssertPlaceRow = Assert<Mutual<PlaceRow, z.infer<typeof PlaceRowSchema>>>;
