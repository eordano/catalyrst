import {
  emptyDraft,
  noName,
  ownsName,
  type CommunityDraft,
  type CommunityPrivacy,
  type CommunityVisibility,
} from "@data/lib/catalyst/overlay/community-create";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CommunityCreateOverlay from "@features/stories/overlay/community-create/CommunityCreateOverlay";

import fixture from "@data/fixtures/bevy-overlay-community-create.json";

import type { Route } from "./+types/bevy-overlay.community-create";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/community-create";

type MembershipOption = { value: CommunityPrivacy; label: string; note: string };

function fixtureMembership(): MembershipOption[] {
  return fixture.membershipOptions.map((o) => ({
    value: o.value === "private" ? "private" : "public",
    label: o.label,
    note: o.note,
  }));
}

function fixtureDraft(): CommunityDraft {
  const s = fixture.sampleDraft;
  return {
    ...emptyDraft(),
    name: s.name,
    description: s.description,
    privacy: s.privacy as CommunityPrivacy,
    visibility: s.visibility as CommunityVisibility,
    hasThumbnail: s.hasThumbnail,
    policyAck: s.policyAck,
  };
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "cl_community_create_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const mode = url.searchParams.get("mode") === "edit" ? "edit" : "create";
  const owned = url.searchParams.get("owned") === "1";
  const name = owned ? ownsName() : noName();

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = {
    sid,
    mode,
    hasName: name.hasName,
    membershipOptions: fixtureMembership(),
    draft: mode === "edit" ? fixtureDraft() : null,
    assignment,
  };

  return wrap(payload);
}

export default function CommunityCreateRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <main className="community-create-route">
      <CommunityCreateOverlay
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        hasName={d.hasName}
        membershipOptions={d.membershipOptions}
        draft={d.draft ?? undefined}
      />
    </main>
  );
}
