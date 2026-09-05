import type { z } from "zod";

import type { GetOptions } from "../client";
import {
  ProjectRowSchema,
  ProjectsEnvelopeSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";
import { newestTimestamp } from "./freshness";

export type ProjectInList = z.infer<typeof ProjectRowSchema>;

export type ProjectCard = {
  id: string;
  title: string;
  type: "grant" | "bid";
  category: string;
  status: string;
  author: string;
  hue: number;
  size: number;
  vested: number;
  released: number;
  total: number;
  vestedPct: number;
  releasedPct: number;
  started: string;
  ends: string;
  token: string;
  months: number;
  update: { index: number; intro: string; date: string; late: boolean };
  quarter: string | null;
  year: number | null;
};

const CATEGORY_KEY: Record<string, string> = {
  Accelerator: "accelerator",
  "Core Unit": "core_unit",
  Documentation: "documentation",
  "In-World Content": "in_world_content",
  Platform: "platform",
  "Social Media Content": "social_media_content",
  Sponsorship: "sponsorship",
};

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return h;
}

function shortAuthor(addr: string | null | undefined): string {
  if (!addr) return "unknown";
  if (/^0x[0-9a-fA-F]{6,}$/.test(addr)) {
    return `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}`;
  }
  return addr;
}

function num(v: unknown): number {
  const n = typeof v === "string" ? Number(v) : (v as number);
  return Number.isFinite(n) ? n : 0;
}

const MS_DAY = 86_400_000;

function relTime(iso: string | null | undefined, now: number): string {
  if (!iso) return "\u{2014}";
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "\u{2014}";
  const diffDays = Math.round((t - now) / MS_DAY);
  const past = diffDays < 0;
  const days = Math.abs(diffDays);
  const months = Math.round(days / 30);
  const phrase =
    months >= 1
      ? `${months} month${months === 1 ? "" : "s"}`
      : `${Math.max(days, 1)} day${days === 1 ? "" : "s"}`;
  if (past) return `${phrase} ago`;
  return `in ${phrase}`;
}

function quarterOf(iso: string | null | undefined): { quarter: string | null; year: number | null } {
  if (!iso) return { quarter: null, year: null };
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return { quarter: null, year: null };
  const year = d.getUTCFullYear();
  const q = Math.floor(d.getUTCMonth() / 3) + 1;
  return { quarter: `${year}-Q${q}`, year };
}

export function mapProject(p: ProjectInList, now = Date.now()): ProjectCard {
  const isBid = p.type === "bid" || p.type === "tender";
  const rawCat = p.configuration?.category ?? "";
  const category = isBid
    ? "tender"
    : CATEGORY_KEY[rawCat] ?? (rawCat.toLowerCase().replace(/\s+/g, "_") || "other");

  const v = p.funding?.vesting ?? undefined;
  const total = num(v?.total) || num(p.configuration?.size);
  const vested = num(v?.vested);
  const released = num(v?.released);
  const size = num(p.configuration?.size) || total;
  const token = v?.token ?? p.funding?.one_time_payment?.token ?? "USDC";

  const vestedPct = total > 0 ? Math.min(100, Math.round((vested / total) * 100)) : 0;
  const releasedPct = total > 0 ? Math.min(100, Math.round((released / total) * 100)) : 0;

  const u = p.latest_update?.update ?? undefined;
  const updTs = p.latest_update?.update_timestamp;
  const date = u?.completion_date
    ? new Date(u.completion_date).toLocaleDateString("en-US", { month: "short", day: "2-digit" })
    : updTs
      ? new Date(updTs * 1000).toLocaleDateString("en-US", { month: "short", day: "2-digit" })
      : "\u{2014}";

  const { quarter, year } = quarterOf(p.funding?.enacted_at);

  const finishMs = v?.finish_at ? Date.parse(v.finish_at) : NaN;
  const endsInPast =
    (Number.isFinite(finishMs) && finishMs < now) ||
    p.status === "finished" ||
    p.status === "revoked";
  const startMs = v?.start_at ? Date.parse(v.start_at) : NaN;
  const months =
    Number.isFinite(startMs) && Number.isFinite(finishMs) && finishMs > startMs
      ? Math.max(1, Math.round((finishMs - startMs) / 2_629_800_000))
      : 0;
  const endsLabel = relTime(v?.finish_at, now);
  const ends = endsInPast && !endsLabel.startsWith("ended")
    ? `ended ${endsLabel.replace(/^in /, "").replace(/ ago$/, " ago")}`
    : endsLabel;

  return {
    id: p.id,
    title: p.title,
    type: isBid ? "bid" : "grant",
    category,
    status: p.status,
    author: shortAuthor(p.author),
    hue: hueFrom(p.author ?? p.id),
    size,
    vested,
    released,
    total,
    vestedPct,
    releasedPct,
    started: relTime(v?.start_at, now),
    ends,
    token,
    months,
    update: {
      index: u?.index ?? 0,
      intro: u?.introduction ?? "No updates yet.",
      date,
      late: (u?.health ?? "") === "atRisk" || (u?.health ?? "") === "offTrack",
    },
    quarter,
    year,
  };
}

const PROJECT_PAGE = 200;
const PROJECT_CAP = 3000;

const PROJECTS_TTL_MS = 30_000;
let projectCorpusCache: { base: string; at: number; rows: ProjectInList[] } | null = null;
let projectCorpusInflight: { base: string; promise: Promise<ProjectInList[]> } | null = null;

async function fetchProjectPage(
  base: string,
  doFetch: typeof fetch,
  offset: number,
  signal?: AbortSignal,
): Promise<ProjectInList[]> {
  const url = `${base}/projects?limit=${PROJECT_PAGE}&offset=${offset}`;
  const res = await doFetch(url, {
    signal,
    cache: "no-store",
    headers: { accept: "application/json" },
  });
  if (!res.ok) throw new Error(`governance projects ${res.status}`);
  const raw = (await res.json()) as unknown;
  const parsed = ProjectsEnvelopeSchema.safeParse(raw);
  if (!parsed.success) throw new Error("governance projects schema mismatch");
  return parsed.data.data;
}

async function fetchAllProjectPages(
  base: string,
  doFetch: typeof fetch,
  signal?: AbortSignal,
): Promise<ProjectInList[]> {
  const first = await fetchProjectPage(base, doFetch, 0, signal);
  if (first.length < PROJECT_PAGE) return first;
  const offsets: number[] = [];
  for (let offset = PROJECT_PAGE; offset < PROJECT_CAP; offset += PROJECT_PAGE) {
    offsets.push(offset);
  }
  const pages = await Promise.all(
    offsets.map((offset) => fetchProjectPage(base, doFetch, offset, signal)),
  );
  const rows = [...first];
  for (const page of pages) {
    rows.push(...page);
    if (page.length < PROJECT_PAGE) break;
  }
  return rows;
}

async function fetchProjectCorpus(opts: GetOptions): Promise<ProjectInList[]> {
  const base = governanceApiBase(opts.base);
  if (opts.fetchImpl) return fetchAllProjectPages(base, opts.fetchImpl, opts.signal);

  const now = Date.now();
  if (projectCorpusCache && projectCorpusCache.base === base &&
      now - projectCorpusCache.at < PROJECTS_TTL_MS) {
    return projectCorpusCache.rows;
  }
  if (projectCorpusInflight && projectCorpusInflight.base === base) {
    return projectCorpusInflight.promise;
  }
  const promise = fetchAllProjectPages(base, fetch)
    .then((rows) => {
      projectCorpusCache = { base, at: Date.now(), rows };
      return rows;
    })
    .finally(() => {
      projectCorpusInflight = null;
    });
  projectCorpusInflight = { base, promise };
  return promise;
}

export async function loadProjects(
  opts: GetOptions = {},
  now = Date.now(),
): Promise<{
  projects: ProjectCard[];
  source: "live" | "unavailable";
  /**
   * updated_at/created_at of the newest project row this node holds, ISO, or
   * null. The mirror's sync loop is off by default
   * (catalyrst-governance/src/config.rs:133), so callers should say how old
   * this is rather than presenting it as current.
   */
  asOf: string | null;
}> {
  try {
    const rows = await fetchProjectCorpus(opts);
    const projects = rows.map((p) => mapProject(p, now));
    if (projects.length === 0) {
      return { projects: [], source: "unavailable", asOf: null };
    }
    const asOf = newestTimestamp(
      rows.flatMap((r) => [r.updated_at, r.created_at]),
    );
    return { projects, source: "live", asOf };
  } catch {
    return { projects: [], source: "unavailable", asOf: null };
  }
}

export type ProjectFilters = {
  category: string;
  subtype: string;
  status: string;
  year: string;
  quarter: string;
  sort: string;
};

const STATUS_ALIAS: Record<string, string> = {
  ongoing: "in_progress",
  in_progress: "in_progress",
  finished: "finished",
  paused: "paused",
  pending: "pending",
  revoked: "revoked",
};

export function normalizeStatus(s: string): string {
  const k = s.trim().toLowerCase();
  if (!k || k === "all") return "";
  return STATUS_ALIAS[k] ?? k;
}

export function normalizeCategory(c: string): string {
  const k = c.trim().toLowerCase();
  if (k === "grants" || k === "grant") return "grants";
  if (k === "bidding" || k === "bidding_and_tendering" || k === "bid" || k === "tender")
    return "bidding_and_tendering";
  return "";
}

export function filterProjects(
  all: ProjectCard[],
  f: ProjectFilters,
): ProjectCard[] {
  const cat = normalizeCategory(f.category);
  const status = normalizeStatus(f.status);
  const subtype = f.subtype.trim().toLowerCase();
  const year = f.year.trim();
  const quarter = f.quarter.trim().replace(/^q/i, "");

  let out = all.filter((p) => {
    if (cat === "grants") {
      if (p.type !== "grant") return false;
      if (subtype && subtype !== "all" && p.category !== subtype) return false;
    } else if (cat === "bidding_and_tendering") {
      if (p.type !== "bid") return false;
    }
    if (status && p.status !== status) return false;
    if (year && String(p.year ?? "") !== year) return false;
    if (quarter && p.quarter !== `${year || p.year}-Q${quarter}`) return false;
    return true;
  });

  out = out.slice();
  if (f.sort === "size") out.sort((a, b) => b.size - a.size);
  return out;
}

export type ProjectStats = {
  count: number;
  grantsCount: number;
  bidsCount: number;
  ongoingCount: number;
  finishedCount: number;
  grantFunding: number;
  bidFunding: number;
  totalFunding: number;
};

export function computeStats(projects: ProjectCard[]): ProjectStats {
  let grantFunding = 0;
  let bidFunding = 0;
  let grantsCount = 0;
  let bidsCount = 0;
  let ongoingCount = 0;
  let finishedCount = 0;
  for (const p of projects) {
    if (p.type === "grant") {
      grantsCount++;
      grantFunding += p.size;
    } else {
      bidsCount++;
      bidFunding += p.size;
    }
    if (p.status === "in_progress") ongoingCount++;
    if (p.status === "finished") finishedCount++;
  }
  return {
    count: projects.length,
    grantsCount,
    bidsCount,
    ongoingCount,
    finishedCount,
    grantFunding,
    bidFunding,
    totalFunding: grantFunding + bidFunding,
  };
}

export function fundingYears(projects: ProjectCard[]): number[] {
  const set = new Set<number>();
  for (const p of projects) if (p.year != null) set.add(p.year);
  return [...set].sort((a, b) => b - a);
}
