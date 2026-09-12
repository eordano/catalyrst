import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { dataOf } from "../envelope";
import { CommunityMemberWireSchema } from "../generated-schemas/communities";

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const CommunityMemberSchema = CommunityMemberWireSchema;
export type CommunityMember = z.infer<typeof CommunityMemberSchema>;

export const CommunityEventSchema = z.object({
  id: z.string(),
  name: z.string(),
  image: nullableStr,
  creatorName: nullableStr,
  timeLabel: nullableStr,
});
export type CommunityEvent = z.infer<typeof CommunityEventSchema>;

export const CommunitySchema = z.object({
  id: z.string(),
  name: z.string(),
  description: nullableStr,
  ownerAddress: z.string(),
  ownerName: nullableStr,
  thumbnailUrl: nullableStr,
  privacy: z.enum(["public", "private"]),
  visibility: z.enum(["all", "unlisted"]).nullish().transform((v) => v ?? null),
  membersCount: z.number().nullish().transform((v) => v ?? null),
  isLive: z.boolean().nullish().transform((v) => v ?? false),
  role: z.string().nullish().transform((v) => v ?? null),
});
export type Community = z.infer<typeof CommunitySchema>;

export type CommunityDetail = {
  community: Community;
  members: CommunityMember[];
  events: CommunityEvent[];
  source: "live" | "fixture";
};

function projectNode(node: unknown, source: "live" | "fixture"): CommunityDetail | null {
  const obj = (node ?? {}) as Record<string, unknown>;

  const community = CommunitySchema.safeParse(obj);
  if (!community.success) {
    console.warn("[communities] community failed validation:", community.error.message);
    return null;
  }

  const members = z
    .array(CommunityMemberSchema)
    .safeParse(Array.isArray(obj.members) ? obj.members : []);
  const events = z
    .array(CommunityEventSchema)
    .safeParse(Array.isArray(obj.events) ? obj.events : []);

  return {
    community: community.data,
    members: members.success ? members.data : [],
    events: events.success ? events.data : [],
    source,
  };
}

const DataEnvelope = dataOf(z.unknown());
export function unwrapData(env: unknown): unknown {
  const parsed = DataEnvelope.safeParse(env);
  return parsed.success ? parsed.data.data : env;
}

const ResultsShape = z.object({ results: z.array(z.unknown()) });
export function unwrapResults(env: unknown): unknown[] | null {
  const parsed = ResultsShape.safeParse(unwrapData(env));
  return parsed.success ? parsed.data.results : null;
}

const RecordShape = z.record(z.string(), z.unknown());

export async function loadCommunity(
  id: string,
  opts: GetOptions = {},
): Promise<CommunityDetail | null> {
  try {
    const [cRaw, mRaw] = await Promise.all([
      getJSON<unknown>(`/v1/communities/${encodeURIComponent(id)}`, opts),
      getJSON<unknown>(
        `/v1/communities/${encodeURIComponent(id)}/members`,
        opts,
      ).catch(() => null),
    ]);

    const community = RecordShape.safeParse(unwrapData(cRaw));
    const members = (mRaw ? unwrapResults(mRaw) : null) ?? [];

    const node = {
      ...(community.success ? community.data : {}),
      members,
      events: [] as unknown[],
    };
    return projectNode(node, "live");
  } catch {
    return null;
  }
}

export async function loadDefaultCommunity(
  opts: GetOptions = {},
): Promise<Community | null> {
  try {
    const raw = await getJSON<unknown>("/v1/communities", {
      ...opts,
      query: { ...opts.query, limit: 60 },
    });
    const rows: Community[] = [];
    for (const n of unwrapResults(raw) ?? []) {
      const parsed = CommunitySchema.safeParse(n);
      if (parsed.success && parsed.data.name.trim() !== "") {
        rows.push(parsed.data);
      }
    }
    if (rows.length === 0) return null;
    return rows.reduce((best, c) =>
      (c.membersCount ?? -1) > (best.membersCount ?? -1) ? c : best,
    );
  } catch {
    return null;
  }
}

