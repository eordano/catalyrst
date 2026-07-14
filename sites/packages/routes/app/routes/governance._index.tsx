import { useEffect, useRef } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import GvHomeLanding from "@ui/governance/pages/GvHomeLanding";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import {
  loadProposals,
  fetchAuthorProfiles,
  applyAuthorLabels,
  type ProposalCard,
} from "@data/lib/catalyst/governance/index";
import { loadProjects, computeStats } from "@data/lib/catalyst/governance/projects";
import {
  loadHomeEngagement,
  loadHomeActivity,
  toHomeGrants,
  type HomeGrant,
  type HomeTopVoter,
  type HomeChartPoint,
  type HomeActivity,
} from "@data/lib/catalyst/governance/home-extras";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance._index";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/home";

export const handle = { agentMarkdown: "governanceLanding" } satisfies AgentMarkdownHandle;

const ENDING_SOON_LIMIT = 5;

const CATEGORY_PILL: Record<string, { tone: string; label: string }> = {
  poi: { tone: "green", label: "Point of Interest" },
  catalyst: { tone: "blue", label: "Catalyst Node" },
  ban_name: { tone: "fuchsia", label: "Name Ban" },
  grant: { tone: "purple", label: "Grant Request" },
  linked_wearables: { tone: "yellow", label: "Linked Wearables" },
  hiring: { tone: "green", label: "Hiring" },
  council_decision_veto: { tone: "red", label: "Council Decision Veto" },
  poll: { tone: "orange", label: "Poll" },
  draft: { tone: "orange", label: "Draft" },
  governance: { tone: "orange", label: "Governance" },
  pitch: { tone: "red", label: "Pitch" },
  tender: { tone: "red", label: "Tender" },
  bid: { tone: "red", label: "Bid" },
};

function toEndingSoon(card: ProposalCard): Record<string, unknown> {
  const pill = CATEGORY_PILL[card.category] ?? {
    tone: "orange",
    label: card.category || "Proposal",
  };
  return {
    id: card.id,
    title: card.title,
    author: card.author,
    hue: card.hue,
    type: pill.label,
    tone: pill.tone,
    votes: card.votes,
    comments: undefined,
    time: card.time,
    urgent: card.urgent,
    met: card.passing,
    vp:
      card.requiredToPass != null
        ? Math.round(card.requiredToPass).toLocaleString("en-US")
        : "\u{2014}",
  };
}

function compactUsd(n: number): string {
  if (n >= 1_000_000) return `$${(n / 1_000_000).toFixed(1)}M USD`;
  return `$${Math.round(n).toLocaleString("en-US")} USD`;
}

const FALLBACK: Assignment = {
  variant: "metrics-forward",
  flags: { showMetrics: true, showEndingSoon: true },
  experimentKey: "gv_home_landing",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const [
    { proposals, fallback, addressById },
    projectsResult,
    engagement,
    activity,
  ] = await Promise.all([
    loadProposals({ signal: request.signal }),
    loadProjects({ signal: request.signal }),
    loadHomeEngagement({ signal: request.signal }),
    loadHomeActivity({ signal: request.signal }),
  ]);

  const activeAll = proposals.filter((p) => p.status === "active");
  const active = activeAll.slice(0, ENDING_SOON_LIMIT);

  const metrics = fallback
    ? []
    : [
        {
          category: "Proposals",
          title: `${activeAll.length} active proposal${activeAll.length === 1 ? "" : "s"}`,
          description: `${activeAll.filter((p) => p.urgent).length} ending in the next 48hs`,
        },
      ];

  let bottomStats: { value: string; label: string }[] = [];
  if (projectsResult.source === "live") {
    const stats = computeStats(projectsResult.projects);
    bottomStats = [
      { value: compactUsd(stats.totalFunding), label: "Funds allocated" },
      { value: stats.grantsCount.toLocaleString("en-US"), label: "Grants funded" },
    ];
  }

  let withAuthors = active;
  try {
    const addresses = [
      ...new Set(
        active
          .map((p) => addressById[p.id])
          .filter((a): a is string => Boolean(a)),
      ),
    ];
    if (addresses.length > 0) {
      const profiles = await fetchAuthorProfiles(addresses, {
        signal: request.signal,
      });
      if (profiles) withAuthors = applyAuthorLabels(active, profiles, addressById);
    }
  } catch {
    withAuthors = active;
  }

  const endingSoon = withAuthors.map(toEndingSoon);

  const grants =
    projectsResult.source === "live" ? toHomeGrants(projectsResult.projects) : [];

  const payload = {
    sid,
    endingSoon,
    fallback,
    metrics,
    bottomStats,
    grants,
    topVoters: engagement?.topVoters ?? [],
    chartPoints: engagement?.chartPoints ?? [],
    engagementUnavailable: engagement === null,
    activity: activity ?? [],
    activityUnavailable: activity === null,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  endingSoon: Record<string, unknown>[];
  fallback: boolean;
  metrics: { category: string; title: string; description: string }[];
  bottomStats: { value: string; label: string }[];
  grants: HomeGrant[];
  topVoters: HomeTopVoter[];
  chartPoints: HomeChartPoint[];
  engagementUnavailable: boolean;
  activity: HomeActivity[];
  activityUnavailable: boolean;
};

export default function GovernanceHome({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "gv_home_viewed",
      { ending_soon: d.endingSoon.length },
      { sid: d.sid, story: STORY },
    );
  }, [d.sid, d.endingSoon.length]);

  function onProposalsClick() {
    track("gv_proposals_clicked", {}, { sid: d.sid, story: STORY });
  }

  return (
    <div className="governance-home-route">
      <GvHomeLanding
        endingSoon={
          d.endingSoon as React.ComponentProps<typeof GvHomeLanding>["endingSoon"]
        }
        metrics={d.metrics}
        bottomStats={d.bottomStats}
        grants={d.grants}
        topVoters={d.topVoters}
        chartPoints={d.chartPoints}
        engagementUnavailable={d.engagementUnavailable}
        activity={d.activity}
        activityUnavailable={d.activityUnavailable}
      />

      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "0 24px 48px",
          textAlign: "center",
        }}
      >
        <Link
          to={href("/governance/proposals")}
          prefetch="intent"
          onClick={onProposalsClick}
          style={{
            display: "inline-block",
            padding: "12px 22px",
            borderRadius: 10,
            background: "var(--brand-cta)",
            color: "#fff",
            fontWeight: 600,
            textDecoration: "none",
          }}
        >
          View all proposals {"\u{2192}"}
        </Link>
      </div>
    </div>
  );
}
