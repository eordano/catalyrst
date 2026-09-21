import type { QueryClient } from "@tanstack/react-query";
import { useQueries } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import Communities from "../../explorer/pages/Communities";
import type { SocialCommunity, SocialDetail, SocialMembership } from "../../explorer/pages/Communities";
import CommunityCreate from "../../explorer/components/CommunityCreate";
import { siteUrl } from "../../data/site";
import { publicThumbnail } from "../../data/thumbnail";
import { fetchPlace } from "../../data/catalyst/placesSchema";
import { qk, STALE } from "../../data/queryKeys";
import { PLAY_UPCOMING_PARAMS } from "../../data/screens/play";
import {
  useCommunities,
  communitiesQuery,
  useCommunity,
  useCommunityPosts,
  useCommunityPlaces,
  useJoinCommunity,
  useLeaveCommunity,
} from "../../data/hooks/useCommunities";
import { useCommunityProfiles } from "../../data/hooks/useCommunityProfiles";
import { useEvents } from "../../data/hooks/useEvents";
import { useFriends } from "../../data/hooks/useFriends";
import { requestFriendAction } from "../../data/hooks/friendActions";
import { requestDirectMessage } from "../../explorer/frames/chatIntent";
import { useBridgeState } from "../../overlay/bridge";

type MaybeHttpError = { status?: number; message?: string } | null | undefined;

export function prefetch(queryClient: QueryClient, address?: string | null) {
  return queryClient.prefetchQuery(communitiesQuery({}, address));
}

function joinErrorText(err: MaybeHttpError): string {
  const status = err?.status ?? 0;
  if (status === 401 || status === 403 || status === 501) {
    return "Joining isn\u{2019}t available on this realm yet";
  }
  if (status === 0) return "Couldn\u{2019}t reach the server \u{2014} try again";
  return err?.message || "Couldn\u{2019}t join \u{2014} please try again";
}

export default function CommunitiesPanel() {
  const identity = useBridgeState((s) => s.identity);
  const players = useBridgeState((s) => s.players);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [override, setOverride] = useState<{ id: string; state: "joined" | "left" | "requested" } | null>(null);

  const list = useCommunities();
  const detail = useCommunity(selectedId);
  const posts = useCommunityPosts(selectedId);
  const places = useCommunityPlaces(selectedId);
  const members = useCommunityProfiles(detail.data?.members ?? [], Boolean(selectedId));
  const { friends, isPending: friendsPending } = useFriends();
  const upcoming = useEvents(PLAY_UPCOMING_PARAMS);
  const join = useJoinCommunity();
  const leave = useLeaveCommunity();

  const shared = places.data ?? [];
  const resolved = useQueries({
    queries: shared.map((place) => ({
      queryKey: qk.place(place.id),
      queryFn: ({ signal }: { signal: AbortSignal }) => fetchPlace(place.id, { signal }),
      staleTime: STALE.place,
      retry: false,
    })),
  });

  const community: SocialCommunity | null = detail.data?.community ?? list.data?.find((c) => c.id === selectedId) ?? null;
  const view = useMemo<SocialDetail | null>(() => {
    if (!selectedId) return null;
    return {
      id: selectedId,
      community,
      members,
      posts: posts.data ?? [],
      places: shared.map((place, index) => {
        const found = resolved[index]?.data;
        return {
          ...place,
          title: found?.title,
          description: found?.description,
          image: publicThumbnail(found?.image, 320),
          location: found ? (found.world ? found.worldName ?? undefined : found.coords) : undefined,
        };
      }),
      isPending: detail.isPending,
      isError: detail.isError,
      postsPending: posts.isPending,
      placesPending: places.isPending,
    };
  }, [selectedId, community, members, posts.data, posts.isPending, shared, resolved, detail.isPending, detail.isError, places.isPending]);

  const current = override && override.id === selectedId ? override.state : null;
  const err = join.error || leave.error;
  const membership: SocialMembership = {
    joined: current === "joined" ? true : current === "left" ? false : Boolean(community?.role && community.role !== "none"),
    requested: current === "requested",
    pending: join.isPending ? "join" : leave.isPending ? "leave" : null,
    error: err ? joinErrorText(err) : null,
  };

  const events = useMemo(
    () => (upcoming.data?.data ?? []).map((event) => ({
      id: event.id,
      name: event.name ?? "Untitled event",
      image: publicThumbnail(event.image, 320),
      startAt: event.next_start_at ?? event.start_at,
      attendees: event.total_attendees,
      live: event.live,
    })),
    [upcoming.data],
  );

  return (
    <>
      <Communities
        communities={list.data}
        isLoading={list.isPending}
        isError={list.isError}
        onRetry={() => void list.refetch()}
        friends={friends}
        friendsLoading={friendsPending}
        isGuest={!identity.address || identity.isGuest}
        events={events}
        eventsLoading={upcoming.isPending}
        nearby={players}
        selfAddress={identity.address}
        detail={view}
        membership={membership}
        onSelect={(id) => {
          setSelectedId(id);
          join.reset();
          leave.reset();
        }}
        onJoin={(c) => {
          if (join.isPending || leave.isPending) return;
          setOverride(null);
          leave.reset();
          join.mutate(
            { id: c.id, privacy: c.privacy },
            { onSuccess: () => setOverride({ id: c.id, state: c.privacy !== "private" ? "joined" : "requested" }) },
          );
        }}
        onLeave={(c) => {
          if (join.isPending || leave.isPending) return;
          setOverride(null);
          join.reset();
          leave.mutate({ id: c.id }, { onSuccess: () => setOverride({ id: c.id, state: "left" }) });
        }}
        onAddFriend={(address) => {
          if (address.toLowerCase() !== identity.address?.toLowerCase()) requestFriendAction("request", address);
        }}
        onMessage={requestDirectMessage}
        onCopyLink={(id) => void navigator.clipboard?.writeText(siteUrl(`/communities/${id}`)).catch(() => undefined)}
        onCreate={() => setShowCreate(true)}
      />
      {showCreate && (
        <CommunityCreate
          onDone={(created) => {
            setShowCreate(false);
            if (created) void list.refetch();
          }}
        />
      )}
    </>
  );
}
