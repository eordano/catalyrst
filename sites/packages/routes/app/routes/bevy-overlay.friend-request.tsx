import { useEffect } from "react";

import {
  loadFriendRequest,
  fixtureCandidateAddress,
  type FriendRequestData,
} from "@data/lib/catalyst/overlay/friend-request.server";
import { isEthAddress, normalizeAddress } from "@data/lib/catalyst/overlay/backpack";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import ClientStage from "@ui/overlay/panels/ClientStage";
import FriendRequestWizard, {
  FRIEND_EVENTS,
} from "@features/stories/overlay/friend-request/FriendRequestWizard";

import type { Route } from "./+types/bevy-overlay.friend-request";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/friend-request";

function parseTab(raw: string | null): "friends" | "requests" | "blocked" {
  return raw === "requests" || raw === "blocked" ? raw : "friends";
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "ov_friend_request",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const rawAddr = url.searchParams.get("address");
  const address =
    rawAddr && isEthAddress(rawAddr) ? normalizeAddress(rawAddr) : fixtureCandidateAddress();
  const tab = parseTab(url.searchParams.get("tab"));
  const action = url.searchParams.get("action") ?? null;

  let panel: FriendRequestData;
  try {
    panel = loadFriendRequest(address);
  } catch {
    panel = {
      self: { address: "0x0", name: "you", hasClaimedName: false, isGuest: false },
      friends: [],
      received: [],
      sent: [],
      blocked: [],
      candidate: {
        address,
        name: "user",
        hasClaimedName: false,
        profilePictureUrl: "",
        mutualCount: 0,
        friendshipStatus: "none",
      },
      counts: { friends: 0, received: 0, sent: 0, blocked: 0 },
    };
  }

  const payload = { sid, panel, tab, action, assignment };

  return wrap(payload);
}

export default function FriendRequestRoute({ loaderData }: Route.ComponentProps) {
  const { sid, panel, tab, action, assignment } = loaderData;

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  useEffect(() => {
    track(FRIEND_EVENTS.panelOpened, { tab }, trackCtx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <ClientStage nojs="Enable JavaScript to send, accept, cancel and block friend requests.">
      <FriendRequestWizard
        trackCtx={trackCtx}
        candidate={panel.candidate}
        initialAction={action ?? undefined}
        tab={tab}
      />
    </ClientStage>
  );
}
