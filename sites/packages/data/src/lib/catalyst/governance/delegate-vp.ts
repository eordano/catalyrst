import { z } from "zod";

import { shortAddress } from "../format/address";
import { EngagementPayloadSchema } from "../generated-schemas/governance";
import {
  governanceApiBase,
  governanceProcessEnv,
  type GovernanceEnv,
} from "./api-base";
import {
  readDelegation,
  resolveDelegateRegistry,
  type DelegateRegistryConfig,
  type DelegateRegistrySetup,
  type DelegationScope,
} from "./delegate-registry";
import { fetchVpDistributions, type VpDistribution } from "./snapshot-vp";
import type { EngagementPayload as RsEngagementPayload } from "@ui/generated/catalyst/governance/EngagementPayload";

export const DEFAULT_SNAPSHOT_SPACE = "snapshot.dcl.eth";
export const SNAPSHOT_SPACE_ENV = "SNAPSHOT_SPACE";

export type Candidate = {
  id: string;
  address: string;
  addressShort: string;
  name: string;
  hue: number;
  vp: number | null;
  vpLabel: string;
  vpDistribution: VpDistribution | null;
  archiveVotes: number;
};

export type CandidateCard = {
  id: string;
  name: string;
  addressShort: string;
  hue: number;
  vpLabel: string;
  activityLabel: string;
};

export type DelegateData = {
  source: "live" | "empty" | "error";
  space: string;
  address: string | null;
  needsWallet: boolean;
  userVp: number | null;
  userVpLabel: string;
  userVpDistribution: VpDistribution | null;
  delegatedTo: string | null;
  delegationScope: DelegationScope | "unknown";
  candidates: Candidate[];
  cards: CandidateCard[];
  registry: DelegateRegistryConfig | null;
  rosterWindowDays: number;
  blockers: string[];
};

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftEngagement = Assert<
  AssignableTo<RsEngagementPayload, z.input<typeof EngagementPayloadSchema>>
>;

type Env = GovernanceEnv;

const processEnv = governanceProcessEnv;

export function snapshotSpace(override?: string, env: Env = processEnv()): string {
  return (override ?? env[SNAPSHOT_SPACE_ENV] ?? DEFAULT_SNAPSHOT_SPACE).trim();
}

const NF = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

export function vpLabel(vp: number | null): string {
  return vp === null ? "\u{2014}" : NF.format(vp);
}

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return Math.abs(h);
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function withTimeout(signal: AbortSignal | undefined, ms: number): AbortSignal | undefined {
  if (ms <= 0 || typeof AbortSignal?.timeout !== "function") return signal;
  const deadline = AbortSignal.timeout(ms);
  if (!signal) return deadline;
  return typeof AbortSignal.any === "function" ? AbortSignal.any([signal, deadline]) : signal;
}

export type RosterEntry = { address: string; votes: number };

export async function fetchDelegateRoster(args: {
  base?: string;
  days: number;
  limit: number;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}): Promise<RosterEntry[]> {
  const doFetch = args.fetchImpl ?? fetch;
  const url = `${governanceApiBase(args.base)}/votes/engagement?days=${args.days}&limit=${args.limit}`;
  const res = await doFetch(url, {
    signal: args.signal,
    cache: "no-store",
    headers: { accept: "application/json" },
  });
  if (!res.ok) throw new Error(`governance engagement returned ${res.status}`);
  const parsed = EngagementPayloadSchema.parse((await res.json()) as unknown);
  return parsed.voters
    .map((v) => ({ address: v.address.trim().toLowerCase(), votes: v.votes }))
    .filter((v) => /^0x[0-9a-f]{40}$/.test(v.address));
}

function toCard(c: Candidate, days: number): CandidateCard {
  return {
    id: c.id,
    name: c.name,
    addressShort: c.addressShort,
    hue: c.hue,
    vpLabel: c.vpLabel,
    activityLabel: `${c.archiveVotes} vote${c.archiveVotes === 1 ? "" : "s"} in ${days}d`,
  };
}

export type LoadDelegateOptions = {
  address?: string | null;
  space?: string;
  base?: string;
  hubUrl?: string;
  setup?: DelegateRegistrySetup;
  days?: number;
  limit?: number;
  timeoutMs?: number;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
};

export async function loadDelegateData(
  options: LoadDelegateOptions = {},
): Promise<DelegateData> {
  const days = options.days ?? 180;
  const limit = options.limit ?? 8;
  const space = snapshotSpace(options.space);
  const setup = options.setup ?? resolveDelegateRegistry();
  const blockers = [...setup.blockers];
  const address = options.address?.trim().toLowerCase() || null;
  const signal = withTimeout(options.signal, options.timeoutMs ?? 8_000);

  let roster: RosterEntry[] = [];
  try {
    roster = await fetchDelegateRoster({
      base: options.base,
      days,
      limit,
      fetchImpl: options.fetchImpl,
      signal,
    });
  } catch (error) {
    blockers.push(`delegate roster unavailable: ${message(error)}`);
  }
  roster = roster.filter((entry) => entry.address !== address);

  const wanted = address ? [address, ...roster.map((r) => r.address)] : roster.map((r) => r.address);
  const [vpSettled, delegationSettled] = await Promise.allSettled([
    wanted.length > 0
      ? fetchVpDistributions({
          space,
          addresses: wanted,
          hubUrl: options.hubUrl,
          fetchImpl: options.fetchImpl,
          signal,
        })
      : Promise.resolve(new Map<string, VpDistribution>()),
    address && setup.config && setup.rpcUrl
      ? readDelegation({
          registry: setup.config,
          rpcUrl: setup.rpcUrl,
          space,
          delegator: address,
          fetchImpl: options.fetchImpl,
          signal,
        })
      : Promise.resolve(null),
  ]);

  let vp = new Map<string, VpDistribution>();
  if (vpSettled.status === "fulfilled") {
    vp = vpSettled.value;
  } else {
    blockers.push(`voting power unavailable: ${message(vpSettled.reason)}`);
  }

  let delegatedTo: string | null = null;
  let delegationScope: DelegationScope | "unknown" = "unknown";
  if (delegationSettled.status === "fulfilled") {
    const state = delegationSettled.value;
    if (state) {
      delegatedTo = state.delegate;
      delegationScope = state.scope;
    }
  } else {
    blockers.push(`current delegation unavailable: ${message(delegationSettled.reason)}`);
  }

  const candidates: Candidate[] = roster.map((entry) => {
    const distribution = vp.get(entry.address) ?? null;
    return {
      id: entry.address,
      address: entry.address,
      addressShort: shortAddress(entry.address),
      name: shortAddress(entry.address),
      hue: hueFrom(entry.address),
      vp: distribution ? distribution.total : null,
      vpLabel: vpLabel(distribution ? distribution.total : null),
      vpDistribution: distribution,
      archiveVotes: entry.votes,
    };
  });

  const userDistribution = address ? (vp.get(address) ?? null) : null;
  const userVp = userDistribution ? userDistribution.total : null;

  return {
    source: candidates.length > 0 ? "live" : blockers.length > 0 ? "error" : "empty",
    space,
    address,
    needsWallet: !address,
    userVp,
    userVpLabel: vpLabel(userVp),
    userVpDistribution: userDistribution,
    delegatedTo,
    delegationScope,
    candidates,
    cards: candidates.map((c) => toCard(c, days)),
    registry: setup.config,
    rosterWindowDays: days,
    blockers,
  };
}

export type DelegateStatus = "confirmed" | "pending";

export type DelegateReceipt = {
  space: string;
  delegate: string;
  vp: number | null;
  txHash: string;
  chainId: number;
  status: DelegateStatus;
  blockNumber: number | null;
};

export type DelegateArgs = {
  space: string;
  delegate: string;
  vp: number | null;
  signal?: AbortSignal;
};

export type DelegateVpFn = (args: DelegateArgs) => Promise<DelegateReceipt>;

export const failClosedDelegate: DelegateVpFn = async () => {
  throw new Error(
    "delegation unavailable: no wallet transaction path is wired for the Snapshot delegate registry",
  );
};
