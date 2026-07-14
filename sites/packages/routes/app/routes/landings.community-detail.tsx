import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import StSocialCommunityDetail from "@ui/web/pages/StSocialCommunityDetail";
import "@ui/web/pages/stsocialcommunitydetail.css";

import {
  loadCommunity,
  loadDefaultCommunity,
  type CommunityDetail,
} from "@data/lib/catalyst/overlay/communities";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/landings.community-detail";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "communityDetail" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "landings/community-detail";

type Tab = "members" | "events";

function parseTab(raw: string | null): Tab {
  return raw === "events" ? "events" : "members";
}

const FALLBACK: Assignment = {
  variant: "full_profile",
  flags: { showMembersRail: true, showEventsGrid: true },
  experimentKey: "lp_community_detail",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  let id = url.searchParams.get("id")?.trim() || "";
  const tab = parseTab(url.searchParams.get("tab"));
  const forcePrivate = url.searchParams.get("gate") === "private";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  if (!id) {
    const def = await loadDefaultCommunity({ signal: request.signal });
    id = def?.id ?? "";
  }

  let detail: CommunityDetail | null = null;
  if (id) {
    try {
      detail = await loadCommunity(id, { signal: request.signal });
    } catch {
      detail = null;
    }
  }

  const payload = { id, tab, forcePrivate, detail, sid };
  return wrap(payload);
}

export default function CommunityDetailRoute({ loaderData }: Route.ComponentProps) {
  const { id, tab, forcePrivate, detail, sid } = loaderData;

  if (!detail) {
    return <StSocialCommunityDetail {...({ state: "notFound", membersTotal: 0 } as React.ComponentProps<typeof StSocialCommunityDetail>)} />;
  }

  return (
    <CommunityLanding
      id={id}
      tab={tab}
      forcePrivate={forcePrivate}
      detail={detail}
      sid={sid}
    />
  );
}

type LandingProps = {
  id: string;
  tab: Tab;
  forcePrivate: boolean;
  detail: CommunityDetail;
  sid: string;
};

function CommunityLanding({ id, tab, forcePrivate, detail, sid }: LandingProps) {
  const [, setSearchParams] = useSearchParams();
  const { community, members, events, source } = detail;

  const isPrivate = community.privacy === "private";
  // A response with no `role` key was not asked on this reader's behalf --
  // the social service strips it for anonymous requests. Unknown is not
  // membership, so it must not unlock a private community.
  const isMember =
    community.role != null && community.role !== "none" && community.role !== "";
  const gated = forcePrivate || (isPrivate && !isMember);

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "lp_community_viewed",
      {
        community_id: community.id,
        privacy: community.privacy,
        members_count: community.membersCount,
        source,
      },
      { sid, story: STORY },
    );
    if (gated) {
      track(
        "lp_community_private_gated",
        { community_id: community.id },
        { sid, story: STORY },
      );
    }
  }, [community.id, community.privacy, community.membersCount, source, gated, sid]);

  function onTab(next: Tab) {
    if (next === tab) return;
    track(
      "lp_community_tab_changed",
      { community_id: community.id, tab: next },
      { sid, story: STORY },
    );
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("tab", next);
        return params;
      },
      { preventScrollReset: true },
    );
  }

  function onJoinIntent() {
    const intent = isMember
      ? "joined"
      : isPrivate
        ? "request"
        : "join";
    track(
      "lp_community_join_intent",
      { community_id: community.id, privacy: community.privacy, intent },
      { sid, story: STORY },
    );
  }

  const vmCommunity = {
    id: community.id,
    name: community.name,
    description: community.description ?? "",
    ownerAddress: community.ownerAddress,
    ownerName: community.ownerName ?? undefined,
    ownerProfilePicture: "",
    privacy: community.privacy,
    membersCount: community.membersCount,
    thumbnail:
      community.thumbnailUrl && community.thumbnailUrl !== "N/A"
        ? community.thumbnailUrl
        : "",
    role: community.role ?? "",
  };
  const vmMembers = members.map((m) => ({
    memberAddress: m.memberAddress,
    name: m.name || m.memberAddress,
    role: m.role,
    hasClaimedName: m.hasClaimedName,
    profilePictureUrl: m.profilePictureUrl || undefined,
  }));
  const vmEvents = events.map((e) => ({
    id: e.id,
    name: e.name,
    image: e.image || undefined,
    creatorName: e.creatorName ?? "",
    timeLabel: e.timeLabel ?? "",
  }));

  useTabScroll(tab, gated);

  return (
    <main className="community-detail-route">
      <StSocialCommunityDetail
        {...({
          community: vmCommunity,
          members: vmMembers,
          events: vmEvents,
          membersTotal: community.membersCount,
          isLoggedIn: isMember,
          isMember,
          hasPendingRequest: false,
          ...(forcePrivate
            ? { community: { ...vmCommunity, privacy: "private" }, isMember: false, isLoggedIn: false }
            : null),
          state: "default",
        } as React.ComponentProps<typeof StSocialCommunityDetail>)}
      />
    </main>
  );
}

const STEPS_WRAP: React.CSSProperties = {
  display: "flex",
  gap: 8,
  alignItems: "center",
  justifyContent: "center",
  flexWrap: "wrap",
  padding: "16px 24px 4px",
};
const CRUMB_WRAP: React.CSSProperties = {
  textAlign: "center",
  padding: "4px 24px 32px",
  color: "#fff",
  fontSize: 13,
};
const CTA_STYLE: React.CSSProperties = {
  border: "none",
  borderRadius: 8,
  padding: "8px 18px",
  fontWeight: 700,
  cursor: "pointer",
  background: "var(--brand-cta)",
  color: "#fff",
};
function tabStyle(active: boolean): React.CSSProperties {
  return {
    border: "1px solid rgba(255,255,255,.25)",
    borderRadius: 8,
    padding: "8px 16px",
    fontWeight: 600,
    cursor: "pointer",
    background: active ? "rgba(255,255,255,.16)" : "transparent",
    color: "#fff",
  };
}

function useTabScroll(tab: Tab, gated: boolean) {
  useEffect(() => {
    if (gated || typeof document === "undefined") return;
    const sel = tab === "events" ? ".stscd__events-col" : ".stscd__members-col";
    const el = document.querySelector(sel);
    el?.scrollIntoView?.({ block: "nearest" });
  }, [tab, gated]);
}
