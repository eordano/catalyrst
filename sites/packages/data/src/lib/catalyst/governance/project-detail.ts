import { z } from "zod";

import type { ListEnvelope as RsListEnvelope } from "@ui/generated/catalyst/governance/ListEnvelope";
import { governanceApiBase } from "./api-base";

export { governanceApiBase };

export type ProjectLink = { id: string; label: string; url: string };

export type Personnel = {
  id: string;
  name: string;
  address: string | null;
  role: string;
  about: string;
  relevantLink?: string;
};

export type Milestone = {
  id: string;
  date: string;
  title: string;
  description: string;
};

export type ProjectUpdate = {
  id: string;
  status: string;
  health: string | null;
  introduction: string;
  created_at: string;
  completion_date?: string | null;
  due_date?: string | null;
  index: number;
};

export type ActivityItem = {
  id: string;
  kind: string;
  label: string;
  time: string;
};

export type NextVested = { time: number; unit: string; amount: string };

export type Funding = {
  enactedLabel: string;
  endLabel: string;
  total: string;
  token: string;
  vestedAmount: string;
  vestedPct: number;
  releasedAmount: string;
  releasedPct: number;
  nextVested: NextVested;
};

export type VestingContract = { id: string; label: string; url: string };

export type ProjectDetail = {
  id: string;
  proposal_id: string;
  title: string;
  status: string;
  ongoingDays: number;
  about: string;
  links: ProjectLink[];
  personnel: Personnel[];
  milestones: Milestone[];
  funding: Funding;
  vestings: VestingContract[];
  type: string;
  author: string;
  authorLabel: string;
  authorHue: number;
  updates: ProjectUpdate[];
  activity: ActivityItem[];
};

export type DetailSource = "live" | "fallback";

export type ProjectDetailResult = {
  source: DetailSource;
  project: ProjectDetail | null;
};

const VestingSchema = z
  .object({
    start_at: z.string().nullable().optional(),
    finish_at: z.string().nullable().optional(),
    released: z.number().nullable().optional(),
    vested: z.number().nullable().optional(),
    total: z.number().nullable().optional(),
    token: z.string().nullable().optional(),
    vestedPerPeriod: z.array(z.number()).nullable().optional(),
  })
  .passthrough();

const LinkSchema = z
  .object({
    id: z.string().optional(),
    label: z.string().optional(),
    title: z.string().optional(),
    url: z.string(),
  })
  .passthrough();

const PersonnelSchema = z
  .object({
    id: z.string().optional(),
    name: z.string(),
    address: z.string().nullable().optional(),
    role: z.string().optional(),
    about: z.string().optional(),
    relevantLink: z.string().nullable().optional(),
  })
  .passthrough();

const MilestoneSchema = z
  .object({
    id: z.string().optional(),
    title: z.string(),
    description: z.string().optional(),
    delivery_date: z.string().nullable().optional(),
  })
  .passthrough();

const UpdateSchema = z
  .object({
    id: z.string().optional(),
    status: z.string().nullable().optional(),
    health: z.string().nullable().optional(),
    introduction: z.string().nullable().optional(),
    created_at: z.string().nullable().optional(),
    completion_date: z.string().nullable().optional(),
    due_date: z.string().nullable().optional(),
  })
  .passthrough();

const LiveProjectSchema = z
  .object({
    id: z.string(),
    proposal_id: z.string().optional(),
    title: z.string(),
    status: z.string(),
    about: z.string().nullable().optional(),
    type: z.string().optional(),
    author: z.string().nullable().optional(),
    vesting_addresses: z.array(z.string()).nullable().optional(),
    links: z.array(LinkSchema).nullable().optional(),
    personnel: z.array(PersonnelSchema).nullable().optional(),
    milestones: z.array(MilestoneSchema).nullable().optional(),
    updates: z.array(UpdateSchema).nullable().optional(),
    funding: z
      .object({
        enacted_at: z.string().nullable().optional(),
        vesting: VestingSchema.nullable().optional(),
      })
      .passthrough()
      .nullable()
      .optional(),
  })
  .passthrough();

const ProjectResponseSchema = z.union([
  z.object({ data: LiveProjectSchema }),
  LiveProjectSchema,
]);

const ProjectsListResponseSchema = z.object({
  data: z.array(LiveProjectSchema),
});

type LiveProject = z.infer<typeof LiveProjectSchema>;

function money(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

function pct(part: number, whole: number): number {
  if (whole <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((part / whole) * 100)));
}

function daysBetween(from: string, to: number): number {
  const start = Date.parse(from);
  if (Number.isNaN(start)) return 0;
  const d = Math.floor((to - start) / 86_400_000);
  return d > 0 ? d : 0;
}

function relativeLabel(iso: string | null | undefined, now: number): string {
  if (!iso) return "";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  const days = Math.round((t - now) / 86_400_000);
  const past = days < 0;
  const n = Math.abs(days);
  const months = Math.round(n / 30);
  const unit =
    n >= 45
      ? `${months} month${months === 1 ? "" : "s"}`
      : `${n} day${n === 1 ? "" : "s"}`;
  return past ? `${unit} ago` : `in ${unit}`;
}

function shortAddr(addr: string): string {
  const a = (addr ?? "").trim();
  if (!a) return "Anonymous";
  if (a.length <= 11) return a;
  return `${a.slice(0, 6)}\u{2026}${a.slice(-4)}`;
}

function hueFromAddr(addr: string): number {
  const a = (addr ?? "").toLowerCase();
  let h = 0;
  for (let i = 0; i < a.length; i++) {
    h = (h * 31 + a.charCodeAt(i)) % 360;
  }
  return h;
}

type LiveVesting = NonNullable<NonNullable<LiveProject["funding"]>["vesting"]>;

function computeNextVested(v: LiveVesting | null, now: number): NextVested {
  if (!v || v.total == null || v.vested == null || v.vested >= v.total) {
    return { time: 0, unit: "days", amount: "0" };
  }
  const remaining = v.total - v.vested;
  const periods = v.vestedPerPeriod ?? [];
  const perStep = periods.length > 0 ? periods[periods.length - 1] : remaining;
  const amount = Math.max(0, Math.min(perStep, remaining));

  const finish = v.finish_at ? Date.parse(v.finish_at) : NaN;
  let time = 0;
  let unit = "days";
  if (!Number.isNaN(finish) && finish > now) {
    const days = Math.round((finish - now) / 86_400_000);
    if (days >= 45) {
      time = Math.round(days / 30);
      unit = "months";
    } else {
      time = days;
      unit = "days";
    }
  }
  return { time, unit, amount: money(amount) };
}

function adaptLiveProject(live: LiveProject): ProjectDetail {
  const now = Date.now();
  const v = live.funding?.vesting ?? null;

  const total = v?.total ?? null;
  const vested = v?.vested ?? null;
  const released = v?.released ?? null;

  const funding: Funding = {
    enactedLabel: relativeLabel(live.funding?.enacted_at, now),
    endLabel: relativeLabel(v?.finish_at, now),
    total: total != null ? money(total) : "0",
    token: v?.token ?? "USD",
    vestedAmount: vested != null ? money(vested) : "0",
    vestedPct: vested != null && total != null ? pct(vested, total) : 0,
    releasedAmount: released != null ? money(released) : "0",
    releasedPct: released != null && total != null ? pct(released, total) : 0,
    nextVested: computeNextVested(v, now),
  };

  const ongoingDays =
    live.status === "in_progress" && v?.start_at
      ? daysBetween(v.start_at, now)
      : 0;

  const addresses = live.vesting_addresses ?? [];
  const vestings: VestingContract[] = addresses.map((addr, i) => ({
    id: `v${i + 1}`,
    label: addresses.length > 1 ? `Vesting contract ${i + 1}` : "Vesting contract",
    url: `https://etherscan.io/address/${addr}`,
  }));

  const links: ProjectLink[] = (live.links ?? []).map((l, i) => ({
    id: l.id ?? `l${i + 1}`,
    label: l.label ?? l.title ?? l.url,
    url: l.url,
  }));

  const personnel: Personnel[] = (live.personnel ?? []).map((m, i) => ({
    id: m.id ?? `p${i + 1}`,
    name: m.name,
    address: m.address ?? null,
    role: m.role ?? "",
    about: m.about ?? "",
    relevantLink: m.relevantLink ?? undefined,
  }));

  const milestones: Milestone[] = (live.milestones ?? []).map((m, i) => ({
    id: m.id ?? `m${i + 1}`,
    date: (m.delivery_date ?? "").slice(0, 10),
    title: m.title,
    description: m.description ?? "",
  }));

  const updates: ProjectUpdate[] = (live.updates ?? []).map((u, i) => ({
    id: u.id ?? `u${i + 1}`,
    status: u.status ?? "",
    health: u.health ?? null,
    introduction: u.introduction ?? "",
    created_at: u.created_at ?? "",
    completion_date: u.completion_date ?? null,
    due_date: u.due_date ?? null,
    index: i + 1,
  }));

  const author = live.author ?? "";

  return {
    id: live.id,
    proposal_id: live.proposal_id ?? "",
    title: live.title,
    status: live.status,
    ongoingDays,
    about: live.about ?? "",
    links,
    personnel,
    milestones,
    funding,
    vestings,
    type: live.type ?? "grant",
    author,
    authorLabel: shortAddr(author),
    authorHue: hueFromAddr(author),
    updates,
    activity: [],
  };
}

export type LoadOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

export async function loadProjectDetail(
  id: string | undefined,
  opts: LoadOptions = {},
): Promise<ProjectDetailResult | null> {
  if (!id) return null;

  const apiBase = governanceApiBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(`${apiBase}/projects/${encodeURIComponent(id)}`, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (res.ok) {
      const raw = (await res.json()) as unknown;
      const parsed = ProjectResponseSchema.safeParse(raw);
      if (parsed.success) {
        const live = (
          "data" in parsed.data ? parsed.data.data : parsed.data
        ) as LiveProject;
        if (live?.id) {
          return { source: "live", project: adaptLiveProject(live) };
        }
      }
    }
  } catch {
  }

  try {
    const res = await doFetch(`${apiBase}/projects`, {
      headers: { accept: "application/json" },
      signal: opts.signal,
    });
    if (res.ok) {
      const raw = (await res.json()) as unknown;
      const parsed = ProjectsListResponseSchema.safeParse(raw);
      if (parsed.success) {
        const live = parsed.data.data.find((p) => p.id === id);
        if (live) {
          return { source: "live", project: adaptLiveProject(live) };
        }
      }
    }
  } catch {
  }

  return { source: "fallback", project: null };
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProjectsListEnvelope = Assert<
  AssignableTo<
    RsListEnvelope,
    Omit<z.input<typeof ProjectsListResponseSchema>, "data"> & {
      data: RsListEnvelope["data"];
    }
  >
>;
