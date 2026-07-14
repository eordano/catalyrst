import type { GetOptions } from "../client";
import { shortAddress } from "../format/address";
import type { ProjectCard } from "./projects";
import {
  ActivityPayloadSchema,
  EngagementPayloadSchema,
} from "../generated-schemas/governance";
import { governanceApiBase } from "./api-base";

export type HomeTopVoter = { rank: number; name: string; hue: number; votes: number };
export type HomeChartPoint = { label: string; value: number };
export type HomeActivity = {
  id: number;
  kind: string;
  hue?: number;
  html: string;
  date: string;
};
export type HomeGrant = {
  id: number;
  title: string;
  category: string;
  hue: number;
  size: string;
  pct: number;
  months: number;
  update: string;
};

function hueFrom(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return Math.abs(h);
}

export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function relTimeFromEpoch(ts: number, now = Date.now()): string {
  const diff = Math.max(0, Math.floor(now / 1000) - ts);
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} min ago`;
  if (diff < 86_400) return `${Math.floor(diff / 3600)} hr ago`;
  if (diff < 7 * 86_400) return `${Math.floor(diff / 86_400)} d ago`;
  return `${Math.floor(diff / (7 * 86_400))} wk ago`;
}

function weekLabel(isoDate: string): string {
  const m = /^\d{4}-(\d{2})-(\d{2})/.exec(isoDate);
  return m ? `${m[1]}/${m[2]}` : isoDate;
}

export async function loadHomeEngagement(
  opts: GetOptions = {},
): Promise<{ topVoters: HomeTopVoter[]; chartPoints: HomeChartPoint[] } | null> {
  try {
    const base = governanceApiBase(opts.base);
    const doFetch = opts.fetchImpl ?? fetch;
    const res = await doFetch(`${base}/votes/engagement?days=30&limit=6`, {
      signal: opts.signal,
      cache: "no-store",
      headers: { accept: "application/json" },
    });
    if (!res.ok) return null;
    const parsed = EngagementPayloadSchema.safeParse((await res.json()) as unknown);
    if (!parsed.success) return null;
    return {
      topVoters: parsed.data.voters.map((v, i) => ({
        rank: i + 1,
        name: shortAddress(v.address),
        hue: hueFrom(v.address),
        votes: v.votes,
      })),
      chartPoints: parsed.data.weekly.map((w) => ({
        label: weekLabel(w.week_start),
        value: w.votes,
      })),
    };
  } catch {
    return null;
  }
}

export async function loadHomeActivity(
  opts: GetOptions = {},
  now = Date.now(),
): Promise<HomeActivity[] | null> {
  try {
    const base = governanceApiBase(opts.base);
    const doFetch = opts.fetchImpl ?? fetch;
    const res = await doFetch(`${base}/activity?limit=12`, {
      signal: opts.signal,
      cache: "no-store",
      headers: { accept: "application/json" },
    });
    if (!res.ok) return null;
    const parsed = ActivityPayloadSchema.safeParse((await res.json()) as unknown);
    if (!parsed.success) return null;
    return parsed.data.items.map((item, i) => {
      const title = escapeHtml(item.title ?? "a proposal");
      const actor = escapeHtml(shortAddress(item.address ?? ""));
      let html: string;
      switch (item.kind) {
        case "vote":
          html = `<b>${actor}</b> voted on <b>${title}</b>`;
          break;
        case "proposal":
          html = `<b>${actor || "Someone"}</b> published <b>${title}</b>`;
          break;
        case "finished":
          html = `Voting finished on <b>${title}</b>`;
          break;
        case "update":
          html = `New project update on <b>${title}</b>`;
          break;
        default:
          html = `<b>${title}</b>`;
      }
      return {
        id: i + 1,
        kind: item.kind,
        hue: item.address ? hueFrom(item.address) : undefined,
        html,
        date: relTimeFromEpoch(item.ts, now),
      };
    });
  } catch {
    return null;
  }
}

const GRANT_CATEGORY_LABEL: Record<string, string> = {
  accelerator: "Accelerator",
  core_unit: "Core Unit",
  documentation: "Documentation",
  in_world_content: "In-World Content",
  platform: "Platform",
  social_media_content: "Social Media Content",
  sponsorship: "Sponsorship",
  tender: "Tender",
};

export function toHomeGrants(projects: ProjectCard[], max = 6): HomeGrant[] {
  return projects
    .filter(
      (p) =>
        p.type === "grant" &&
        p.status !== "revoked" &&
        p.total > 0 &&
        p.vestedPct < 100,
    )
    .slice(0, max)
    .map((p, i) => ({
      id: i + 1,
      title: p.title,
      category: GRANT_CATEGORY_LABEL[p.category] ?? p.category,
      hue: p.hue,
      size: `$${Math.round(p.size).toLocaleString("en-US")}`,
      pct: p.vestedPct,
      months: p.months,
      update:
        p.update.index > 0
          ? `Update #${p.update.index} \u{B7} ${p.update.intro.slice(0, 60)}`
          : "No updates yet",
    }));
}
