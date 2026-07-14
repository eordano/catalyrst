import {
  loadCommunities,
  loadCommunityDetail,
  type CommunityRow,
  type CommunityJoinDetail,
} from "@data/lib/catalyst/overlay/community-join";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CommunityJoinWizard from "@features/stories/overlay/community-join/CommunityJoinWizard";
import type { CommitFn } from "@data/lib/catalyst/overlay/community-join";
import { commitCommunityJoin } from "@data/lib/catalyst/overlay/community-commit";
import { useAuth } from "@data/lib/auth/index";

import type { Route } from "./+types/bevy-overlay.community-join";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/community-join";

const FALLBACK: Assignment = {
  variant: "guided",
  flags: { wizard: true },
  experimentKey: "cl_community_join",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const search = url.searchParams.get("q")?.trim() ?? "";
  const selectId = url.searchParams.get("select")?.trim() ?? "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let rows: CommunityRow[] = [];
  let total = 0;
  let source: "live" | "fixture" = "live";
  try {
    const res = await loadCommunities({ search, signal: request.signal });
    rows = res.rows;
    total = res.total;
    source = res.source;
  } catch {
    rows = [];
  }

  let selected: CommunityJoinDetail | null = null;
  if (selectId) {
    try {
      selected = await loadCommunityDetail(selectId, { signal: request.signal });
    } catch {
      selected = null;
    }
  }

  const payload = { sid, search, rows, total, selected, source, assignment };
  return wrap(payload);
}

export default function CommunityJoinRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const { identity } = useAuth();
  const commit: CommitFn | undefined = identity
    ? ({ communityId, action, signal }) =>
        commitCommunityJoin({ communityId, action }, { identity, signal })
    : undefined;
  return (
    <main className="community-join-route">
      <CommunityJoinWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        rows={d.rows}
        total={d.total}
        selected={d.selected}
        search={d.search}
        source={d.source}
        commit={commit}
      />
    </main>
  );
}
