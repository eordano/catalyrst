import { z } from "zod";

import staticConfig from "./submit-draft.data.json";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";
import { ProposalRowSchema as RsProposalRowSchema } from "../generated-schemas/governance";

export { governanceApiBase };

export type LinkedPoll = {
  id: string;
  title: string;
  status: string;
  startAt: string;
  finishAt: string;
  user: string;
  label: string;
};

export type DraftFieldLimits = {
  titleMin: number;
  titleMax: number;
  summaryMax: number;
  bodyMax: number;
  bodyMin: number;
  coauthorsMax: number;
};

export type DraftAccount = {
  address: string;
  short: string;
  vp: number;
};

export type DraftSource = "live" | "empty";

export type DraftSubmitData = {
  source: DraftSource;
  pollsFallback: boolean;
  vpThreshold: number;
  vpUnit: string;
  account: DraftAccount;
  limits: DraftFieldLimits;
  bodies: string[];
  linkedPolls: LinkedPoll[];
};

const StaticConfigSchema = z.object({
  vpThreshold: z.number(),
  vpUnit: z.string(),
  field_limits: z.object({
    title_min: z.number(),
    title_max: z.number(),
    summary_max: z.number(),
    body_max: z.number(),
    body_min: z.number(),
    coauthors_max: z.number(),
  }),
  bodies: z.array(z.string()),
});

const STATIC = StaticConfigSchema.parse(staticConfig);

export const DRAFT_LIMITS: DraftFieldLimits = {
  titleMin: STATIC.field_limits.title_min,
  titleMax: STATIC.field_limits.title_max,
  summaryMax: STATIC.field_limits.summary_max,
  bodyMax: STATIC.field_limits.body_max,
  bodyMin: STATIC.field_limits.body_min,
  coauthorsMax: STATIC.field_limits.coauthors_max,
};

export const DRAFT_BODIES: string[] = STATIC.bodies;
export const VP_THRESHOLD: number = STATIC.vpThreshold;
export const VP_UNIT: string = STATIC.vpUnit;

const ProposalSchema = RsProposalRowSchema.pick({
  id: true,
  title: true,
  status: true,
  type: true,
  start_at: true,
  finish_at: true,
  user: true,
});

const ProposalsResponseSchema = z.object({
  data: z.array(ProposalSchema),
});

type UpstreamPoll = z.infer<typeof ProposalSchema>;

function projectPoll(p: UpstreamPoll): LinkedPoll {
  return {
    id: p.id,
    title: p.title,
    status: p.status,
    startAt: p.start_at,
    finishAt: p.finish_at,
    user: p.user,
    label: `[DAO:Poll] ${p.title}`,
  };
}

export type LoadDraftOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  limit?: number;
  scan?: number;
};

export type LinkedPollsResult = {
  ok: boolean;
  polls: LinkedPoll[];
};

export async function loadLinkedPolls(
  opts: LoadDraftOptions = {},
): Promise<LinkedPollsResult> {
  const base = governanceApiBase(opts.base);
  const limit = opts.limit ?? 6;
  const scan = opts.scan ?? 200;
  const url = `${base}/proposals?type=poll&limit=${scan}&offset=0`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (!res.ok) return { ok: false, polls: [] };
    const raw = (await res.json()) as unknown;
    const parsed = ProposalsResponseSchema.safeParse(raw);
    if (!parsed.success) return { ok: false, polls: [] };

    const polls = parsed.data.data.filter((p) => p.type === "poll");
    const passed = polls.filter((p) => p.status === "passed");
    const pool = passed.length > 0 ? passed : polls;
    const projected = pool.slice(0, limit).map(projectPoll);
    if (projected.length === 0) return { ok: false, polls: [] };

    return { ok: true, polls: projected };
  } catch {
    return { ok: false, polls: [] };
  }
}

export function buildDraftData(args: {
  polls: LinkedPoll[];
  pollsOk: boolean;
  account: DraftAccount;
}): DraftSubmitData {
  return {
    source: args.pollsOk ? "live" : "empty",
    pollsFallback: !args.pollsOk,
    vpThreshold: VP_THRESHOLD,
    vpUnit: VP_UNIT,
    account: args.account,
    limits: DRAFT_LIMITS,
    bodies: DRAFT_BODIES,
    linkedPolls: args.polls,
  };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalsEnvelope = Assert<
  AssignableTo<
    RsListEnvelope,
    Omit<z.input<typeof ProposalsResponseSchema>, "data"> & {
      data: RsListEnvelope["data"];
    }
  >
>;
