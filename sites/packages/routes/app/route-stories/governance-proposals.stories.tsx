import GovernanceProposalsRoute from "../routes/governance.proposals";
import proposalsFx from "@data/fixtures/route-governance-proposals.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

type Card = (typeof proposalsFx.corpus)[number];
const corpus: Card[] = proposalsFx.corpus;

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
const PAGE_SIZE = 36;

function filterProposals(
  all: Card[],
  category: string,
  status: string,
  search: string,
): Card[] {
  const q = search.toLowerCase();
  const members = category ? (CATEGORY_MEMBERS[category] ?? [category]) : null;
  return all.filter((p) => {
    if (members && !members.includes(p.category)) return false;
    if (status && p.status !== status) return false;
    if (q && !p.title.toLowerCase().includes(q)) return false;
    return true;
  });
}

function categoryCountsOf(all: Card[]): Record<string, number> {
  const counts: Record<string, number> = { all: all.length };
  for (const id of SIDEBAR_CATEGORY_IDS) {
    const members = CATEGORY_MEMBERS[id] ?? [id];
    counts[id] = all.filter((p) => members.includes(p.category)).length;
  }
  return counts;
}

function proposalsStubLoader({ request }: { request: Request }) {
  const url = new URL(request.url);
  const category = url.searchParams.get("category")?.trim() ?? "";
  const status = url.searchParams.get("status")?.trim() ?? "";
  const search = url.searchParams.get("search")?.trim() ?? "";
  const pageParam = Number(url.searchParams.get("page"));
  const requestedPage =
    Number.isFinite(pageParam) && pageParam >= 1 ? Math.floor(pageParam) : 1;

  const filtered = filterProposals(corpus, category, status, search);
  const countScope = search ? filterProposals(corpus, "", "", search) : corpus;
  const totalFiltered = filtered.length;
  const pageCount = Math.max(1, Math.ceil(totalFiltered / PAGE_SIZE));
  const page = Math.min(requestedPage, pageCount);
  const start = (page - 1) * PAGE_SIZE;

  return {
    sid: proposalsFx.sid,
    category,
    status,
    search,
    proposals: filtered.slice(start, start + PAGE_SIZE),
    page,
    pageCount,
    pageSize: PAGE_SIZE,
    totalFiltered,
    total: corpus.length,
    categoryCounts: categoryCountsOf(countScope),
    source: "live",
    fallback: false,
  };
}

const at = (url: string) =>
  proposalsStubLoader({ request: new Request(`http://story.local${url}`) });

export default {
  title: "Routes/GovernanceProposals",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Default = {
  render: routeStory({
    Component: GovernanceProposalsRoute,
    path: "/governance/proposals",
    loaderData: at("/governance/proposals"),
    loader: proposalsStubLoader,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain(corpus[0].title);
    });
  },
};

const firstEnacted = corpus.find((p) => p.status === "enacted");

export const EnactedOnly = {
  render: routeStory({
    Component: GovernanceProposalsRoute,
    path: "/governance/proposals",
    url: "/governance/proposals?status=enacted",
    loaderData: at("/governance/proposals?status=enacted"),
    loader: proposalsStubLoader,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain(firstEnacted?.title ?? "");
    });
  },
};

export const SearchNoResults = {
  render: routeStory({
    Component: GovernanceProposalsRoute,
    path: "/governance/proposals",
    url: "/governance/proposals?search=zzzz-no-such-proposal",
    loaderData: at("/governance/proposals?search=zzzz-no-such-proposal"),
    loader: proposalsStubLoader,
  }),
};

export const ApiDown = {
  render: routeStory({
    Component: GovernanceProposalsRoute,
    path: "/governance/proposals",
    loaderData: {
      sid: proposalsFx.sid,
      category: "",
      status: "",
      search: "",
      proposals: [],
      page: 1,
      pageCount: 1,
      pageSize: PAGE_SIZE,
      totalFiltered: 0,
      total: 0,
      categoryCounts: categoryCountsOf([]),
      source: "error",
      fallback: true,
    },
  }),
};
