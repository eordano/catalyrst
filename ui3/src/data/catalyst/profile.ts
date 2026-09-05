import { siteUrl } from "../../data/site";
import { getJSON, catalystBase, type RequestOpts } from "./client";
import { field, isRecord, listOf } from "./rows";
import {
  AvatarSchema,
  BadgeDataSchema,
  CategoriesEnvelopeSchema,
  GalleryEnvelopeSchema,
  GalleryImageSchema,
  ProfileEnvelopeSchema,
  UserBadgesEnvelopeSchema,
} from "./schemas/profile";
import type {
  AvatarInfoWire,
  AvatarWire,
  BadgeData,
  GalleryImage,
  ProfileEnvelopeWire,
  ProfileLinkWire,
} from "./schemas/profile";

export { AvatarSchema, BadgeDataSchema, GalleryImageSchema, ProfileEnvelopeSchema };
export type { BadgeData, GalleryImage };

export function normalizeAddress(addr?: string | null): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr?: string | null): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test((addr ?? "").trim());
}

// The wire -> `Avatar` normalization, which the schema module cannot carry: a
// perf build replaces it with an accepting stub, and a stub reproduces no
// transform. Here it runs whether or not validation did, so a profile that has
// never been edited reads as null rather than undefined in both builds.
//
// Every field a schema marks nullish is restated, so `Avatar` is exactly what a
// caller gets and a field added to the schema shows up as its honest
// `| undefined` until it is normalized here too.

export type ProfileLink = { title: string | null; url: string | null };

export type AvatarInfo = AvatarInfoWire & { wearables: string[] | null };

export type Avatar = AvatarWire & {
  name: string | null;
  hasClaimedName: boolean | null;
  description: string | null;
  links?: ProfileLink[];
  avatar?: AvatarInfo;
};

export type ProfileEnvelope = ProfileEnvelopeWire & { avatars: Avatar[] };

function normalizeProfileLink(l: ProfileLinkWire): ProfileLink {
  return { title: l.title ?? null, url: l.url ?? null };
}

function normalizeAvatarInfo(a: AvatarInfoWire): AvatarInfo {
  return { ...a, wearables: a.wearables ?? null };
}

/**
 * `links` and `avatar` are lifted out of the spread and written back only when
 * present: both are optional on the wire, and a schema omits an absent optional
 * key rather than setting it to undefined, so writing one in unconditionally
 * would itself be a difference between the two modes.
 */
export function normalizeAvatar(a: AvatarWire): Avatar {
  const { links, avatar, ...rest } = a;
  const out: Avatar = {
    ...rest,
    name: a.name ?? null,
    hasClaimedName: a.hasClaimedName ?? null,
    description: a.description ?? null,
  };
  if (links) out.links = links.map(normalizeProfileLink);
  if (avatar) out.avatar = normalizeAvatarInfo(avatar);
  return out;
}

// Profile is the one reader family that uses `.parse` rather than `.safeParse`,
// so it drops nothing: a payload the shape rejects throws, and the caller's query
// shows an error. Perf mode does not remove that failure, it MOVES it -- `.parse`
// becomes the identity and the throw reappears a line later as
// `Cannot read properties of undefined (reading 'map')` inside a normalizer, or
// worse, does not appear at all: `fetchUserPhotos` handed back `undefined` typed
// as `GalleryImage[]`, and the gallery crashed on it instead.
//
// So the four places these readers walk into the parsed envelope are made total.
// In the checking build the schema already proved every one of them and none of
// this fires; in perf a malformed envelope now reads as empty, which is what the
// callers already handle, rather than as a TypeError somewhere else.

export function normalizeProfileEnvelope(env: ProfileEnvelopeWire): ProfileEnvelope {
  return {
    ...env,
    avatars: listOf<AvatarWire>(env.avatars).filter(isRecord).map(normalizeAvatar),
  };
}

export function parseProfileEnvelope(raw: unknown): ProfileEnvelope {
  return normalizeProfileEnvelope(ProfileEnvelopeSchema.parse(raw));
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
    name: avatar.name || (addr ? `${addr.slice(0, 5)}\u{2026}${addr.slice(-4)}` : ""),
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

export async function fetchBadgeCategories(opts: RequestOpts = {}): Promise<string[]> {
  const raw = await getJSON("/categories", { service: "badges", ...opts });
  const env = CategoriesEnvelopeSchema.parse(raw);
  return listOf<string>(field(field(env, "data"), "categories"));
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
  const data = field(UserBadgesEnvelopeSchema.parse(raw), "data");
  const badges = (key: string) =>
    listOf<BadgeData>(field(data, key)).filter(isRecord).map(mapBadge);
  return { achieved: badges("achieved"), notAchieved: badges("notAchieved") };
}

export async function fetchUserPhotos(
  address?: string | null,
  opts: RequestOpts = {},
): Promise<GalleryImage[]> {
  const raw = await getJSON(
    `/api/users/${encodeURIComponent(normalizeAddress(address))}/images`,
    { service: "cameraReel", ...opts },
  );
  return listOf<GalleryImage>(field(GalleryEnvelopeSchema.parse(raw), "images"));
}
