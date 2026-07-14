import { useEffect, useRef } from "react";
import { Outlet, useMatches, useSearchParams } from "react-router";

import GvProjectDetail from "@ui/governance/pages/GvProjectDetail";

import {
  loadProjectDetail,
  type DetailSource,
  type ProjectDetail,
} from "@data/lib/catalyst/governance/project-detail";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, trackExposure } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance.projects_.$id";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "projectDetail" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "governance/project-detail";

const TABS = ["general", "milestones", "updates", "activity"] as const;
type Tab = (typeof TABS)[number];

function readTab(raw: string | null): Tab {
  const v = (raw ?? "").toLowerCase();
  return (TABS as readonly string[]).includes(v) ? (v as Tab) : "general";
}

function uiTabId(tab: Tab): string {
  return tab === "general" ? "general_info" : tab;
}

const FALLBACK: Assignment = {
  variant: "funding-sidebar",
  flags: { showFundingSidebar: true, showVestingProgress: true },
  experimentKey: "gv_project_detail",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;
  const url = new URL(request.url);
  const tab = readTab(url.searchParams.get("tab"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/project-detail",
    FALLBACK,
    { skipExposure: true },
  );

  let project: ProjectDetail | null = null;
  let source: DetailSource = "fallback";
  try {
    const res = await loadProjectDetail(id, { signal: request.signal });
    if (res) {
      project = res.project;
      source = res.source;
    }
  } catch {
    project = null;
  }

  if (project) {
    trackExposure({
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }

  const payload = { id, tab, project, source, sid, assignment };

  return wrap(payload);
}

export default function GovernanceProjectDetailRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const matches = useMatches();
  const hasChild =
    matches[matches.length - 1]?.id !== "routes/governance.projects_.$id";
  if (hasChild) return <Outlet />;
  return (
    <ProjectDetailView
      id={d.id}
      tab={d.tab}
      project={d.project}
      source={d.source}
      sid={d.sid}
      assignment={d.assignment}
    />
  );
}

type ViewProps = {
  id: string;
  tab: Tab;
  project: ProjectDetail | null;
  source: DetailSource;
  sid: string;
  assignment: Assignment;
};

function ProjectDetailView({ id, tab, project, source, sid, assignment }: ViewProps) {
  const [, setSearchParams] = useSearchParams();
  const ctx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  const viewedId = useRef<string | null>(null);
  useEffect(() => {
    if (!project || viewedId.current === id) return;
    viewedId.current = id;
    track(
      "gv_project_viewed",
      { project_id: id, status: project.status, source },
      ctx,
    );
  }, [id, project, source]); // eslint-disable-line react-hooks/exhaustive-deps

  const seenTab = useRef<string | null>(null);
  useEffect(() => {
    if (!project) return;
    const k = `${id}:${tab}`;
    if (tab === "general" || seenTab.current === k) return;
    seenTab.current = k;
    track("gv_project_tab_viewed", { project_id: id, tab }, ctx);
  }, [id, tab, project]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!project) {
    return (
      <main className="governance-project-detail-route governance-project-detail-route--notfound">
        <GvProjectDetail notFound initialTabId="" />
      </main>
    );
  }

  function updateTab(next: Tab) {
    if (next !== "general") {
      track("gv_project_tab_viewed", { project_id: id, tab: next }, ctx);
    }
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (next === "general") params.delete("tab");
        else params.set("tab", next);
        return params;
      },
      { preventScrollReset: true },
    );
  }

  function onClick(e: React.MouseEvent<HTMLElement>) {
    const el = e.target as HTMLElement;

    const vtab = el.closest<HTMLElement>(".gpd__vtab");
    if (vtab) {
      const label = (vtab.textContent ?? "").toLowerCase();
      const next: Tab = label.includes("milestone")
        ? "milestones"
        : label.includes("update")
          ? "updates"
          : label.includes("activity")
            ? "activity"
            : "general";
      seenTab.current = `${id}:${next}`;
      updateTab(next);
      return;
    }

    const linkInRight = el.closest<HTMLElement>(".gpd__right .gpd__linkitem");
    if (linkInRight) {
      const sidebar = linkInRight.closest(".gpd__right");
      const links = sidebar
        ? Array.from(sidebar.querySelectorAll<HTMLElement>(".gpd__linkitem"))
        : [];
      const idx = links.indexOf(linkInRight);
      const vesting = project?.vestings[idx];
      if (vesting) {
        track(
          "gv_project_vesting_clicked",
          { project_id: id, vesting_id: vesting.id },
          ctx,
        );
      }
    }
  }

  return (
    <main className="governance-project-detail-route" onClick={onClick}>
      <GvProjectDetail
        key={`${id}:${tab}`}
        initialTabId={uiTabId(tab)}
        project={project as React.ComponentProps<typeof GvProjectDetail>["project"]}
      />
    </main>
  );
}
