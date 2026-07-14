import type { ComponentType } from "react";
import { createRoutesStub, useLocation } from "react-router";

import GovernanceProposalDetailRoute from "../routes/governance.proposals_.$id";
import detailFx from "@data/fixtures/route-governance-proposal-detail.json";
import { expect, waitFor } from "@ui/docs/sb";

const ROUTE_ID = "routes/governance.proposals_.$id";

type StubComponent = ComponentType<Record<string, unknown>>;

function EmptyFallback() {
  return null;
}

function NavTarget() {
  const location = useLocation();
  return (
    <div style={{ padding: 32, font: "14px/1.5 monospace" }}>
      <p>
        story stub &#x2014; navigated to <strong>{location.pathname + location.search}</strong>
      </p>
    </div>
  );
}

function detailStory(payload: unknown, url: string) {
  const Stub = createRoutesStub([
    {
      id: ROUTE_ID,
      path: "/governance/proposals/:id",
      Component: GovernanceProposalDetailRoute as unknown as StubComponent,
      loader: () => payload,
      HydrateFallback: EmptyFallback,
    },
    { id: "story-catchall", path: "*", Component: NavTarget },
  ]);
  return function DetailRouteStory() {
    return (
      <Stub
        initialEntries={[url]}
        hydrationData={{ loaderData: { [ROUTE_ID]: payload } }}
      />
    );
  };
}

export default {
  title: "Routes/GovernanceProposalDetail",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const FinishedWithVotes = {
  render: detailStory(
    detailFx.finished,
    `/governance/proposals/${detailFx.finished.id}`,
  ),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain(detailFx.finished.proposal.title);
      expect(canvasElement.textContent).toContain("total votes");
    });
  },
};

export const ActiveVoteFlow = {
  render: detailStory(
    detailFx.active,
    `/governance/proposals/${detailFx.active.id}`,
  ),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain(detailFx.active.proposal.title);
      expect(
        canvasElement.querySelector('[aria-label="Cast your vote"]'),
      ).not.toBeNull();
    });
  },
};

export const ActiveGuidedVoteFlow = {
  render: detailStory(
    {
      ...detailFx.active,
      assignment: {
        variant: "guided",
        flags: { guided: true },
        experimentKey: "gv_vote_flow",
      },
    },
    `/governance/proposals/${detailFx.active.id}`,
  ),
};

export const NotFound = {
  render: detailStory(
    {
      id: "does-not-exist",
      proposal: null,
      votes: null,
      comments: [],
      commentsTotal: 0,
      sid: detailFx.finished.sid,
      assignment: detailFx.finished.assignment,
      authorName: null,
    },
    "/governance/proposals/does-not-exist",
  ),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Proposal not found");
    });
  },
};
