import { z } from "zod";

import type { GetOptions } from "../client";
import { loadProposals } from "./index";
import { ETH_ADDRESS_RE } from "../format/address";

export const ACTIVITY_TABS = ["proposals", "watchlist", "coauthoring"] as const;
export type ActivityTab = (typeof ACTIVITY_TABS)[number];

export function toActivityTab(raw: string | null | undefined): ActivityTab {
  const k = (raw ?? "").trim().toLowerCase();
  return (ACTIVITY_TABS as readonly string[]).includes(k)
    ? (k as ActivityTab)
    : "proposals";
}

export function isAddress(raw: string | null | undefined): boolean {
  return !!raw && ETH_ADDRESS_RE.test(raw.trim());
}

const VpSchema = z.object({
  total: z.number(),
  own: z.number(),
  delegated: z.number(),
});

const VpSegmentSchema = z.object({
  id: z.string(),
  label: z.string(),
  value: z.number(),
  tone: z.string(),
});

const VotingStatsSchema = z.object({
  participationTotal: z.number(),
  participationPercentage: z.string(),
  personalMatchPercentage: z.number(),
  outcomeMatchPercentage: z.number(),
});

const BadgeSchema = z.object({
  id: z.union([z.number(), z.string()]),
  label: z.string(),
  hue: z.number(),
});

const ProjectSchema = z.object({
  id: z.string(),
  title: z.string(),
  role: z.string(),
  amount: z.number(),
  token: z.string(),
  passed: z.string(),
  funded: z.number(),
});

const ProposalRowSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.string(),
  category: z.string(),
  votes: z.number().nullish(),
  date: z.string(),
  pending: z.boolean().nullish(),
});

const VotedRowSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.string(),
  category: z.string(),
  choice: z.string(),
  date: z.string(),
  stance: z.string(),
});

const DelegatorSchema = z.object({
  address: z.string(),
  vp: z.number(),
  hue: z.number(),
});

const ProfileSchema = z.object({
  address: z.string(),
  username: z.string(),
  validated: z.boolean(),
  bio: z.string(),
  vp: VpSchema,
  vpDistribution: z.array(VpSegmentSchema),
  votingStats: VotingStatsSchema,
  badges: z.array(BadgeSchema),
  badgesMore: z.number(),
  projects: z.array(ProjectSchema),
  activity: z.object({
    proposals: z.array(ProposalRowSchema),
    watchlist: z.array(ProposalRowSchema),
    coauthoring: z.array(ProposalRowSchema),
  }),
  delegatedTo: DelegatorSchema.nullish(),
  delegators: z.array(DelegatorSchema),
  voted: z.array(VotedRowSchema),
});

export type ProposalRow = z.infer<typeof ProposalRowSchema>;
export type ProfileActivity = z.infer<typeof ProfileSchema>;

export function emptyProfile(address: string): ProfileActivity {
  return {
    address,
    username: "",
    validated: false,
    bio: "",
    vp: { total: 0, own: 0, delegated: 0 },
    vpDistribution: [],
    votingStats: {
      participationTotal: 0,
      participationPercentage: "",
      personalMatchPercentage: 0,
      outcomeMatchPercentage: 0,
    },
    badges: [],
    badgesMore: 0,
    projects: [],
    activity: { proposals: [], watchlist: [], coauthoring: [] },
    delegatedTo: null,
    delegators: [],
    voted: [],
  };
}

export async function loadUserProposals(
  address: string,
  opts: GetOptions = {},
): Promise<{ rows: ProposalRow[]; source: "live" | "empty" }> {
  if (!isAddress(address)) return { rows: [], source: "empty" };

  const result = await loadProposals(opts);
  if (result.fallback) return { rows: [], source: "empty" };

  const addr = address.toLowerCase();
  const rows: ProposalRow[] = result.proposals
    .filter((p) => result.addressById[p.id] === addr)
    .map((p) => ({
      id: p.id,
      title: p.title,
      status: p.status,
      category: p.category,
      votes: p.votes ?? null,
      date: p.time,
    }));

  return { rows, source: "live" };
}

export function rowsForTab(profile: ProfileActivity, tab: ActivityTab): ProposalRow[] {
  if (tab === "watchlist") return profile.activity.watchlist;
  if (tab === "coauthoring") return profile.activity.coauthoring;
  return profile.activity.proposals;
}
