import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions, Query } from "../client";
import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";

export type BidCard = {
  id: string;
  title: string;
  teamName: string;
  budget: number;
  leadingVP: number;
  leadingChoice: string;
  status: string;
  current: boolean;
};

export type Tender = {
  id: string;
  title: string;
  status: string;
  start: string;
  finish: string;
};

export type BidVoteContext = {
  bidId: string;
  tender: Tender;
  field: BidCard[];
  account: { address: string; label: string; votingPower: number };
  maxErrorsBeforeRedirect: number;
  snapshotSpace: string;
  live: boolean;
  copy: BidVoteCopy;
};

export type BidVoteCopy = {
  title: string;
  description_line1: string;
  description_line2: string;
  action: string;
  retry: string;
  voting_failed: string;
  snapshot_not_available: string;
  snapshot_description: string;
  snapshot_suggestion: string;
  snapshot_button: string;
};

const COPY: BidVoteCopy = {
  title: "There's still more reviewing to do",
  description_line1: "The best way to vote is from consciousness.",
  description_line2: "Please, make sure you go through all {amount} proposals.",
  action: 'Vote "{vote}" anyway',
  retry: "Retry in {timer}...",
  voting_failed: "Failed to cast vote",
  snapshot_not_available: "Voting is not available",
  snapshot_description: "You can still cast your vote directly on Snapshot.",
  snapshot_suggestion:
    "If you think this is a mistake, please reach out to the DAO committee.",
  snapshot_button: "Vote on Snapshot",
};

const DEFAULT_SNAPSHOT_SPACE = "snapshot.dcl.eth";
const MAX_ERRORS_BEFORE_REDIRECT = 2;

const DEFAULT_BID_ID = "4e5dd188-3c30-4ac7-b182-c7b0dc7de002";

const GUEST_ACCOUNT = { address: "", label: "Guest", votingPower: 0 };

export function defaultBidId(): string {
  return DEFAULT_BID_ID;
}

const ConfigurationSchema = z
  .object({
    funding: z.union([z.number(), z.string()]).nullish(),
    teamName: z.string().nullish(),
    linked_proposal_id: z.string().nullish(),
    projectDuration: z.union([z.number(), z.string()]).nullish(),
  })
  .passthrough()
  .nullish();

const ProposalSchema = z.object({
  id: z.string(),
  title: z.string().nullish(),
  type: z.string().nullish(),
  status: z.string().nullish(),
  user: z.string().nullish(),
  start_at: z.string().nullish(),
  finish_at: z.string().nullish(),
  snapshot_space: z.string().nullish(),
  configuration: ConfigurationSchema,
});

export type LiveBidProposal = z.infer<typeof ProposalSchema>;

const EnvelopeSchema = z.object({
  data: z.array(z.unknown()).nullish(),
});

function toNumber(v: unknown): number {
  const n = typeof v === "string" ? Number(v) : typeof v === "number" ? v : NaN;
  return Number.isFinite(n) ? n : 0;
}

function toCard(p: LiveBidProposal, current: boolean): BidCard {
  return {
    id: p.id,
    title: p.title ?? p.id,
    teamName: p.configuration?.teamName ?? "",
    budget: toNumber(p.configuration?.funding),
    leadingVP: 0,
    leadingChoice: "\u{2014}",
    status: p.status ?? "active",
    current,
  };
}

function toTender(
  raw: LiveBidProposal | null,
  tenderId: string | null,
  fallback: LiveBidProposal,
): Tender {
  if (raw) {
    return {
      id: raw.id,
      title: raw.title ?? "Tender",
      status: raw.status ?? "",
      start: raw.start_at ?? "",
      finish: raw.finish_at ?? "",
    };
  }
  return {
    id: tenderId ?? fallback.id,
    title: "Tender",
    status: "",
    start: "",
    finish: "",
  };
}

export type FetchOptions = GetOptions;

async function fetchProposals(
  query: Query,
  base: string,
  opts: GetOptions,
): Promise<LiveBidProposal[] | null> {
  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/proposals", {
      base,
      query,
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
    });
  } catch {
    return null;
  }
  const env = EnvelopeSchema.safeParse(raw);
  if (!env.success) return null;
  const out: LiveBidProposal[] = [];
  for (const row of env.data.data ?? []) {
    const parsed = ProposalSchema.safeParse(row);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

async function fetchProposalById(
  id: string,
  base: string,
  opts: GetOptions,
): Promise<LiveBidProposal | null> {
  const rows = await fetchProposals({ id, limit: 1 }, base, opts);
  return rows?.find((p) => p.id === id) ?? null;
}

export async function loadBidVoteContext(
  bidId: string,
  opts: FetchOptions = {},
): Promise<BidVoteContext | null> {
  const base = governanceApiBase(opts.base);

  const current = await fetchProposalById(bidId, base, opts);
  if (!current) return null;

  const tenderId = current.configuration?.linked_proposal_id ?? null;

  const [tenderRaw, fieldRaw] = await Promise.all([
    tenderId
      ? fetchProposalById(tenderId, base, opts)
      : Promise.resolve<LiveBidProposal | null>(null),
    tenderId
      ? fetchProposals(
          { type: "bid", linked_proposal_id: tenderId, limit: 50 },
          base,
          opts,
        )
      : Promise.resolve<LiveBidProposal[]>([]),
  ]);

  if (fieldRaw === null) return null;

  const source = fieldRaw.length > 0 ? fieldRaw : [current];
  const field = source.map((p) => toCard(p, p.id === bidId));
  if (!field.some((c) => c.current)) {
    field.unshift(toCard(current, true));
  }

  return {
    bidId,
    tender: toTender(tenderRaw, tenderId, current),
    field,
    account: GUEST_ACCOUNT,
    maxErrorsBeforeRedirect: MAX_ERRORS_BEFORE_REDIRECT,
    snapshotSpace:
      tenderRaw?.snapshot_space ?? current.snapshot_space ?? DEFAULT_SNAPSHOT_SPACE,
    live: true,
    copy: COPY,
  };
}

const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalsEnvelope = Assert<
  AssignableTo<RsListEnvelope, z.input<typeof EnvelopeSchema>>
>;
