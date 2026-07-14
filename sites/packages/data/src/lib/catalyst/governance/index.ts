import { z } from "zod";

import governanceFixture from "../../../fixtures/governance.json";
import { getJSON, catalystBase, CatalystError } from "../client";
import type { GetOptions } from "../client";
import type { ProjectRow as RsProjectRow } from "@ui/generated/catalyst/governance/ProjectRow";
import type { ProjectsEnvelope as RsProjectsEnvelope } from "@ui/generated/catalyst/governance/ProjectsEnvelope";
import { shortAddress, ETH_ADDRESS_RE } from "../format/address";
import {
  ProjectRowSchema,
  ProposalRowSchema,
  ProposalsEnvelopeSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";
import { newestTimestamp } from "./freshness";

export type ProposalCard = {
  id: string;
  title: string;
  category: string;
  status: string;
  author: string;
  hue: number;
  forPct?: number;
  againstPct?: number;
  passing: boolean;
  votes?: number;
  comments: number;
  time: string;
  urgent?: boolean;
  requiredToPass?: number;
};

export type ProposalDetail = {
  id: string;
  type: string;
  toneClass: string;
  catLabel: string;
  catTone: string;
  status: string;
  statusLabel: string;
  statusTone: string;
  title: string;
  author: string;
  authorHue: number;
  published: string;
  start: string;
  finish: string;
  snapshot: string;
  threshold: string;
  thresholdReached: boolean;
  yourVp: string;
  budget?: { size: string; beneficiary: string; tier: string };
  description?: string;
};

type FixtureProposal = ProposalCard & {
  authorLabel: string;
  detail: Omit<
    ProposalDetail,
    "id" | "title" | "author" | "authorHue" | "status"
  > & { budget?: { size: string; beneficiary: string; tier: string } };
};

type GovernanceFixture = {
  metrics: Record<string, string>;
  proposals: FixtureProposal[];
};

const FIXTURE = governanceFixture as unknown as GovernanceFixture;

export function getProposalAuthorAddress(id: string): string | null {
  const p = FIXTURE.proposals.find((x) => x.id === id);
  return p ? p.author : null;
}

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return Math.abs(h);
}

function titleCase(s: string): string {
  if (!s) return "\u{2014}";
  return s
    .split(/[_\s]+/)
    .map((w) => (w ? w.charAt(0).toUpperCase() + w.slice(1) : ""))
    .join(" ");
}

const CATEGORY_META: Record<string, { label: string; tone: string }> = {
  poi: { label: "Point of Interest", tone: "green" },
  catalyst: { label: "Catalyst Node", tone: "blue" },
  ban_name: { label: "Name Ban", tone: "fuchsia" },
  grant: { label: "Grant Request", tone: "purple" },
  linked_wearables: { label: "Linked Wearables Registry", tone: "yellow" },
  hiring: { label: "Hiring", tone: "green" },
  council_decision_veto: { label: "Council Decision Veto", tone: "red" },
  poll: { label: "Poll", tone: "orange" },
  draft: { label: "Draft", tone: "orange" },
  governance: { label: "Governance", tone: "orange" },
  pitch: { label: "Pitch", tone: "red" },
  tender: { label: "Tender", tone: "red" },
  bid: { label: "Bid", tone: "red" },
};

const STATUS_META: Record<string, { label: string; tone: string }> = {
  active: { label: "Active", tone: "neutral" },
  passed: { label: "Passed", tone: "green" },
  enacted: { label: "Enacted", tone: "green" },
  rejected: { label: "Rejected", tone: "red" },
  out_of_budget: { label: "Out of Budget", tone: "yellow" },
  finished: { label: "Finished", tone: "neutral" },
  pending: { label: "Pending", tone: "neutral" },
};

function toIso(v: string | number | null | undefined): string | null {
  if (v == null) return null;
  if (typeof v === "number") return new Date(v).toISOString();
  return v;
}

function relPhrase(iso: string | null, now: number): { past: boolean; phrase: string } | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return null;
  const diff = t - now;
  const past = diff < 0;
  const days = Math.max(1, Math.round(Math.abs(diff) / 86_400_000));
  let phrase: string;
  if (days >= 365) {
    const y = Math.round(days / 365);
    phrase = `${y} year${y === 1 ? "" : "s"}`;
  } else if (days >= 30) {
    const m = Math.round(days / 30);
    phrase = `${m} month${m === 1 ? "" : "s"}`;
  } else if (days >= 7) {
    const w = Math.round(days / 7);
    phrase = `${w} week${w === 1 ? "" : "s"}`;
  } else {
    phrase = `${days} day${days === 1 ? "" : "s"}`;
  }
  return { past, phrase };
}

function proposalTime(
  status: string,
  finishIso: string | null,
  createdIso: string | null,
  now: number,
): string {
  const r = relPhrase(finishIso, now) ?? relPhrase(createdIso, now);
  if (!r) return "\u{2014}";
  if (!r.past && status === "active") return `Ends in ${r.phrase}`;
  const verb =
    status === "enacted"
      ? "Enacted"
      : status === "passed"
        ? "Passed"
        : status === "out_of_budget"
          ? "Finished"
          : status === "finished"
            ? "Finished"
            : "Ended";
  return `${verb} ${r.phrase} ago`;
}

type LiveProposal = z.infer<typeof ProposalRowSchema>;

const PROPOSAL_PAGE = 200;
const PROPOSAL_CAP = 3000;

function mapLiveProposal(live: LiveProposal, now: number): ProposalCard {
  const category = (live.type ?? "").toLowerCase();
  const status = (live.status ?? "").toLowerCase();
  const finishMs = live.finish_at ? Date.parse(live.finish_at) : NaN;
  const urgent =
    status === "active" &&
    Number.isFinite(finishMs) &&
    finishMs > now &&
    finishMs - now < 48 * 3_600_000;
  return {
    id: live.id,
    title: live.title,
    category,
    status,
    author: shortAddr(live.user),
    hue: hueFrom(live.user ?? live.id),
    forPct: undefined,
    againstPct: undefined,
    passing: false,
    votes: undefined,
    comments: 0,
    time: proposalTime(status, live.finish_at ?? null, toIso(live.created_at), now),
    urgent: urgent || undefined,
    requiredToPass:
      live.required_to_pass != null ? num(live.required_to_pass) : undefined,
  };
}

export type ProposalsResult = {
  proposals: ProposalCard[];
  source: "live" | "error";
  fallback: boolean;
  addressById: Record<string, string>;
  /**
   * created_at of the newest proposal this node holds, ISO, or null. The node
   * serves a mirror whose sync loop is off by default
   * (catalyrst-governance/src/config.rs:133), so a list with no date on it
   * reads as current when it can be a year old. Callers should surface it.
   */
  asOf: string | null;
};

function emptyProposalsResult(): ProposalsResult {
  return {
    proposals: [],
    source: "error",
    fallback: true,
    addressById: {},
    asOf: null,
  };
}

const PROPOSALS_TTL_MS = 5 * 60_000;
let proposalCorpusCache: { base: string; at: number; rows: LiveProposal[] } | null = null;
let proposalCorpusInflight: { base: string; promise: Promise<LiveProposal[]> } | null = null;

async function fetchProposalPage(
  base: string,
  doFetch: typeof fetch,
  offset: number,
  signal?: AbortSignal,
): Promise<LiveProposal[]> {
  const url = `${base}/proposals?limit=${PROPOSAL_PAGE}&offset=${offset}`;
  const res = await doFetch(url, {
    signal,
    cache: "no-store",
    headers: { accept: "application/json" },
  });
  if (!res.ok) throw new CatalystError(`governance ${res.status}`, url, res.status);
  const raw = (await res.json()) as unknown;
  const parsed = ProposalsEnvelopeSchema.safeParse(raw);
  if (!parsed.success) throw new CatalystError("governance schema mismatch", url);
  return parsed.data.data;
}

async function fetchAllProposalPages(
  base: string,
  doFetch: typeof fetch,
  signal?: AbortSignal,
): Promise<LiveProposal[]> {
  const first = await fetchProposalPage(base, doFetch, 0, signal);
  if (first.length < PROPOSAL_PAGE) return first;
  const offsets: number[] = [];
  for (let offset = PROPOSAL_PAGE; offset < PROPOSAL_CAP; offset += PROPOSAL_PAGE) {
    offsets.push(offset);
  }
  const pages = await Promise.all(
    offsets.map((offset) => fetchProposalPage(base, doFetch, offset, signal)),
  );
  const rows = [...first];
  for (const page of pages) {
    rows.push(...page);
    if (page.length < PROPOSAL_PAGE) break;
  }
  return rows;
}

async function fetchLiveProposalCorpus(opts: ProposalLoadOptions): Promise<LiveProposal[]> {
  const base = governanceApiBase(opts.base);
  if (opts.fetchImpl) return fetchAllProposalPages(base, opts.fetchImpl, opts.signal);

  const now = Date.now();
  const cached =
    proposalCorpusCache && proposalCorpusCache.base === base
      ? proposalCorpusCache
      : null;
  if (cached && now - cached.at < PROPOSALS_TTL_MS) {
    return cached.rows;
  }
  if (proposalCorpusInflight && proposalCorpusInflight.base === base) {
    return cached ? cached.rows : proposalCorpusInflight.promise;
  }
  const promise = fetchAllProposalPages(base, fetch)
    .then((rows) => {
      proposalCorpusCache = { base, at: Date.now(), rows };
      return rows;
    })
    .finally(() => {
      proposalCorpusInflight = null;
    });
  proposalCorpusInflight = { base, promise };
  if (cached) {
    promise.catch(() => {});
    return cached.rows;
  }
  return promise;
}

export async function loadProposals(
  opts: ProposalLoadOptions = {},
  now = Date.now(),
): Promise<ProposalsResult> {
  try {
    const rows = await fetchLiveProposalCorpus(opts);
    const proposals: ProposalCard[] = [];
    const addressById: Record<string, string> = {};
    for (const r of rows) {
      proposals.push(mapLiveProposal(r, now));
      if (r.user && ETH_ADDRESS_RE.test(r.user)) {
        addressById[r.id] = r.user.toLowerCase();
      }
    }
    if (proposals.length === 0) return emptyProposalsResult();
    const asOf = newestTimestamp(rows.map((r) => r.created_at));
    return { proposals, source: "live", fallback: false, addressById, asOf };
  } catch {
    return emptyProposalsResult();
  }
}

function synthDetailFromLiveProposal(live: LiveProposal, now: number): ProposalDetail {
  const type = (live.type ?? "").toLowerCase();
  const status = (live.status ?? "").toLowerCase();
  const catMeta = CATEGORY_META[type] ?? { label: titleCase(type), tone: "neutral" };
  const statusMeta = STATUS_META[status] ?? { label: titleCase(status), tone: "neutral" };
  const cfg = live.configuration ?? undefined;

  const description = cfg?.description ?? cfg?.abstract ?? undefined;
  const threshold =
    live.required_to_pass != null
      ? Math.round(num(live.required_to_pass)).toLocaleString("en-US")
      : "\u{2014}";

  let budget: { size: string; beneficiary: string; tier: string } | undefined;
  if (type === "grant") {
    const size = num(cfg?.size);
    const token = cfg?.paymentToken ?? "USD";
    budget = {
      size: size > 0 ? `${size.toLocaleString("en-US")} ${token}` : "\u{2014}",
      beneficiary: shortAddr(cfg?.beneficiary ?? live.user),
      tier: tierLabel(cfg?.tier),
    };
  }

  return {
    id: live.id,
    type,
    toneClass: catMeta.tone,
    catLabel: catMeta.label,
    catTone: catMeta.tone,
    status,
    statusLabel: statusMeta.label,
    statusTone: statusMeta.tone,
    title: live.title,
    author: shortAddr(live.user),
    authorHue: hueFrom(live.user ?? live.id),
    published: fmtDateTime(toIso(live.created_at)),
    start: fmtDateTime(live.start_at),
    finish: fmtDateTime(live.finish_at),
    snapshot: live.snapshot_id ? `#${live.snapshot_id.slice(2, 9)}` : "#snapshot",
    threshold,
    thresholdReached: false,
    yourVp: "0",
    description,
    budget,
  };
}

async function findLiveProposalById(
  id: string,
  opts: ProposalLoadOptions,
  now: number,
): Promise<LiveProposal | null> {
  void now;
  let confirmedAbsent = false;
  try {
    const base = governanceApiBase(opts.base);
    const doFetch = opts.fetchImpl ?? fetch;
    const url = `${base}/proposals?id=${encodeURIComponent(id)}&limit=1`;
    const res = await doFetch(url, {
      signal: opts.signal,
      cache: "no-store",
      headers: { accept: "application/json" },
    });
    if (res.ok) {
      const parsed = ProposalsEnvelopeSchema.safeParse(
        (await res.json()) as unknown,
      );
      if (parsed.success) {
        const match = parsed.data.data.find((r) => r.id === id);
        if (match) return match;
        confirmedAbsent = true;
      }
    }
  } catch {
  }
  try {
    const rows = await fetchLiveProposalCorpus(opts);
    return rows.find((r) => r.id === id) ?? null;
  } catch (err) {
    if (confirmedAbsent) return null;
    throw err;
  }
}

/**
 * Rows validate against the generated `ProjectRowSchema` -- the ts-rs image of
 * what catalyrst-governance's `GET /projects` serialises (`rows.rs`
 * `ProjectRow`). On the real wire `proposal_id`, `author`,
 * `configuration.category` and `created_at` are always present.
 */
type ProposalLiveProject = z.infer<typeof ProjectRowSchema>;

const ProposalProjectsListSchema = z.object({
  data: z.array(ProjectRowSchema),
});

const PROJECT_STATUS_LABEL: Record<string, { label: string; tone: string }> = {
  pending: { label: "Pending", tone: "neutral" },
  in_progress: { label: "In Progress", tone: "blue" },
  finished: { label: "Passed", tone: "green" },
  paused: { label: "Paused", tone: "orange" },
  revoked: { label: "Revoked", tone: "red" },
};

function num(v: unknown): number {
  const n = typeof v === "string" ? Number(v) : (v as number);
  return Number.isFinite(n) ? n : 0;
}

function shortAddr(addr: string | null | undefined): string {
  if (!addr) return "\u{2014}";
  return /^0x[0-9a-fA-F]{6,}$/.test(addr) ? shortAddress(addr) : addr;
}

function tierLabel(raw: string | null | undefined): string {
  if (!raw) return "\u{2014}";
  const m = raw.match(/^(Tier\s*\d+|Higher Tier|Lower Tier)/i);
  return m ? m[1] : raw;
}

function fmtDateTime(iso: string | null | undefined): string {
  if (!iso) return "\u{2014}";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "\u{2014}";
  const d = new Date(t);
  const date = d.toLocaleDateString("en-US", {
    month: "short",
    day: "2-digit",
    year: "numeric",
    timeZone: "UTC",
  });
  const time = d.toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  });
  return `${date} ${time}`;
}

function grantTemplate(): FixtureProposal | undefined {
  return (
    FIXTURE.proposals.find((p) => p.detail.type === "grant") ?? FIXTURE.proposals[0]
  );
}

function synthProposalFromProject(live: ProposalLiveProject): ProposalDetail {
  const tmpl = grantTemplate();
  const d = tmpl?.detail;
  const v = live.funding?.vesting ?? null;

  const status = live.status;
  const statusMeta =
    PROJECT_STATUS_LABEL[status] ??
    { label: status ? status.charAt(0).toUpperCase() + status.slice(1) : "\u{2014}", tone: "neutral" };

  const total =
    num(v?.total) || num(live.configuration?.size) || 0;
  const token = v?.token ?? live.funding?.one_time_payment?.token ?? "USDC";
  const sizeLabel =
    total > 0 ? `${Math.round(total).toLocaleString("en-US")} ${token}` : "\u{2014}";

  const published = fmtDateTime(
    live.created_at != null ? new Date(live.created_at).toISOString() : null,
  );
  const start = fmtDateTime(v?.start_at ?? live.funding?.enacted_at);
  const finish = fmtDateTime(v?.finish_at);

  return {
    id: live.proposal_id ?? live.id,
    type: "grant",
    toneClass: d?.toneClass ?? "purple",
    catLabel: "Grant Request",
    catTone: d?.catTone ?? "purple",
    status,
    statusLabel: statusMeta.label,
    statusTone: statusMeta.tone,
    title: live.title,
    author: live.author ?? "unknown",
    authorHue: tmpl?.hue ?? 268,
    published,
    start,
    finish,
    snapshot: d?.snapshot ?? "#snapshot",
    threshold: d?.threshold ?? "2,000,000",
    thresholdReached: d?.thresholdReached ?? false,
    yourVp: d?.yourVp ?? "0",
    budget: {
      size: sizeLabel,
      beneficiary: shortAddr(live.configuration?.beneficiary ?? live.author),
      tier: tierLabel(live.configuration?.tier),
    },
  };
}

export type ProposalLoadOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

export type ProposalDetailResult = {
  source: "fixture" | "live" | "project";
  proposal: ProposalDetail;
  authorAddress: string | null;
};

export async function loadProposalDetail(
  id: string | undefined,
  opts: ProposalLoadOptions = {},
): Promise<ProposalDetailResult | null> {
  if (!id) return null;

  const now = Date.now();

  let liveError: unknown = null;
  try {
    const live = await findLiveProposalById(id, opts, now);
    if (live) {
      return {
        source: "live",
        proposal: synthDetailFromLiveProposal(live, now),
        authorAddress: live.user ?? null,
      };
    }
  } catch (err) {
    liveError = err;
  }

  const apiBase = governanceApiBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(`${apiBase}/projects`, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (res.ok) {
      const raw = (await res.json()) as unknown;
      const parsed = ProposalProjectsListSchema.safeParse(raw);
      if (parsed.success) {
        const match =
          parsed.data.data.find((p) => p.proposal_id === id) ??
          parsed.data.data.find((p) => p.id === id);
        if (match) {
          return {
            source: "project",
            proposal: synthProposalFromProject(match),
            authorAddress: match.author ?? null,
          };
        }
      }
    }
  } catch {
  }

  if (liveError) throw liveError;
  return null;
}

const AvatarSnapshotsSchema = z
  .object({ face256: z.string().nullish() })
  .nullish();

const AvatarSchema = z.object({
  name: z.string().nullish(),
  userId: z.string().nullish(),
  ethAddress: z.string().nullish(),
  avatar: z
    .object({ snapshots: AvatarSnapshotsSchema })
    .nullish(),
});

const ProfileSchema = z.object({
  avatars: z.array(AvatarSchema).nullish(),
});

export type Profile = z.infer<typeof ProfileSchema>;

export type AuthorProfile = {
  address: string;
  name: string | null;
  face: string | null;
};

function projectProfile(address: string, raw: unknown): AuthorProfile {
  const parsed = ProfileSchema.safeParse(raw);
  const avatar = parsed.success ? parsed.data.avatars?.[0] : undefined;
  return {
    address: address.toLowerCase(),
    name: avatar?.name ?? null,
    face: avatar?.avatar?.snapshots?.face256 ?? null,
  };
}

export async function fetchAuthorProfiles(
  addresses: string[],
  opts: GetOptions = {},
): Promise<Record<string, AuthorProfile> | null> {
  const ids = addresses
    .filter((a) => ETH_ADDRESS_RE.test(a))
    .map((a) => a.toLowerCase());
  if (ids.length === 0) return {};

  const base = catalystBase(opts.base);
  const url = `${base}/lambdas/profiles`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ ids }),
      signal: opts.signal,
    });
    if (!res.ok) return null;
    const raw = (await res.json()) as unknown;
    const list = Array.isArray(raw) ? raw : [];

    const out: Record<string, AuthorProfile> = {};
    for (const entry of list) {
      const parsed = ProfileSchema.safeParse(entry);
      const addr = parsed.success
        ? parsed.data.avatars?.[0]?.ethAddress ?? parsed.data.avatars?.[0]?.userId
        : undefined;
      if (!addr) continue;
      out[addr.toLowerCase()] = projectProfile(addr, entry);
    }
    return out;
  } catch {
    return null;
  }
}

export async function fetchAuthorProfile(
  address: string,
  opts: GetOptions = {},
): Promise<AuthorProfile | null> {
  if (!ETH_ADDRESS_RE.test(address)) return null;
  try {
    const raw = await getJSON<unknown>(
      `/lambdas/profiles/${encodeURIComponent(address)}`,
      opts,
    );
    return projectProfile(address, raw);
  } catch (err) {
    if (err instanceof CatalystError) return null;
    return null;
  }
}

export function applyAuthorLabels(
  cards: ProposalCard[],
  profiles: Record<string, AuthorProfile>,
  addressById?: Record<string, string>,
): ProposalCard[] {
  if (Object.keys(profiles).length === 0) return cards;
  return cards.map((card) => {
    const addr = addressById?.[card.id] ?? getProposalAuthorAddress(card.id);
    if (!addr) return card;
    const prof = profiles[addr.toLowerCase()];
    return prof?.name ? { ...card, author: prof.name } : card;
  });
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProposalProjectRow = Assert<
  AssignableTo<RsProjectRow, z.input<typeof ProjectRowSchema>>
>;
export type _DriftProjectsEnvelope = Assert<
  AssignableTo<RsProjectsEnvelope, z.input<typeof ProposalProjectsListSchema>>
>;
