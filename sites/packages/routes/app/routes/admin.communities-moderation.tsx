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

/**
 * Two different data paths on one page, and they must be labelled differently.
 *
 * The community LIST is public: `catalyrst-social-service`
 * `src/rest/handlers/communities.rs:176-177` uses `try_extract_signer`, which is
 * optional, and gates nothing. Anyone gets the same list. It is not admin data
 * and this page does not claim it is.
 *
 * Suspend / unsuspend is the one already-correct privileged write in this
 * surface, and its mechanism is deliberately untouched. The browser posts to the
 * `/admin/community-suspension` resource route; that action calls
 * `community-moderation.server.ts#submitSuspension`, which holds
 * `API_ADMIN_TOKEN` server-side and 503s when it is unset. Server side,
 * `catalyrst-social-service/src/rest/handlers/admin.rs` `require_admin`
 * answers 403 "admin controls disabled (API_ADMIN_TOKEN unset)" when
 * `state.admin_token` is `None`, compares timing-safely, and is called first by
 * both `suspend_community` and `unsuspend_community`.
 *
 * The only change here: a failed list read no longer renders as an empty list.
 * `loadModerationCommunities` already distinguished `"error"` from `"empty"`;
 * the route was throwing that distinction away.
 */

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
