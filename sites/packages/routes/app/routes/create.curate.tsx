import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import EmptyState from "@ui/components/EmptyState";
import "@ui/creatorhub/frames/creatorhubchrome.css";

import { useAuth } from "@data/lib/auth/index";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import {
  filterRows,
  readAssigneeFilter,
  readStatusFilter,
  readTypeFilter,
  toBdRow,
  type BdCommitteeRow,
  type CommitteeMember,
} from "@data/lib/catalyst/creator-hub/curate-committee";
import { loadCommitteeCuration } from "@data/lib/catalyst/creator-hub/curate-committee.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import CurationWizard from "@features/stories/creator-hub/curate-committee/CurationWizard";
import type { CurationFilters, DecisionStatus } from "@features/stories/creator-hub/curate-committee/machine";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.curate";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Curate");

const STORY: StoryId = "creator-hub/curate-committee";

const FALLBACK: Assignment = {
  variant: "comments",
  flags: { comments: true, commentRequiredOnReject: false },
  experimentKey: "bd_curation_comments",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const activeId = url.searchParams.get("id")?.trim() || null;
  const decisionRaw = url.searchParams.get("decision")?.trim();
  const decision: DecisionStatus | undefined =
    decisionRaw === "rejected" ? "rejected" : decisionRaw === "approved" ? "approved" : undefined;

  const youParam =
    url.searchParams.get("address")?.trim().toLowerCase() ||
    readWallet(request) ||
    null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { committee, allRows, usedFixture, error, usedFallback } =
    await loadCommitteeCuration({
      youAddress: youParam,
      isCommittee: true,
      signal: request.signal,
    });

  const isCommittee =
    !!youParam &&
    committee.members.some((m) => m.address === youParam.toLowerCase());

  const filters: CurationFilters = {
    status: readStatusFilter(url.searchParams.get("status")),
    type: readTypeFilter(url.searchParams.get("type")),
    assignee: readAssigneeFilter(
      url.searchParams.get("assignee"),
      committee.you.address,
    ),
  };

  const rows = filterRows(allRows, filters);
  const now = Date.now();
  const bdRows: BdCommitteeRow[] = rows.map((r) => toBdRow(r, committee, now));

  const payload = {
    sid,
    step,
    activeId,
    decision,
    isCommittee,
    filters,
    assignment,
    committee,
    rows: bdRows,
    totalCount: allRows.length,
    usedFixture,
    error,
    usedFallback,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  step: string | null;
  activeId: string | null;
  decision: DecisionStatus | undefined;
  isCommittee: boolean;
  filters: CurationFilters;
  assignment: Assignment;
  committee: { you: CommitteeMember; members: CommitteeMember[] };
  rows: BdCommitteeRow[];
  totalCount: number;
  usedFixture: boolean;
  error: boolean;
  usedFallback: boolean;
};

const ShieldGlyph = (
  <svg
    viewBox="0 0 24 24"
    width="34"
    height="34"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M12 3 5 6v5c0 4.5 3 7.5 7 9 4-1.5 7-4.5 7-9V6l-7-3Z" />
    <path d="m9.4 12 1.9 1.9 3.3-3.7" />
  </svg>
);

export default function CreateCurate({ loaderData }: Route.ComponentProps) {
  const d = loaderData as unknown as LoaderData;
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const signIn = () => {
    track("ch_curate_signin_clicked", {}, { sid: d.sid, story: STORY });
    openSignIn();
  };

  const me = (address ?? "").toLowerCase();
  const isMember =
    !!me && d.committee.members.some((m) => m.address.toLowerCase() === me);
  const allowed = d.isCommittee && isConnected && isMember;

  return (
    <CreatorHubChrome
      active="curate"
      committee={d.isCommittee}
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={signIn}
    >
      <section className="curate-committee-route">
        {d.error ? (
          <p
            className="curate-committee-route__sim"
            role="alert"
            style={{
              color: "color-mix(in srgb, var(--error) 45%, var(--text))",
              background: "color-mix(in srgb, var(--error) 12%, transparent)",
              borderBottomColor:
                "color-mix(in srgb, var(--error) 35%, transparent)",
            }}
          >
            <strong style={{ color: "var(--text)" }}>
              Couldn&apos;t reach the curation queue.
            </strong>{" "}
            Please try again shortly.
          </p>
        ) : null}
        {allowed ? (
          <CurationWizard
            rows={d.rows}
            totalCount={d.totalCount}
            committee={d.committee}
            isCommittee={d.isCommittee}
            connectedAddress={address ?? ""}
            onConnect={() => openSignIn()}
            filters={d.filters}
            trackCtx={{
              sid: d.sid,
              story: STORY,
              variant: d.assignment.variant,
              experimentKey: d.assignment.experimentKey,
            }}
            initialStep={d.step ?? undefined}
            initialActiveId={d.activeId ?? undefined}
            initialDecision={d.decision}
          />
        ) : isConnected ? (
          <EmptyState
            icon={ShieldGlyph}
            iconWash
            title="Committee access only"
            subtitle="This wallet isn't on the curation committee. Reviewing collections is reserved for committee members."
            actions={[{ label: "Back to Creator Hub", href: "/create" }]}
            variant={undefined}
            tone={undefined}
            actionsGap={undefined}
            style={undefined}
          />
        ) : (
          <EmptyState
            icon={ShieldGlyph}
            iconWash
            title="Curation is for committee members"
            subtitle="Connect the committee wallet to review collections in the curation queue."
            actions={[{ label: "Sign in", onClick: signIn }]}
            variant={undefined}
            tone={undefined}
            actionsGap={undefined}
            style={undefined}
          />
        )}
      </section>
    </CreatorHubChrome>
  );
}
