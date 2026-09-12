import AdControlNotice from "@ui/admin/pages/AdControlNotice";
import SitesChrome from "@ui/web/frames/SitesChrome";

import {
  loadModerationCommunities,
  type CommunityModerationCard,
} from "@data/lib/catalyst/admin/community-moderation";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import ModerateCommunitiesWizard from "@features/stories/admin/communities-moderation/ModerateCommunitiesWizard";

import type { Route } from "./+types/admin.communities-moderation";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/communities-moderation";

const FALLBACK: Assignment = {
  variant: "moderation_console",
  flags: { moderation_console: true },
  experimentKey: "admin_communities_moderation",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const search = url.searchParams.get("search")?.trim() || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const list = await loadModerationCommunities(
    { search: search || undefined, limit: 50 },
    { signal: request.signal },
  );
  const cards: CommunityModerationCard[] = list.cards;

  const payload = { sid, step, cards, source: list.source, assignment };

  return wrap(payload);
}

export default function AdminCommunitiesModerationRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <SitesChrome active="play">
      {d.source === "error" && (
        <AdControlNotice
          title="The community list could not be read"
          message={
            "This is a public, unauthenticated read and it failed. No communities " +
            "are shown, because an empty list and a failed read are not the same " +
            "thing."
          }
          serverCheck={"catalyrst-social-service/src/rest/handlers/communities.rs:176-177 (try_extract_signer, optional \u{2014} no gate)"}
        />
      )}
      <ModerateCommunitiesWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        cards={d.cards}
        initialStep={d.step ?? undefined}
      />
    </SitesChrome>
  );
}
