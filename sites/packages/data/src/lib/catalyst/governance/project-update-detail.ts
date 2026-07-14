import { z } from "zod";

import fixture from "../../../fixtures/governance-project-update-detail.json";
import {
  CommentsPayloadSchema,
  ProjectRowSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";

export { governanceApiBase };

export type ProjectHealth = "onTrack" | "atRisk" | "offTrack";

export type FinancialRow = {
  description: string;
  receiver: string;
  token: string;
  amount: number;
  link: string;
};

export type FinancialGroup = {
  category: string;
  records: FinancialRow[];
};

export type FundsSummary = {
  released: string;
  releasedTxCount: number;
  releasedTime: string;
  disclosed: string;
  undisclosed: string;
};

export type DetailProject = {
  id: string;
  title: string;
  authorHue: number;
};

export type DetailUpdate = {
  id: string;
  index: number;
  status: string;
  health: ProjectHealth | null;
  author: string;
  completion_date: string;
  updated_at: string;
  due_date: string;
  due_amount: string;
  introduction: string;
  highlights: string[];
  blockers: string;
  next_steps: string;
  additional_notes: string;
  financial_records: FinancialGroup[];
  funds: FundsSummary;
  discourse_topic_id: number | null;
};

export type UpdateComment = {
  id: number;
  name: string;
  hue: number;
  validated: boolean;
  time: string;
  text: string;
};

export type ProjectUpdateDetail = {
  source: "live" | "fixture";
  project: DetailProject;
  update: DetailUpdate;
  comments: UpdateComment[];
  totalComments: number;
};

function money(n: number): string {
  return "$" + n.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function dayDelta(dueIso: string | null, doneIso: string | null): string {
  if (!dueIso || !doneIso) return "";
  const due = new Date(dueIso).getTime();
  const done = new Date(doneIso).getTime();
  if (Number.isNaN(due) || Number.isNaN(done)) return "";
  const days = Math.max(0, Math.round((done - due) / 86_400_000));
  return `${days} ${days === 1 ? "day" : "days"}`;
}

export function highlightsToList(md: string | null | undefined): string[] {
  if (!md) return [];
  const lines = md.split(/\r?\n/);
  const out: string[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    const bullet = line.replace(/^[-*+]\s+/, "").replace(/^\d+\.\s+/, "");
    const clean = bullet
      .replace(/^#{1,6}\s+/, "")
      .replace(/\*\*/g, "")
      .replace(/`/g, "")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .trim();
    if (clean) out.push(clean);
  }
  if (out.length === 0) {
    const t = md.trim();
    return t ? [t] : [];
  }
  return out;
}

export function groupFinancials(
  records: ReadonlyArray<{
    category?: string | null;
    description?: string | null;
    receiver?: string | null;
    token?: string | null;
    amount?: number | null;
    link?: string | null;
  }>,
): FinancialGroup[] {
  const order: string[] = [];
  const byCat = new Map<string, FinancialRow[]>();
  for (const r of records) {
    const cat = r.category || "Other";
    if (!byCat.has(cat)) {
      byCat.set(cat, []);
      order.push(cat);
    }
    byCat.get(cat)!.push({
      description: r.description ?? "",
      receiver: r.receiver ?? "",
      token: r.token ?? "",
      amount: typeof r.amount === "number" ? r.amount : 0,
      link: r.link ?? "",
    });
  }
  return order.map((cat) => ({ category: cat, records: byCat.get(cat)! }));
}

type FixtureShape = {
  project: {
    id: string;
    title: string;
    authorHue?: number;
    author?: string;
  };
  update: {
    id: string;
    index: number;
    status: string;
    health: string | null;
    author: string | null;
    completion_date: string | null;
    updated_at: string | null;
    due_date: string | null;
    introduction: string | null;
    highlights: string | null;
    blockers: string | null;
    next_steps: string | null;
    additional_notes: string | null;
    discourse_topic_id: number | null;
    financial_records: Array<{
      category?: string;
      description?: string;
      receiver?: string;
      token?: string;
      amount?: number;
      link?: string;
    }>;
  };
  funds: FundsSummary;
  comments: Array<{
    user_forum_id?: number;
    username: string;
    avatar_url?: string;
    created_at: string;
    text: string;
  }>;
  totalComments: number;
};

const FIXTURE = fixture as unknown as FixtureShape;

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const days = Math.max(0, Math.round((Date.now() - then) / 86_400_000));
  if (days <= 0) return "today";
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  const months = Math.round(days / 30);
  if (months < 12) return `${months} ${months === 1 ? "month" : "months"} ago`;
  const years = Math.round(months / 12);
  return `${years} ${years === 1 ? "year" : "years"} ago`;
}

function hueFor(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return h % 360;
}

const VALID_HEALTH = new Set<ProjectHealth>(["onTrack", "atRisk", "offTrack"]);
function normHealth(h: string | null | undefined): ProjectHealth | null {
  return h && VALID_HEALTH.has(h as ProjectHealth) ? (h as ProjectHealth) : null;
}

function projectComments(
  raw: ReadonlyArray<{
    user_forum_id?: number | null;
    username: string;
    created_at: string;
    text: string;
  }>,
): UpdateComment[] {
  return raw.map((c, i) => ({
    id: c.user_forum_id ?? i + 1,
    name: c.username,
    hue: hueFor(c.username),
    validated: !/^0x/i.test(c.username),
    time: relativeTime(c.created_at),
    text: c.text,
  }));
}

export function fixtureDetail(): ProjectUpdateDetail {
  const u = FIXTURE.update;
  return {
    source: "fixture",
    project: {
      id: FIXTURE.project.id,
      title: FIXTURE.project.title,
      authorHue: FIXTURE.project.authorHue ?? 268,
    },
    update: {
      id: u.id,
      index: u.index,
      status: u.status,
      health: normHealth(u.health),
      author: u.author ?? "",
      completion_date: fmtDate(u.completion_date),
      updated_at: fmtDate(u.updated_at),
      due_date: fmtDate(u.due_date),
      due_amount: dayDelta(u.due_date, u.completion_date ?? u.updated_at),
      introduction: u.introduction ?? "",
      highlights: highlightsToList(u.highlights),
      blockers: u.blockers ?? "",
      next_steps: u.next_steps ?? "",
      additional_notes: u.additional_notes ?? "",
      financial_records: groupFinancials(u.financial_records ?? []),
      funds: FIXTURE.funds,
      discourse_topic_id: u.discourse_topic_id,
    },
    comments: projectComments(FIXTURE.comments),
    totalComments: FIXTURE.totalComments,
  };
}

const ProjectDetailSchema = ProjectRowSchema.extend({
  updates: z.array(z.record(z.string(), z.unknown())).optional(),
});

type ProjectDetailPayload = z.infer<typeof ProjectDetailSchema>;
type UpdateRecord = Record<string, unknown>;

function uStr(u: UpdateRecord, key: string): string | null {
  const v = u[key];
  return typeof v === "string" ? v : null;
}

function uNum(u: UpdateRecord, key: string): number | null {
  const v = u[key];
  return typeof v === "number" ? v : null;
}

function uFinancials(u: UpdateRecord) {
  const v = u["financial_records"];
  if (!Array.isArray(v)) return [];
  return v
    .filter((r): r is UpdateRecord => typeof r === "object" && r !== null)
    .map((r) => ({
      category: uStr(r, "category"),
      description: uStr(r, "description"),
      receiver: uStr(r, "receiver"),
      token: uStr(r, "token"),
      amount: uNum(r, "amount"),
      link: uStr(r, "link"),
    }));
}

function stripHtml(html: string): string {
  return html
    .replace(/<[^>]+>/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function projectFunds(
  p: ProjectDetailPayload,
  records: ReadonlyArray<{ amount?: number | null }>,
): FundsSummary {
  const released = p.funding.vesting?.released ?? 0;
  const disclosed = records.reduce((s, r) => s + (r.amount ?? 0), 0);
  return {
    released: money(released),
    releasedTxCount: 0,
    releasedTime: "",
    disclosed: money(disclosed),
    undisclosed: money(Math.max(0, released - disclosed)),
  };
}

export type LoadOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  commentLimit?: number;
};

export async function loadProjectUpdateDetail(
  projectId: string | undefined,
  opts: LoadOptions = {},
): Promise<ProjectUpdateDetail> {
  const fb = fixtureDetail();
  if (!projectId) return fb;

  const base = governanceApiBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;
  const limit = opts.commentLimit ?? 6;

  try {
    const res = await doFetch(
      `${base}/projects/${encodeURIComponent(projectId)}`,
      { headers: { accept: "application/json" }, signal: opts.signal },
    );
    if (!res.ok) return fb;
    const parsed = ProjectDetailSchema.safeParse((await res.json()) as unknown);
    if (!parsed.success) return fb;
    const p = parsed.data;

    const updates = p.updates ?? [];
    let chosenIdx = -1;
    for (let i = updates.length - 1; i >= 0; i--) {
      const u = updates[i];
      if (uStr(u, "health") && (uStr(u, "introduction") || uStr(u, "highlights"))) {
        chosenIdx = i;
        break;
      }
    }
    if (chosenIdx === -1) return fb;
    const u = updates[chosenIdx];
    const records = uFinancials(u);

    let comments: UpdateComment[] = [];
    let totalComments = 0;
    try {
      const cres = await doFetch(
        `${base}/proposals/${encodeURIComponent(p.proposal_id)}/comments?limit=${limit}`,
        { headers: { accept: "application/json" }, signal: opts.signal },
      );
      if (cres.ok) {
        const cparsed = CommentsPayloadSchema.safeParse(
          (await cres.json()) as unknown,
        );
        if (cparsed.success) {
          totalComments = cparsed.data.total;
          comments = projectComments(
            cparsed.data.comments.slice(0, limit).map((c, i) => ({
              user_forum_id: i + 1,
              username: c.username,
              created_at: c.created_at,
              text: stripHtml(c.text).slice(0, 600),
            })),
          );
        }
      }
    } catch {
    }

    return {
      source: "live",
      project: {
        id: p.id,
        title: p.title,
        authorHue: hueFor(p.author || p.id),
      },
      update: {
        id: uStr(u, "id") ?? `u${chosenIdx + 1}`,
        index: chosenIdx + 1,
        status: uStr(u, "status") ?? "done",
        health: normHealth(uStr(u, "health")),
        author: uStr(u, "author") ?? "",
        completion_date: fmtDate(uStr(u, "completion_date")),
        updated_at: fmtDate(uStr(u, "updated_at")),
        due_date: fmtDate(uStr(u, "due_date")),
        due_amount: dayDelta(
          uStr(u, "due_date"),
          uStr(u, "completion_date") ?? uStr(u, "updated_at"),
        ),
        introduction: uStr(u, "introduction") ?? "",
        highlights: highlightsToList(uStr(u, "highlights")),
        blockers: uStr(u, "blockers") ?? "",
        next_steps: uStr(u, "next_steps") ?? "",
        additional_notes: uStr(u, "additional_notes") ?? "",
        financial_records: groupFinancials(records),
        funds: projectFunds(p, records),
        discourse_topic_id: uNum(u, "discourse_topic_id"),
      },
      comments,
      totalComments,
    };
  } catch {
    return fb;
  }
}
