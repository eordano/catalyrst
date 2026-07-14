import type { GetOptions } from "../client";
import { shortAddress } from "../format/address";
import {
  CommentsPayloadSchema,
  ProposalVotesPayloadSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";

export type VoteChoiceVM = {
  id: string;
  label: string;
  pct: number;
  vp: string;
  tone: string;
  voted: boolean;
};
export type VoteRationaleVM = {
  id: number;
  name: string;
  hue: number;
  choice: string;
  tone: string;
  vp: string;
  text: string;
};
export type VpSeriesVM = { yes: number[]; no: number[]; ticks: string[] };
export type ProposalVotesVM = {
  choices: VoteChoiceVM[];
  totalVotesLabel: string;
  rationales: VoteRationaleVM[];
  vpSeries?: VpSeriesVM;
};
export type ProposalCommentVM = {
  id: number;
  name: string;
  hue: number;
  time: string;
  text: string;
};

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return Math.abs(h);
}

function titleCaseChoice(s: string): string {
  return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
}

function fmtVp(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

function toneFor(choiceIndex1: number): string {
  return choiceIndex1 === 1 ? "yes" : "no";
}

function relTime(iso: string, now = Date.now()): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "\u{2014}";
  const diff = Math.max(0, Math.floor((now - t) / 1000));
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} min ago`;
  if (diff < 86_400) return `${Math.floor(diff / 3600)} hr ago`;
  if (diff < 30 * 86_400) return `${Math.floor(diff / 86_400)} d ago`;
  return new Date(t).toLocaleDateString("en-US", { month: "short", day: "2-digit", year: "numeric" });
}

function tickLabel(isoDate: string): string {
  const m = /^\d{4}-(\d{2})-(\d{2})/.exec(isoDate);
  return m ? `${m[1]}/${m[2]}` : isoDate;
}

const RATIONALES_SHOWN = 8;

export async function loadProposalVotes(
  id: string,
  opts: GetOptions = {},
): Promise<ProposalVotesVM | null> {
  try {
    const base = governanceApiBase(opts.base);
    const doFetch = opts.fetchImpl ?? fetch;
    const res = await doFetch(
      `${base}/proposals/${encodeURIComponent(id)}/votes`,
      { signal: opts.signal, cache: "no-store", headers: { accept: "application/json" } },
    );
    if (!res.ok) return null;
    const parsed = ProposalVotesPayloadSchema.safeParse(
      (await res.json()) as unknown,
    );
    if (!parsed.success) return null;
    const d = parsed.data;
    if (d.choices.length === 0 || d.votes_count === 0) return null;

    const choices: VoteChoiceVM[] = d.choices.map((label, i) => {
      const score = d.scores[i] ?? 0;
      return {
        id: String(i + 1),
        label: titleCaseChoice(label),
        pct: d.scores_total > 0 ? Math.round((score / d.scores_total) * 100) : 0,
        vp: fmtVp(score),
        tone: toneFor(i + 1),
        voted: false,
      };
    });

    const rationales: VoteRationaleVM[] = d.votes
      .filter((v) => (v.reason ?? "").trim().length > 0)
      .slice(0, RATIONALES_SHOWN)
      .map((v, i) => ({
        id: i + 1,
        name: shortAddress(v.voter),
        hue: hueFrom(v.voter),
        choice: titleCaseChoice(d.choices[v.choice - 1] ?? ""),
        tone: toneFor(v.choice),
        vp: fmtVp(v.vp),
        text: (v.reason ?? "").trim(),
      }));

    const vpSeries: VpSeriesVM | undefined =
      d.series && d.series.ticks.length > 1
        ? {
            yes: d.series.choice1,
            no: d.series.choice2,
            ticks: d.series.ticks.map(tickLabel),
          }
        : undefined;

    return {
      choices,
      totalVotesLabel: `${d.votes_count.toLocaleString("en-US")} total votes`,
      rationales,
      vpSeries,
    };
  } catch {
    return null;
  }
}

export async function loadProposalComments(
  id: string,
  opts: GetOptions = {},
  now = Date.now(),
): Promise<{ total: number; comments: ProposalCommentVM[] }> {
  const empty = { total: 0, comments: [] };
  try {
    const base = governanceApiBase(opts.base);
    const doFetch = opts.fetchImpl ?? fetch;
    const res = await doFetch(
      `${base}/proposals/${encodeURIComponent(id)}/comments?limit=25`,
      { signal: opts.signal, cache: "no-store", headers: { accept: "application/json" } },
    );
    if (!res.ok) return empty;
    const parsed = CommentsPayloadSchema.safeParse((await res.json()) as unknown);
    if (!parsed.success) return empty;
    return {
      total: parsed.data.total,
      comments: parsed.data.comments.map((c, i) => ({
        id: i + 1,
        name: c.username,
        hue: hueFrom(c.username),
        time: relTime(c.created_at, now),
        text: c.text,
      })),
    };
  } catch {
    return empty;
  }
}
