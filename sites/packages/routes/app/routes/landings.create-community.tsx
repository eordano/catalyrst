import { Link } from "react-router";

import LdCreateCommunityView from "@ui/landings/pages/LdCreateCommunityView";

import { loadCommunity, type Community } from "@data/lib/catalyst/overlay/communities";
import {
  emptyDraft,
  type CommunityDraft,
  type OwnedPlace,
} from "@data/lib/catalyst/overlay/create-community";
import type { CreateFn } from "@features/stories/landings/create-community/machine";
import { createCommunity } from "@data/lib/catalyst/overlay/community-commit";
import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CreateCommunity from "@features/stories/landings/create-community/CreateCommunity";

import type { Route } from "./+types/landings.create-community";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/create-community";

type DraftSource = "live" | "empty" | "error";

function fixturePlaces(): OwnedPlace[] {
  return [];
}

function draftFromCommunity(c: Community): CommunityDraft {
  const thumb = c.thumbnailUrl && c.thumbnailUrl !== "N/A" ? c.thumbnailUrl : null;
  return {
    ...emptyDraft(),
    name: c.name,
    description: c.description ?? "",
    privacy: c.privacy,
    // The list response strips `visibility` for an unsigned read. Seeding the
    // form with "all" would re-assert a listing setting nobody read back, so an
    // unknown visibility leaves the draft's own default for the user to choose.
    ...(c.visibility ? { visibility: c.visibility } : {}),
    hasThumbnail: Boolean(thumb),
    thumbnailPreviewUrl: thumb,
    placeIds: [],
  };
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "lp_community_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const mode = url.searchParams.get("mode") === "edit" ? "edit" : "create";
  const editId = url.searchParams.get("id");

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let draft: CommunityDraft | null = null;
  let source: DraftSource = "empty";
  if (mode === "edit") {
    if (editId) {
      try {
        const detail = await loadCommunity(editId, { signal: request.signal });
        if (detail && detail.source === "live") {
          draft = draftFromCommunity(detail.community);
          source = "live";
        } else {
          draft = emptyDraft();
          source = "error";
        }
      } catch {
        draft = emptyDraft();
        source = "error";
      }
    } else {
      draft = emptyDraft();
      source = "empty";
    }
  }

  const payload = {
    sid,
    mode,
    source,
    editId: editId ?? null,
    ownedPlaces: fixturePlaces(),
    draft,
    assignment,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  mode: "create" | "edit";
  source: DraftSource;
  editId: string | null;
  ownedPlaces: OwnedPlace[];
  draft: CommunityDraft | null;
  assignment: Assignment;
};

export default function CreateCommunityRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;
  const editLink = d.editId ? `?mode=edit&id=${encodeURIComponent(d.editId)}` : "?mode=edit";
  const { identity } = useAuth();
  const create: CreateFn | undefined = identity
    ? async ({ draft, signal }) => {
        const c = await createCommunity(draft, null, { identity, signal });
        return {
          id: c.id,
          name: c.name,
          privacy: draft.privacy,
          visibility: draft.visibility,
        };
      }
    : undefined;

  return (
    <LdCreateCommunityView mode={d.mode} source={d.source} editHref={editLink} LinkComponent={Link}>
      <CreateCommunity
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        ownedPlaces={d.ownedPlaces}
        draft={d.draft ?? undefined}
        create={create}
      />
    </LdCreateCommunityView>
  );
}
