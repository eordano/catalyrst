import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { z } from "zod";
import { NameColorSchema } from "../generated-schemas/communities";
import { ETH_ADDRESS_RE } from "../format/address";

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

/** The generated {r,g,b} triple, softened: profile entities may omit channels. */
const Color3Schema = NameColorSchema.partial().passthrough();

/** `title` and `url` are both required on a profile `Link` (`@dcl/schemas`
 *  `platform/profile`), and a link with an empty href is not a link -- it was a
 *  row the parse never read, rendered as one the user could click. */
const LinkSchema = z.object({
  title: z.string(),
  url: z.string(),
});

/** `wearables` is optional rather than defaulted: `passport.server.ts` counts
 *  it to decide whether a profile is empty, and `[]` would answer "this player
 *  wears nothing" for an `avatar` block that simply did not list any. Absent
 *  reaches the caller as `undefined`, which its `?? 0` reads as unknown. */
const AvatarInfoSchema = z
  .object({
    wearables: z.array(z.string()).optional(),
    snapshots: z
      .object({ face256: z.string().optional(), body: z.string().optional() })
      .passthrough()
      .optional(),
  })
  .passthrough();

/**
 * `name`, `description` and `hasClaimedName` are required on every `Avatar`
 * (`@dcl/schemas` `platform/profile`, `required: [... 'hasClaimedName']`).
 * `hasClaimedName` is the one that mattered: defaulted to `false` it told the
 * passport that a NAME holder had never claimed one, from a payload the parse
 * had not actually read.
 */
export const AvatarSchema = z
  .object({
    name: z.string(),
    hasClaimedName: z.boolean(),
    nameColor: z.union([Color3Schema, z.string()]).optional(),
    description: z.string(),
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

/** `avatars` is required on a `Profile`; a body without it is not a profile
 *  answer, and defaulting it to `[]` turned every broken read into "this
 *  wallet has no avatar". */
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
  address: string,
  opts: GetOptions = {},
): Promise<Avatar | null> {
  const raw = await getJSON<unknown>(
    `/lambdas/profile/${encodeURIComponent(normalizeAddress(address))}`,
    opts,
  );
  const env = parseProfileEnvelope(raw);
  return env.avatars[0] ?? null;
}

export type ProfileInfoField = { key: string; label: string; value: string; icon: string };
export type ProfileLink = { title: string; url: string };
export type ProfileVM = {
  address: string;
  name: string;
  hasClaimedName: boolean;
  nameColor: string;
  mutualCount: number;
  bio: string;
  accountUrl: string;
  info: ProfileInfoField[];
  links: ProfileLink[];
  equipped: unknown[];
};

const DEFAULT_NAME_COLOR = "#FF8362";

function color3ToHex(c: { r?: number; g?: number; b?: number }): string {
  const to255 = (n: number | undefined) =>
    Math.max(0, Math.min(255, Math.round((n ?? 0) * 255)));
  const hex = (n: number) => n.toString(16).padStart(2, "0");
  return `#${hex(to255(c.r))}${hex(to255(c.g))}${hex(to255(c.b))}`;
}

const INFO_FIELDS: { src: keyof Avatar; key: string; label: string; icon: string }[] = [
  { src: "country", key: "country", label: "Country", icon: "globe" },
  { src: "language", key: "language", label: "Language", icon: "translate" },
  { src: "pronouns", key: "pronouns", label: "Pronouns", icon: "pronouns" },
  { src: "gender", key: "gender", label: "Gender", icon: "gender" },
  { src: "profession", key: "profession", label: "Profession", icon: "games" },
  { src: "hobbies", key: "favorite_hobby", label: "Favorite hobby", icon: "heart" },
];

export function emptyProfile(address: string): ProfileVM {
  const addr = normalizeAddress(address);
  return {
    address: addr,
    name: addr,
    hasClaimedName: false,
    nameColor: DEFAULT_NAME_COLOR,
    mutualCount: 0,
    bio: "",
    accountUrl: addr
      ? "https://catalyst.example.com/shop"
      : "",
    info: [],
    links: [],
    equipped: [],
  };
}

export function mapProfile(avatar: Avatar, address: string): ProfileVM {
  const nameColor =
    typeof avatar.nameColor === "string"
      ? avatar.nameColor
      : avatar.nameColor
        ? color3ToHex(avatar.nameColor)
        : DEFAULT_NAME_COLOR;

  const info: ProfileInfoField[] = [];
  for (const f of INFO_FIELDS) {
    const value = avatar[f.src];
    if (typeof value === "string" && value.trim()) {
      info.push({ key: f.key, label: f.label, value, icon: f.icon });
    }
  }

  const links: ProfileLink[] = (avatar.links ?? [])
    .filter((l) => /^https?:\/\//i.test(l.url))
    .map((l) => ({ title: l.title || l.url, url: l.url }));

  const addr = normalizeAddress(address) || avatar.ethAddress || address;

  return {
    address: addr,
    name: avatar.name || addr,
    hasClaimedName: Boolean(avatar.hasClaimedName),
    nameColor,
    mutualCount: 0,
    bio: avatar.description ?? "",
    accountUrl: "https://catalyst.example.com/shop",
    info,
    links,
    equipped: [],
  };
}
