import { getJSON, serviceBase, type RequestOpts } from "./client";
import { field, hasId, isRecord, keepRows } from "./rows";
import {
  CommunityEventSchema,
  CommunityMemberSchema,
  CommunityPlaceSchema,
  CommunityPostSchema,
  CommunitySchema,
} from "./schemas/communities";
import type {
  CommunityEventWire,
  CommunityMemberWire,
  CommunityPlaceWire,
  CommunityPostWire,
  CommunityWire,
} from "./schemas/communities";

export {
  CommunityEventSchema,
  CommunityMemberSchema,
  CommunityPlaceSchema,
  CommunityPostSchema,
  CommunitySchema,
};

const PROD_COMMUNITY_CDN = /^https:\/\/cdn\.decentraland\.org(?=\/social\/communities\/)/;

export function normalizeCommunityThumbnail(v: unknown): string | null {
  if (typeof v !== "string" || v === "" || v === "N/A") return null;
  return v.replace(PROD_COMMUNITY_CDN, serviceBase("communitiesCdn"));
}

export function normalizeCommunity(c: CommunityWire) {
  return {
    ...c,
    ownerName: c.ownerName ?? null,
    thumbnailUrl: normalizeCommunityThumbnail(c.thumbnailUrl),
    visibility: c.visibility ?? null,
    role: c.role ?? null,
  };
}

export type Community = ReturnType<typeof normalizeCommunity>;

export function normalizeCommunityMember(m: CommunityMemberWire) {
  return { ...m, joinedAt: m.joinedAt ?? null };
}

export type CommunityMember = ReturnType<typeof normalizeCommunityMember>;

export function normalizeCommunityEvent(e: CommunityEventWire) {
  return {
    ...e,
    name: e.name ?? null,
    image: e.image ?? null,
    creatorName: e.creatorName ?? null,
    timeLabel: e.timeLabel ?? null,
  };
}

export type CommunityEvent = ReturnType<typeof normalizeCommunityEvent>;

export function normalizeCommunityPost(p: CommunityPostWire) {
  return { ...p, createdAt: p.createdAt ?? null };
}

export type CommunityPost = ReturnType<typeof normalizeCommunityPost>;

export function normalizeCommunityPlace(p: CommunityPlaceWire) {
  return { ...p, addedAt: p.addedAt ?? null };
}

export type CommunityPlace = ReturnType<typeof normalizeCommunityPlace>;

export type CommunityDetail = {
  community: Community;
  members: CommunityMember[];
  events: CommunityEvent[];
  source: string;
};

function hasMemberAddress(row: unknown): boolean {
  return isRecord(row) && typeof row.memberAddress === "string";
}

function projectDetail(
  raw: unknown,
  rawMembers: unknown,
  rawEvents: unknown,
  source: string,
): CommunityDetail | null {
  const community = CommunitySchema.safeParse(raw ?? {});
  if (!community.success) {
    console.warn("[communities] community failed validation:", community.error.message);
    return null;
  }
  if (!hasId(community.data)) {
    console.warn("[communities] community has no id");
    return null;
  }

  return {
    community: normalizeCommunity(community.data),
    members: keepRows(rawMembers, CommunityMemberSchema, hasMemberAddress, normalizeCommunityMember),
    events: keepRows(rawEvents, CommunityEventSchema, hasId, normalizeCommunityEvent),
    source,
  };
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
  return keepRows(
    field(unwrapData(raw), "results"),
    CommunitySchema,
    hasId,
    normalizeCommunity,
  );
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

    return projectDetail(community, members, [], "live");
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
    return keepRows(
      field(unwrapData(raw), "posts"),
      CommunityPostSchema,
      hasId,
      normalizeCommunityPost,
    );
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
    return keepRows(
      field(unwrapData(raw), "results"),
      CommunityPlaceSchema,
      hasId,
      normalizeCommunityPlace,
    );
  } catch {
    return [];
  }
}
