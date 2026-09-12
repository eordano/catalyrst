import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  GetImagesResponseSchema,
  ImageSchema,
} from "../generated-schemas/camera-reel";
import { z } from "zod";

const CategoriesEnvelopeSchema = z
  .object({
    data: z.object({ categories: z.array(z.string()) }).passthrough(),
  })
  .passthrough();

export const BadgeDataSchema = z
  .object({
    id: z.string(),
    name: z.string(),
    description: z.string().nullish(),
    category: z.string().nullish(),
    isTier: z.boolean().optional(),
    completedAt: z.string().nullish(),
    progress: z
      .object({
        stepsDone: z.number().optional(),
        totalStepsTarget: z.number().optional(),
        lastCompletedTierName: z.string().nullish(),
        lastCompletedTierImage: z.string().nullish(),
      })
      .passthrough()
      .optional(),
  })
  .passthrough();
export type BadgeData = z.infer<typeof BadgeDataSchema>;

const UserBadgesEnvelopeSchema = z
  .object({
    data: z
      .object({
        achieved: z.array(BadgeDataSchema),
        notAchieved: z.array(BadgeDataSchema),
      })
      .passthrough(),
  })
  .passthrough();

export async function fetchBadgeCategories(opts: GetOptions = {}): Promise<string[]> {
  const raw = await getJSON<unknown>("/categories", opts);
  return CategoriesEnvelopeSchema.parse(raw).data.categories;
}

export async function fetchUserBadges(
  address: string,
  opts: GetOptions = {},
): Promise<{ achieved: BadgeData[]; notAchieved: BadgeData[] }> {
  const raw = await getJSON<unknown>(
    `/users/${encodeURIComponent(address.toLowerCase())}/badges`,
    opts,
  );
  const env = UserBadgesEnvelopeSchema.parse(raw);
  return { achieved: env.data.achieved, notAchieved: env.data.notAchieved };
}

type WireImage = z.infer<typeof ImageSchema>;
export type GalleryImage = WireImage & { dateTime: string };

function liftDateTime(img: WireImage): GalleryImage {
  return { ...img, dateTime: img.metadata.dateTime };
}

export async function fetchUserPhotos(
  address: string,
  opts: GetOptions = {},
): Promise<GalleryImage[]> {
  const raw = await getJSON<unknown>(
    `/camera-reel/api/users/${encodeURIComponent(address.toLowerCase())}/images`,
    opts,
  );
  return GetImagesResponseSchema.parse(raw).images.map(liftDateTime);
}