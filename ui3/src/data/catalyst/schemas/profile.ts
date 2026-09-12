
import { z } from "zod";

const Color3Schema = z
  .object({ r: z.number(), g: z.number(), b: z.number() })
  .partial()
  .passthrough();

const nullableStr = z.string().nullish();

const LinkSchema = z.object({
  title: nullableStr,
  url: nullableStr,
});

export type ProfileLinkWire = z.infer<typeof LinkSchema>;

const AvatarInfoSchema = z
  .object({
    wearables: z.array(z.string()).nullish(),
    snapshots: z
      .object({ face256: z.string().optional(), body: z.string().optional() })
      .passthrough()
      .optional(),
  })
  .passthrough();

export type AvatarInfoWire = z.infer<typeof AvatarInfoSchema>;

export const AvatarSchema = z
  .object({
    name: nullableStr,
    hasClaimedName: z.boolean().nullish(),
    nameColor: z.union([Color3Schema, z.string()]).optional(),
    description: nullableStr,
    links: z.array(LinkSchema).optional(),
    country: z.string().optional(),
    gender: z.string().optional(),
    pronouns: z.string().optional(),
    relationshipStatus: z.string().optional(),
    sexualOrientation: z.string().optional(),
    language: z.string().optional(),
    profession: z.string().optional(),
    birthdate: z.number().optional(),
    realName: z.string().optional(),
    hobbies: z.string().optional(),
    ethAddress: z.string().optional(),
    userId: z.string().optional(),
    avatar: AvatarInfoSchema.optional(),
  })
  .passthrough();

export type AvatarWire = z.infer<typeof AvatarSchema>;

export const ProfileEnvelopeSchema = z
  .object({
    avatars: z.array(AvatarSchema),
    timestamp: z.number().optional(),
  })
  .passthrough();

export type ProfileEnvelopeWire = z.infer<typeof ProfileEnvelopeSchema>;

export const CategoriesEnvelopeSchema = z
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
    assets: z.unknown().optional(),
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

export const UserBadgesEnvelopeSchema = z
  .object({
    data: z
      .object({
        achieved: z.array(BadgeDataSchema),
        notAchieved: z.array(BadgeDataSchema),
      })
      .passthrough(),
  })
  .passthrough();

export const GalleryImageSchema = z
  .object({
    id: z.string(),
    url: z.string(),
    thumbnailUrl: z.string(),
    isPublic: z.boolean().optional(),
    dateTime: z.string(),
  })
  .passthrough();

export type GalleryImage = z.infer<typeof GalleryImageSchema>;

export const GalleryEnvelopeSchema = z
  .object({
    images: z.array(GalleryImageSchema),
    userData: z
      .object({
        currentImages: z.number().optional(),
        maxImages: z.number().optional(),
      })
      .passthrough()
      .optional(),
  })
  .passthrough();
