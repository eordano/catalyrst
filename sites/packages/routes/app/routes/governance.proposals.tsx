import "@ui/governance/pages/governanceproposals.css";

import {
  loadProposals,
  fetchAuthorProfiles,
  applyAuthorLabels,
  type ProposalCard,
} from "@data/lib/catalyst/governance/index";
import {
  governanceAsOfLabel,
  GOVERNANCE_MIRROR_NOTE,
} from "@data/lib/catalyst/governance/freshness";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import ProposalsList from "@features/components/governance/ProposalsList";

import type { Route } from "./+types/governance.proposals";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/proposals";

export const handle = { agentMarkdown: "governanceProposals" } satisfies AgentMarkdownHandle;

const CATEGORY_MEMBERS: Record<string, string[]> = {
  governance: ["poll", "draft", "governance"],
  bidding: ["pitch", "tender", "bid"],
};
const SIDEBAR_CATEGORY_IDS = [
  "poi",
  "catalyst",
  "ban_name",
  "linked_wearables",
  "hiring",
  "council_decision_veto",
  "governance",
  "grant",
  "bidding",
];

function filterProposals(
  all: ProposalCard[],
  category: string,
  status: string,
  search: string,
): ProposalCard[] {
  const q = search.toLowerCase();
  const members = category ? (CATEGORY_MEMBERS[category] ?? [category]) : null;
  return all.filter((p) => {
    if (members && !members.includes(p.category)) return false;
    if (status && p.status !== status) return false;
    if (q && !p.title.toLowerCase().includes(q)) return false;
    return true;
  });
}

function categoryCountsOf(all: ProposalCard[]): Record<string, number> {
  const counts: Record<string, number> = { all: all.length };
  for (const id of SIDEBAR_CATEGORY_IDS) {
    const members = CATEGORY_MEMBERS[id] ?? [id];
    counts[id] = all.filter((p) => members.includes(p.category)).length;
  }
  return counts;
}

const PAGE_SIZE = 36;

const FALLBACK: Assignment = {
  variant: "filterable-list",
  flags: { urlFilters: true },
  experimentKey: "gv_proposals_browse",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const category = url.searchParams.get("category")?.trim() ?? "";
  const status = url.searchParams.get("status")?.trim() ?? "";
  const search = url.searchParams.get("search")?.trim() ?? "";
  const pageParam = Number(url.searchParams.get("page"));
  const requestedPage =
    Number.isFinite(pageParam) && pageParam >= 1 ? Math.floor(pageParam) : 1;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { proposals: all, source, fallback, addressById, asOf } = await loadProposals({
    signal: request.signal,
  });

  const filtered = filterProposals(all, category, status, search);
  const countScope = search ? filterProposals(all, "", "", search) : all;

  const totalFiltered = filtered.length;
  const pageCount = Math.max(1, Math.ceil(totalFiltered / PAGE_SIZE));
  const page = Math.min(requestedPage, pageCount);
  const start = (page - 1) * PAGE_SIZE;
  const pageItems = filtered.slice(start, start + PAGE_SIZE);

  let withAuthors = pageItems;
  try {
    const addresses = [
      ...new Set(
        pageItems
          .map((p) => addressById[p.id])
          .filter((a): a is string => Boolean(a)),
      ),
    ];
    const profiles = await fetchAuthorProfiles(addresses, {
      signal: request.signal,
    });
    if (profiles) withAuthors = applyAuthorLabels(pageItems, profiles, addressById);
  } catch {
    withAuthors = pageItems;
  }

  const payload = {
    sid,
    category,
    status,
    search,
    proposals: withAuthors,
    page,
    pageCount,
    pageSize: PAGE_SIZE,
    totalFiltered,
    total: all.length,
    categoryCounts: categoryCountsOf(countScope),
    source,
    fallback,
    asOf,
  };

  return wrap(payload);
}

export default function GovernanceProposalsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  const asOfLabel = governanceAsOfLabel(d.asOf);

  return (
    <>
      {asOfLabel && (
        <GovernanceNotice
          tone="stale"
          title={`Newest proposal on this node: ${asOfLabel}`}
          detail={GOVERNANCE_MIRROR_NOTE}
        />
      )}
      <ProposalsList
        sid={d.sid}
        proposals={d.proposals}
        category={d.category}
        status={d.status}
        search={d.search}
        page={d.page}
        pageCount={d.pageCount}
        totalFiltered={d.totalFiltered}
        total={d.total}
        categoryCounts={d.categoryCounts}
        fallback={d.fallback}
      />
    </>
  );
}
