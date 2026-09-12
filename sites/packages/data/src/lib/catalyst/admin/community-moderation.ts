import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { shortAddress } from "../format/address";
import { warnInvalid } from "../warn";

export const COMMUNITY_STATUSES = ["all", "active", "suspended", "inactive"] as const;
export type CommunityStatus = (typeof COMMUNITY_STATUSES)[number];

export function parseStatus(raw: string | null | undefined): CommunityStatus {
  if (raw && (COMMUNITY_STATUSES as readonly string[]).includes(raw)) {
    return raw as CommunityStatus;
  }
  return "all";
}

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const CommunityRowSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  ownerAddress: z.string(),
  privacy: z.enum(["public", "private"]),
  visibility: z.enum(["all", "unlisted"]),
  active: z.boolean(),
  unlisted: z.boolean(),
  suspended: z.boolean().nullish().transform((v) => v ?? null),
  suspendedAt: nullableStr,
  suspendedBy: nullableStr,
  suspensionReason: nullableStr,
  membersCount: z.number(),
  ownerName: nullableStr,
  thumbnail: nullableStr,
  thumbnailUrl: nullableStr,
  flaggedReason: nullableStr,
});
export type CommunityRow = z.infer<typeof CommunityRowSchema>;

export function hueFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export function truncateAddress(value: string): string {
  return shortAddress(value);
}

export type CommunityModerationStatus =
  | "Active"
  | "Suspended"
  | "Inactive"
  | "Unknown";

export function statusLabel(row: CommunityRow): CommunityModerationStatus {
  if (row.suspended) return "Suspended";
  if (!row.active) return "Inactive";
  return row.suspended === null ? "Unknown" : "Active";
}

export type CommunityModerationCard = {
  id: string;
  name: string;
  owner: string;
  ownerName: string | null;
  privacy: "public" | "private";
  active: boolean;
  suspended: boolean | null;
  membersCount: number;
  thumbnail: string;
  flaggedReason: string;
  status: CommunityModerationStatus;
  hue: number;
};

function normalizeThumbnail(value: string | null): string {
  return value && value !== "N/A" ? value : "";
}

export function toCard(row: CommunityRow): CommunityModerationCard {
  return {
    id: row.id,
    name: row.name,
    owner: row.ownerAddress,
    ownerName: row.ownerName?.trim() || null,
    privacy: row.privacy,
    active: row.active,
    suspended: row.suspended,
    membersCount: row.membersCount,
    thumbnail: normalizeThumbnail(row.thumbnailUrl ?? row.thumbnail),
    flaggedReason: row.flaggedReason ?? "",
    status: statusLabel(row),
    hue: hueFor(row.id),
  };
}

export function matchesStatus(card: CommunityModerationCard, status: CommunityStatus): boolean {
  switch (status) {
    case "active":
      return card.active && card.suspended === false;
    case "suspended":
      return card.suspended === true;
    case "inactive":
      return !card.active;
    case "all":
      return true;
  }
}

function unwrapResults(env: unknown): unknown[] {
  const e = env as { data?: { results?: unknown[] } } | null;
  const results = e?.data?.results;
  return Array.isArray(results) ? results : [];
}

export type ModerationListResult = {
  cards: CommunityModerationCard[];
  source: "live" | "empty" | "error";
};

export async function loadModerationCommunities(
  params: { search?: string; limit?: number; offset?: number } = {},
  opts: GetOptions = {},
): Promise<ModerationListResult> {
  try {
    const env = await getJSON<unknown>("/v1/communities", {
      ...opts,
      query: {
        search: params.search,
        limit: params.limit ?? 50,
        offset: params.offset ?? 0,
      },
    });
    const rows = unwrapResults(env);
    if (rows.length === 0) return { cards: [], source: "empty" };
    const cards: CommunityModerationCard[] = [];
    for (const raw of rows) {
      const r = CommunityRowSchema.safeParse(raw);
      if (r.success) cards.push(toCard(r.data));
      else warnInvalid("CommunityRow", r.error.issues);
    }
    if (cards.length === 0) return { cards: [], source: "error" };
    return { cards, source: "live" };
  } catch {
    return { cards: [], source: "error" };
  }
}

export const COMMUNITY_DECISIONS = ["suspend", "unsuspend"] as const;
export type CommunityDecision = (typeof COMMUNITY_DECISIONS)[number];

export const SuspendResultSchema = z.object({
  ok: z.boolean(),
  id: z.string(),
  suspended: z.boolean(),
});
export type SuspendResult = z.infer<typeof SuspendResultSchema>;

const ActionErrorSchema = z.object({ error: z.string() });

export const SUSPENSION_ACTION_PATH = "/admin/community-suspension";

export async function requestSuspension(
  args: { communityId: string; decision: CommunityDecision; reason?: string },
  opts: { signal?: AbortSignal; actionPath?: string } = {},
): Promise<SuspendResult> {
  const { communityId, decision, reason } = args;
  if (!communityId) throw new Error("community id is required");

  const res = await fetch(opts.actionPath ?? SUSPENSION_ACTION_PATH, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      communityId,
      decision,
      reason: reason?.trim() || undefined,
    }),
    signal: opts.signal,
  });

  let json: unknown = null;
  try {
    json = await res.json();
  } catch {
  }

  if (!res.ok) {
    const parsed = ActionErrorSchema.safeParse(json);
    const msg = parsed.success
      ? parsed.data.error
      : `Moderation request failed (HTTP ${res.status}).`;
    throw new Error(msg);
  }

  const parsed = SuspendResultSchema.safeParse(json);
  if (!parsed.success) throw new Error("Moderation response was not understood.");
  return parsed.data;
}
