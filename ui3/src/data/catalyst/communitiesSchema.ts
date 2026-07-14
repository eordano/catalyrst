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

// The wire -> exported-type normalization, which the schema module cannot
// carry: a perf build replaces it with an accepting stub, and a stub reproduces
// no transform. Here it runs whether or not validation did.
//
// The thumbnail rewrite could never have lived in a shape anyway -- it needs
// `serviceBase`, so the schema module had to reach into the HTTP client to hold
// it. Losing it does not blur a null; it sends play.catalyst.example.com to the PROD CDN for
// an asset catalyst.example.com serves, which is a federation break.

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

// What these readers can USE, as opposed to what their schemas declare. Every
// normalizer above is total -- it spreads the row and fills the nullish fields --
// and no view mapper dereferences a community, so identity is the whole
// requirement: a card with no name renders blank, a card with no id renders
// under a duplicate key and its detail panel can never be opened. `hasId`
// (rows.ts) covers the community, its posts, its events and its places; a member
// is addressed by wallet instead.
//
// The consequence is deliberate and is what the perf-parity gate's
// `communities/loadCommunities+badRow` case records: a node that is not a
// community but does carry an id survives into perf mode as an empty card. That
// is the mode trusting the wire, which is inside its contract; taking the other
// cards down with it would not be.

function hasMemberAddress(row: unknown): boolean {
  return isRecord(row) && typeof row.memberAddress === "string";
}

/**
 * Members and events stay beside the community rather than being merged into it
 * and parsed as one node: a schema hands back only the keys it declares, but a
 * perf build's stub hands back everything it was given, so a reader that
 * over-feeds its schema is asking validation to do the projection for it.
 */
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
    // Per-row, so one malformed member cannot erase the roster: an
    // all-or-nothing array parse turns "one bad row" into "this community has
    // no members".
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
