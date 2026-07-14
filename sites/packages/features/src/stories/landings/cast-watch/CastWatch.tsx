import { useCallback, useState } from "react";

import StCastWatcher from "@ui/web/pages/StCastWatcher";

import { track, type TrackContext } from "@core/lib/telemetry/track";
import type { WatchResult } from "@data/lib/catalyst/landings/cast-watcher";

const JOIN_CLASS = "stcastwatcher__join";
const LEAVE_CLASS = "stcastwatcher__leave";
const ICON_BTN_CLASS = "stcastwatcher__iconbtn";
const SIDE_CLOSE_CLASS = "stcastwatcher__sideclose";

const ICON_MUTE = 0;
const ICON_CHAT = 1;

type WatcherState = "onboarding" | "joining" | "live" | "waiting";

export type CastWatchProps = {
  trackCtx: TrackContext;
  watch: WatchResult;
  demoMessages?: Array<{ name: string; time: string; body: string }>;
  participantCount?: number;
  unreadCount?: number;
  onTrack?: typeof track;
};

export default function CastWatch({
  trackCtx,
  watch,
  demoMessages,
  participantCount,
  unreadCount,
  onTrack = track,
}: CastWatchProps) {
  const initialState: WatcherState = watch.status === "live" ? "onboarding" : "waiting";

  const [state, setState] = useState<WatcherState>(initialState);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [isTabMuted, setIsTabMuted] = useState(false);

  const roomId = watch.credentials?.roomId ?? "";
  const placeName = watch.placeName;

  const join = useCallback(() => {
    setState("joining");
    onTrack(
      "cast_watch_joined",
      { room_id: roomId, place_name: placeName, stub: true },
      trackCtx,
    );
    setTimeout(() => setState("live"), 600);
  }, [onTrack, roomId, placeName, trackCtx]);

  const leave = useCallback(() => {
    onTrack("cast_watch_left", {}, trackCtx);
    setSidebarOpen(false);
    setState("onboarding");
  }, [onTrack, trackCtx]);

  const toggleMute = useCallback(() => {
    setIsTabMuted((prev) => {
      const next = !prev;
      onTrack(next ? "cast_watch_muted" : "cast_watch_unmuted", {}, trackCtx);
      return next;
    });
  }, [onTrack, trackCtx]);

  const toggleChat = useCallback(() => {
    setSidebarOpen((prev) => {
      const next = !prev;
      if (next) onTrack("cast_watch_chat_opened", {}, trackCtx);
      return next;
    });
  }, [onTrack, trackCtx]);

  const onClickCapture = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement | null;
      const btn = target?.closest?.("button");
      if (!btn) return;

      if (btn.classList.contains(JOIN_CLASS)) {
        join();
        return;
      }
      if (btn.classList.contains(LEAVE_CLASS)) {
        leave();
        return;
      }
      if (btn.classList.contains(SIDE_CLOSE_CLASS)) {
        setSidebarOpen(false);
        return;
      }
      if (btn.classList.contains(ICON_BTN_CLASS)) {
        const parent = btn.parentElement;
        if (!parent) return;
        const icons = Array.from(parent.children).filter((el) =>
          el.classList.contains(ICON_BTN_CLASS),
        );
        const idx = icons.indexOf(btn);
        if (idx === ICON_MUTE) toggleMute();
        else if (idx === ICON_CHAT) toggleChat();
      }
    },
    [join, leave, toggleMute, toggleChat],
  );

  const Watcher = StCastWatcher as unknown as React.ComponentType<{
    streamName?: string;
    state?: WatcherState;
    sidebarOpen?: boolean;
    isTabMuted?: boolean;
    chatFooter?: string;
    participantCount?: number;
    unreadCount?: number;
  }>;

  const chatFooter = `Jump into ${placeName} in Decentraland to participate in the chat.`;

  return (
    <div
      className="cast-watch"
      data-state={state}
      data-status={watch.status}
      data-location={watch.location}
      onClickCapture={onClickCapture}
    >
      <Watcher
        streamName={placeName}
        state={state}
        sidebarOpen={state === "live" && sidebarOpen}
        isTabMuted={isTabMuted}
        chatFooter={chatFooter}
        participantCount={participantCount}
        unreadCount={state === "live" && !sidebarOpen ? unreadCount : 0}
      />
      {demoMessages && demoMessages.length === 0 && null}
    </div>
  );
}
