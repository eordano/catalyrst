import { siteUrl } from "../../data/site";
import { z } from "zod";
import { getJSON, catalystBase, type RequestOpts } from "./client";

export function normalizeAddress(addr?: string | null): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr?: string | null): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test((addr ?? "").trim());
}

const Color3Schema = z
  .object({ r: z.number(), g: z.number(), b: z.number() })
  .partial()
  .passthrough();

const nullableStr = z
  .string()
  .nullish()
  .transform((v) => v ?? null);

const LinkSchema = z.object({
  title: nullableStr,
  url: nullableStr,
});

const AvatarInfoSchema = z
  .object({
    wearables: z.array(z.string()).nullish().transform((v) => v ?? null),
    snapshots: z
      .object({ face256: z.string().optional(), body: z.string().optional() })
      .passthrough()
      .optional(),
  })
  .passthrough();

export const AvatarSchema = z
  .object({
    name: nullableStr,
    hasClaimedName: z.boolean().nullish().transform((v) => v ?? null),
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

export type Avatar = z.infer<typeof AvatarSchema>;

export const ProfileEnvelopeSchema = z
  .object({
    avatars: z.array(AvatarSchema),
    timestamp: z.number().optional(),
  })
  .passthrough();

export type ProfileEnvelope = z.infer<typeof ProfileEnvelopeSchema>;

export function parseProfileEnvelope(raw: unknown): ProfileEnvelope {
  return ProfileEnvelopeSchema.parse(raw);
}

export async function fetchProfile(
  address?: string | null,
  opts: RequestOpts = {},
): Promise<Avatar | null> {
  const raw = await getJSON(
    `/lambdas/profile/${encodeURIComponent(normalizeAddress(address))}`,
    opts,
  );
  const env = parseProfileEnvelope(raw);
  return env.avatars[0] ?? null;
}

const DEFAULT_NAME_COLOR = "#FF8362";

function color3ToHex(c: { r?: number; g?: number; b?: number }): string {
  const to255 = (n?: number) => Math.max(0, Math.min(255, Math.round((n ?? 0) * 255)));
  const hex = (n: number) => n.toString(16).padStart(2, "0");
  return `#${hex(to255(c.r))}${hex(to255(c.g))}${hex(to255(c.b))}`;
}

type InfoField = { src: keyof Avatar; key: string; label: string; icon: string };

const INFO_FIELDS: InfoField[] = [
  { src: "country", key: "country", label: "Country", icon: "globe" },
  { src: "language", key: "language", label: "Language", icon: "translate" },
  { src: "pronouns", key: "pronouns", label: "Pronouns", icon: "pronouns" },
  { src: "gender", key: "gender", label: "Gender", icon: "gender" },
  { src: "profession", key: "profession", label: "Profession", icon: "games" },
  { src: "hobbies", key: "favorite_hobby", label: "Favorite hobby", icon: "heart" },
];

export function mapProfile(avatar: Avatar, address?: string | null) {
  const nameColor =
    typeof avatar.nameColor === "string"
      ? avatar.nameColor
      : avatar.nameColor
        ? color3ToHex(avatar.nameColor)
        : DEFAULT_NAME_COLOR;

  const info: Array<{ key: string; label: string; value: string; icon: string }> = [];
  for (const f of INFO_FIELDS) {
    const value = avatar[f.src];
    if (typeof value === "string" && value.trim()) {
      info.push({ key: f.key, label: f.label, value, icon: f.icon });
    }
  }

  const links = (avatar.links ?? [])
    .flatMap((l) => (l.url && /^https?:\/\//i.test(l.url) ? [{ title: l.title || l.url, url: l.url }] : []));

  const addr = normalizeAddress(address) || avatar.ethAddress || address || "";
  const shortTag = addr ? `#${addr.slice(-4)}` : "";

  return {
    address: addr,
    name: avatar.name || (addr ? `${addr.slice(0, 5)}…${addr.slice(-4)}` : ""),
    tag: shortTag,
    hasClaimedName: Boolean(avatar.hasClaimedName),
    nameColor,
    mutualCount: 0,
    bio: avatar.description ?? "",
    accountUrl: siteUrl("/shop"),
    info,
    links,
    equipped: avatar.avatar?.wearables ?? [],
  };
}

export function profileFaceUrl(
  avatar: Avatar | null | undefined,
  opts: RequestOpts = {},
): string | null {
  const snap = avatar?.avatar?.snapshots?.face256;
  if (!snap || typeof snap !== "string") return null;
  if (/^https?:\/\//i.test(snap) || snap.startsWith("data:")) return snap;
  return `${catalystBase(opts.base)}/content/contents/${snap}`;
}

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

export async function fetchBadgeCategories(opts: RequestOpts = {}): Promise<string[]> {
  const raw = await getJSON("/categories", { service: "badges", ...opts });
  return CategoriesEnvelopeSchema.parse(raw).data.categories;
}

function assetColor(assets: unknown): string | null {
  if (!assets || typeof assets !== "object") return null;
  const two = (assets as Record<string, unknown>)["2d"];
  if (!two || typeof two !== "object") return null;
  const rec = two as Record<string, unknown>;
  const flat = rec.normal ?? rec.baseColor;
  return typeof flat === "string" && flat.trim() ? flat : null;
}

export function badgeImage(b: BadgeData | null | undefined): string | null {
  const tier = b?.progress?.lastCompletedTierImage;
  if (typeof tier === "string" && tier.trim()) return tier;
  return assetColor(b?.assets);
}

export function mapBadge(b: BadgeData) {
  return {
    id: b.id,
    name: b.name,
    description: b.description ?? "",
    category: b.category ?? null,
    tier: b.progress?.lastCompletedTierName ?? null,
    image: badgeImage(b),
    completedAt: b.completedAt ?? null,
  };
}

export async function fetchUserBadges(address?: string | null, opts: RequestOpts = {}) {
  const raw = await getJSON(
    `/users/${encodeURIComponent(normalizeAddress(address))}/badges`,
    { service: "badges", ...opts },
  );
  const env = UserBadgesEnvelopeSchema.parse(raw);
  return {
    achieved: env.data.achieved.map(mapBadge),
    notAchieved: env.data.notAchieved.map(mapBadge),
  };
}

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

const GalleryEnvelopeSchema = z
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

export async function fetchUserPhotos(
  address?: string | null,
  opts: RequestOpts = {},
): Promise<GalleryImage[]> {
  const raw = await getJSON(
    `/api/users/${encodeURIComponent(normalizeAddress(address))}/images`,
    { service: "cameraReel", ...opts },
  );
  return GalleryEnvelopeSchema.parse(raw).images;
}
