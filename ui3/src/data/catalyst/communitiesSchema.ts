import { z } from "zod";

import { getJSON, serviceBase, type RequestOpts } from "./client";

const nullableStr = z.string().nullish().transform((v) => v ?? null);

const communityThumbnail = z
  .string()
  .nullish()
  .transform((v) => {
    if (!v || v === "N/A") return null;
    return v.replace(
      /^https:\/\/cdn\.decentraland\.org(?=\/social\/communities\/)/,
      serviceBase("communitiesCdn"),
    );
  });

/**
 * `CommunityMemberWire` (catalyrst-social-service) serializes every one of these,
 * so a member row that is missing one is not a member with a blank name.
 */
export const CommunityMemberSchema = z.object({
  memberAddress: z.string(),
  role: z.string(),
  joinedAt: nullableStr,
  name: z.string(),
  profilePictureUrl: z.string(),
  hasClaimedName: z.boolean(),
});

export type CommunityMember = z.infer<typeof CommunityMemberSchema>;

export const CommunityEventSchema = z.object({
  id: z.string(),
  name: nullableStr,
  image: nullableStr,
  creatorName: nullableStr,
  timeLabel: nullableStr,
});

export type CommunityEvent = z.infer<typeof CommunityEventSchema>;

export const CommunityPostSchema = z.object({
  id: z.string(),
  authorAddress: z.string(),
  content: z.string(),
  createdAt: nullableStr,
  likesCount: z.number(),
  isLikedByUser: z.boolean(),
  authorName: z.string(),
  authorProfilePictureUrl: z.string(),
  authorHasClaimedName: z.boolean(),
});

export type CommunityPost = z.infer<typeof CommunityPostSchema>;

export const CommunityPlaceSchema = z.object({
  id: z.string(),
  addedBy: z.string(),
  addedAt: nullableStr,
});

export type CommunityPlace = z.infer<typeof CommunityPlaceSchema>;

/**
 * `visibility` and `role` describe the *viewer's* relationship to the community
 * and are emitted only on a signed request; null therefore means "nobody asked",
 * which is what an anonymous read actually knows. The rest is emitted always.
 */
export const CommunitySchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  ownerAddress: z.string(),
  ownerName: nullableStr,
  thumbnailUrl: communityThumbnail,
  privacy: z.enum(["public", "private"]),
  visibility: z.enum(["all", "unlisted"]).nullish().transform((v) => v ?? null),
  membersCount: z.number(),
  isLive: z.boolean(),
  role: z.string().nullish().transform((v) => v ?? null),
});

export type Community = z.infer<typeof CommunitySchema>;

export type CommunityDetail = {
  community: Community;
  members: CommunityMember[];
  events: CommunityEvent[];
  source: string;
};

function projectNode(node: unknown, source: string): CommunityDetail | null {
  const obj = (node ?? {}) as Record<string, unknown>;

  const community = CommunitySchema.safeParse(obj);
  if (!community.success) {
    console.warn("[communities] community failed validation:", community.error.message);
    return null;
  }

  return {
    community: community.data,
    members: collectValid(obj.members, CommunityMemberSchema),
    events: collectValid(obj.events, CommunityEventSchema),
    source,
  };
}

/**
 * Per-row, so one malformed member cannot erase the roster: an all-or-nothing
 * array parse turns "one bad row" into "this community has no members".
 */
function collectValid<T>(raw: unknown, schema: z.ZodType<T>): T[] {
  if (!Array.isArray(raw)) return [];
  const out: T[] = [];
  for (const item of raw) {
    const r = schema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  return out;
}

function unwrapData(env: unknown): unknown {
  return (env as { data?: unknown } | null | undefined)?.data ?? env;
}

export async function loadCommunities(
  params: RequestOpts["query"] = {},
  opts: RequestOpts = {},
): Promise<Community[]> {
  const raw = await getJSON("/v1/communities", {
    service: "communities",
    ...opts,
    query: params,
  });
  const container = unwrapData(raw) as { results?: unknown } | null | undefined;
  const results = container?.results;
  if (!Array.isArray(results)) return [];
  return results
    .map((node) => {
      const parsed = CommunitySchema.safeParse(node ?? {});
      return parsed.success ? parsed.data : null;
    })
    .filter((c): c is Community => c !== null);
}

export async function loadCommunity(
  id?: string | null,
  opts: RequestOpts = {},
): Promise<CommunityDetail | null> {
  if (!id) return null;
  try {
    const svcOpts = { service: "communities" as const, ...opts };
    const [cRaw, mRaw] = await Promise.all([
      getJSON(`/v1/communities/${encodeURIComponent(id)}`, svcOpts),
      getJSON(`/v1/communities/${encodeURIComponent(id)}/members`, svcOpts).catch(
        () => null,
      ),
    ]);

    const community = unwrapData(cRaw) as Record<string, unknown>;
    const mData = mRaw ? (unwrapData(mRaw) as { results?: unknown }) : null;
    const members = Array.isArray(mData?.results) ? mData.results : [];

    const node = { ...community, members, events: [] };
    return projectNode(node, "live");
  } catch {
    return null;
  }
}

export async function loadCommunityPosts(
  id?: string | null,
  opts: RequestOpts = {},
): Promise<CommunityPost[]> {
  if (!id) return [];
  try {
    const raw = await getJSON(`/v1/communities/${encodeURIComponent(id)}/posts`, {
      service: "communities",
      ...opts,
    });
    const container = unwrapData(raw) as { posts?: unknown } | null | undefined;
    const posts = Array.isArray(container?.posts) ? container.posts : [];
    return posts
      .map((node) => {
        const parsed = CommunityPostSchema.safeParse(node ?? {});
        return parsed.success ? parsed.data : null;
      })
      .filter((p): p is CommunityPost => p !== null);
  } catch {
    return [];
  }
}

export async function loadCommunityPlaces(
  id?: string | null,
  opts: RequestOpts = {},
): Promise<CommunityPlace[]> {
  if (!id) return [];
  try {
    const raw = await getJSON(`/v1/communities/${encodeURIComponent(id)}/places`, {
      service: "communities",
      ...opts,
    });
    const container = unwrapData(raw) as { results?: unknown } | null | undefined;
    const places = Array.isArray(container?.results) ? container.results : [];
    return places
      .map((node) => {
        const parsed = CommunityPlaceSchema.safeParse(node ?? {});
        return parsed.success ? parsed.data : null;
      })
      .filter((p): p is CommunityPlace => p !== null);
  } catch {
    return [];
  }
}
