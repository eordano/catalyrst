import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  CommunitySchema,
  CommunityMemberSchema,
  unwrapData,
  unwrapResults,
  type Community,
  type CommunityMember,
} from "./communities";

export type CommunityRow = Community & {
  members?: CommunityMember[];
};

export type CommunityJoinDetail = {
  community: Community;
  members: CommunityMember[];
  source: "live" | "fixture";
};

const RowSchema = CommunitySchema.extend({
  members: z.array(CommunityMemberSchema).optional(),
});

function projectRow(node: unknown): CommunityRow | null {
  const parsed = RowSchema.safeParse(node ?? {});
  if (!parsed.success) {
    console.warn("[community-join] row failed validation:", parsed.error.message);
    return null;
  }
  return parsed.data;
}

export function isDisplayableCommunity(c: CommunityRow): boolean {
  const name = (c.name ?? "").trim();
  if (!name) return false;
  if (/[<>]/.test(name)) return false;
  if (name.length > 60) return false;
  if (/^(.)\1{7,}$/.test(name)) return false;
  if (/^(benchcom|warmupcom|benchup|smoke-)/i.test(name)) return false;
  return true;
}

export async function loadCommunities(
  opts: GetOptions & { search?: string } = {},
): Promise<{ rows: CommunityRow[]; total: number; source: "live" | "fixture" }> {
  const { search = "", ...getOpts } = opts;
  const raw = await getJSON<unknown>("/v1/communities", {
    ...getOpts,
    query: { ...getOpts.query, search: search || undefined, limit: 100 },
  });
  const results = unwrapResults(raw);
  if (results === null) {
    throw new Error("/v1/communities did not answer with a results list");
  }
  const rows = results
    .map(projectRow)
    .filter((r): r is CommunityRow => r !== null)
    .filter(isDisplayableCommunity);
  const envTotal = (raw as { data?: { total?: unknown } })?.data?.total;
  const total = typeof envTotal === "number" && envTotal >= rows.length ? envTotal : rows.length;
  return { rows, total, source: "live" };
}

export async function loadCommunityDetail(
  id: string,
  opts: GetOptions = {},
): Promise<CommunityJoinDetail | null> {
  try {
    const [cRaw, mRaw] = await Promise.all([
      getJSON<unknown>(`/v1/communities/${encodeURIComponent(id)}`, opts),
      getJSON<unknown>(
        `/v1/communities/${encodeURIComponent(id)}/members`,
        opts,
      ).catch(() => null),
    ]);
    const community = CommunitySchema.safeParse(unwrapData(cRaw));
    if (!community.success) return null;

    const members = z
      .array(CommunityMemberSchema)
      .safeParse((mRaw ? unwrapResults(mRaw) : null) ?? []);

    return {
      community: community.data,
      members: members.success ? members.data : [],
      source: "live",
    };
  } catch {
    return null;
  }
}

export type JoinAction = "join" | "request";

export function actionFor(community: Pick<Community, "privacy">): JoinAction {
  return community.privacy === "private" ? "request" : "join";
}

export type RequestBody = { kind: "request_to_join" };

export const CommitResultSchema = z.object({
  ok: z.literal(true),
  action: z.enum(["join", "request"]),
  communityId: z.string(),
  role: z.string().nullable(),
  signatureHash: z.string(),
  pending: z.boolean(),
});
export type CommitResult = z.infer<typeof CommitResultSchema>;

export type CommitFn = (args: {
  communityId: string;
  action: JoinAction;
  signal?: AbortSignal;
}) => Promise<CommitResult>;

export const simulateCommit: CommitFn = async ({ communityId, action, signal }) => {
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 450);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
  const pending = action === "request";
  return {
    ok: true,
    action,
    communityId,
    role: pending ? null : "member",
    signatureHash: simSignatureHash(`${action}:${communityId}`),
    pending,
  };
};

function simSignatureHash(seed: string): string {
  let out = "";
  let h = 0x811c9dc5;
  for (let i = 0; out.length < 64; i++) {
    const ch = seed.charCodeAt(i % Math.max(1, seed.length)) ^ (seed.length + i);
    h = Math.imul(h ^ ch, 0x01000193) >>> 0;
    out += h.toString(16).padStart(8, "0");
  }
  return out.slice(0, 64);
}
