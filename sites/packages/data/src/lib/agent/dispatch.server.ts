import type { EntryContext } from "react-router";

import { LEGAL_DOCS } from "@ui/data/legalPageConfig";

import {
  assetDetailToMarkdown,
  blogIndexToMarkdown,
  blogPostToMarkdown,
  collectionToMarkdown,
  communityToMarkdown,
  discoverToMarkdown,
  eventDetailToMarkdown,
  governanceLandingToMarkdown,
  governanceProjectsToMarkdown,
  governanceProposalsToMarkdown,
  homeToMarkdown,
  legalDocToMarkdown,
  placeToMarkdown,
  placesIndexToMarkdown,
  profileToMarkdown,
  projectDetailToMarkdown,
  proposalDetailToMarkdown,
  transparencyToMarkdown,
  whatsOnToMarkdown,
  type AgentLegalDoc,
  type AgentMarkdownKey,
} from "./markdown";

const SERIALIZERS: Record<AgentMarkdownKey, (data: unknown) => string | null> = {
  blogIndex: (data) => {
    const d = data as { posts?: Parameters<typeof blogIndexToMarkdown>[0] } | undefined;
    return blogIndexToMarkdown(d?.posts ?? []);
  },
  blogPost: (data) => {
    const d = data as { post?: Parameters<typeof blogPostToMarkdown>[0] | null } | undefined;
    return d?.post ? blogPostToMarkdown(d.post) : null;
  },
  legalDoc: (data) => {
    const d = data as { doc?: string } | undefined;
    const doc = d?.doc ? (LEGAL_DOCS as Record<string, AgentLegalDoc>)[d.doc] : undefined;
    return doc ? legalDocToMarkdown(doc) : null;
  },
  placesIndex: (data) => {
    const d = data as { places?: Parameters<typeof placesIndexToMarkdown>[0] } | undefined;
    return placesIndexToMarkdown(d?.places ?? []);
  },
  placeDetail: (data) => {
    const d = data as { place?: Parameters<typeof placeToMarkdown>[0] | null } | undefined;
    return d?.place ? placeToMarkdown(d.place) : null;
  },
  home: (data) => {
    const d = data as { story?: Parameters<typeof homeToMarkdown>[0] | null } | undefined;
    return d?.story ? homeToMarkdown(d.story) : null;
  },
  governanceLanding: (data) => {
    const d = data as
      | { endingSoon?: Parameters<typeof governanceLandingToMarkdown>[0] }
      | undefined;
    return governanceLandingToMarkdown(d?.endingSoon ?? []);
  },
  governanceProposals: (data) => {
    const d = data as Parameters<typeof governanceProposalsToMarkdown>[0] | undefined;
    return d ? governanceProposalsToMarkdown(d) : null;
  },
  proposalDetail: (data) => {
    const d = data as { proposal?: Parameters<typeof proposalDetailToMarkdown>[0] | null } | undefined;
    return d?.proposal ? proposalDetailToMarkdown(d.proposal) : null;
  },
  whatsOn: (data) => {
    const d = data as Parameters<typeof whatsOnToMarkdown>[0] | undefined;
    return d ? whatsOnToMarkdown(d) : null;
  },
  eventDetail: (data) => {
    const d = data as { event?: Parameters<typeof eventDetailToMarkdown>[0] | null } | undefined;
    return d?.event ? eventDetailToMarkdown(d.event) : null;
  },
  governanceProjects: (data) => {
    const d = data as Parameters<typeof governanceProjectsToMarkdown>[0] | undefined;
    return d ? governanceProjectsToMarkdown(d) : null;
  },
  projectDetail: (data) => {
    const d = data as { project?: Parameters<typeof projectDetailToMarkdown>[0] | null } | undefined;
    return d?.project ? projectDetailToMarkdown(d.project) : null;
  },
  transparency: (data) => {
    const d = data as { transparency?: Parameters<typeof transparencyToMarkdown>[0] | null } | undefined;
    return d?.transparency ? transparencyToMarkdown(d.transparency) : null;
  },
  assetDetail: (data) => {
    const d = data as
      | { nft?: Parameters<typeof assetDetailToMarkdown>[0] | null; listings?: Parameters<typeof assetDetailToMarkdown>[1] }
      | undefined;
    return d?.nft ? assetDetailToMarkdown(d.nft, d.listings ?? []) : null;
  },
  profileDetail: (data) => {
    const d = data as { profile?: Parameters<typeof profileToMarkdown>[0] | null } | undefined;
    return d?.profile ? profileToMarkdown(d.profile) : null;
  },
  discover: (data) => {
    const d = data as { content?: Parameters<typeof discoverToMarkdown>[0] | null } | undefined;
    return d?.content ? discoverToMarkdown(d.content) : null;
  },
  collectionDetail: (data) => {
    const d = data as Parameters<typeof collectionToMarkdown>[0] | undefined;
    return d?.header ? collectionToMarkdown(d) : null;
  },
  communityDetail: (data) => {
    const d = data as { detail?: Parameters<typeof communityToMarkdown>[0] | null } | undefined;
    return d?.detail && d.detail.source === "live" ? communityToMarkdown(d.detail) : null;
  },
};

type MatchLike = { route: { id: string; handle?: unknown } };

export function renderAgentMarkdown(
  routerContext: EntryContext,
): { md: string; status: number } | null {
  const ctx = routerContext.staticHandlerContext;
  if (!ctx) return null;

  const matches = (ctx.matches ?? []) as unknown as MatchLike[];
  const loaderData = ctx.loaderData as Record<string, unknown>;

  for (let i = matches.length - 1; i >= 0; i--) {
    const route = matches[i].route;
    const handle = route.handle as { agentMarkdown?: AgentMarkdownKey } | undefined;
    const key = handle?.agentMarkdown;
    if (!key) continue;

    const serialize = SERIALIZERS[key];
    if (!serialize) return null;

    const md = serialize(loaderData[route.id]);
    if (md == null) return null;
    return { md, status: ctx.statusCode ?? 200 };
  }

  return null;
}
