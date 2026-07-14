import {
  loadProjects,
  filterProjects,
  computeStats,
  fundingYears,
  type ProjectCard,
  type ProjectFilters,
  type ProjectStats,
} from "@data/lib/catalyst/governance/projects";
import {
  governanceAsOfLabel,
  GOVERNANCE_MIRROR_NOTE,
} from "@data/lib/catalyst/governance/freshness";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import ProjectsList from "@features/components/governance/ProjectsList";

import type { Route } from "./+types/governance.projects";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/projects";

export const handle = { agentMarkdown: "governanceProjects" } satisfies AgentMarkdownHandle;

const SORTS = new Set(["update_timestamp", "created_at", "size"]);

const FALLBACK: Assignment = {
  variant: "filterable-list",
  flags: { urlFilters: true, quarterStats: true },
  experimentKey: "gv_projects_browse",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const category = url.searchParams.get("category")?.trim() ?? "";
  const subtype = url.searchParams.get("subtype")?.trim() ?? "";
  const status = url.searchParams.get("status")?.trim() ?? "";
  const year = url.searchParams.get("year")?.trim() ?? "";
  const quarterRaw = url.searchParams.get("quarter")?.trim() ?? "";
  const quarter = /^Q?[1-4]$/i.test(quarterRaw) ? quarterRaw.toUpperCase().replace(/^([1-4])$/, "Q$1") : "";
  const sortRaw = url.searchParams.get("sort")?.trim() ?? "";
  const sort = SORTS.has(sortRaw) ? sortRaw : "update_timestamp";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { projects: all, source, asOf } = await loadProjects({
    signal: request.signal,
  }).catch(() => ({
    projects: [] as ProjectCard[],
    source: "unavailable" as const,
    asOf: null,
  }));

  const filters: ProjectFilters = {
    category,
    subtype,
    status,
    year,
    quarter,
    sort,
  };
  const filtered = filterProjects(all, filters);

  const stats: ProjectStats = computeStats(filtered);
  const years = fundingYears(all);

  const payload = {
    sid,
    source,
    asOf,
    projects: filtered,
    stats,
    years,
    total: all.length,
    category,
    subtype,
    status,
    year,
    quarter,
    sort,
  };

  return wrap(payload);
}

export default function GovernanceProjectsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  const asOfLabel = governanceAsOfLabel(d.asOf);

  return (
    <>
      {asOfLabel && (
        <GovernanceNotice
          tone="stale"
          title={`Newest project record on this node: ${asOfLabel}`}
          detail={GOVERNANCE_MIRROR_NOTE}
        />
      )}
      <ProjectsList
        sid={d.sid}
        projects={d.projects}
        stats={d.stats}
        years={d.years}
        total={d.total}
        source={d.source}
        category={d.category}
        subtype={d.subtype}
        status={d.status}
        year={d.year}
        quarter={d.quarter}
        sort={d.sort}
      />
    </>
  );
}
