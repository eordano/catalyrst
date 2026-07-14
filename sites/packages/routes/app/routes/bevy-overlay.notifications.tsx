import { useCallback, useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import { loadNotifications } from "@data/lib/catalyst/overlay/notifications.server";
import {
  filterByCategory,
  parseFilter,
  type Notification,
} from "@data/lib/catalyst/overlay/notifications";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { NotificationsStage as NotificationsStageView } from "@ui/overlay/panels/NotificationsPanel";

import type { Route } from "./+types/bevy-overlay.notifications";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/notifications";

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "notifications";
}

const FALLBACK: Assignment = {
  variant: "control",
  flags: { showFilters: false, markAll: false },
  experimentKey: "notif_triage_controls",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const filter = parseFilter(url.searchParams.get("filter"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const feed = await loadNotifications({ signal: request.signal });

  const payload = {
    sid,
    filter,
    now: Date.now(),
    notifications: feed.notifications,
    unavailable: feed.unavailable,
    assignment,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  filter: string;
  now: number;
  notifications: Notification[];
  unavailable: string | null;
  assignment: Assignment;
};

export default function NotificationsRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return (
    <NotificationsStage
      sid={d.sid}
      filter={d.filter}
      now={d.now}
      notifications={d.notifications}
      unavailable={d.unavailable}
      assignment={d.assignment}
    />
  );
}

type StageProps = LoaderData;

function NotificationsStage({
  sid,
  filter,
  now,
  notifications,
  unavailable,
  assignment,
}: StageProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const open = isPanelOpen(searchParams.get("panel"));

  const flags = assignment.flags as { showFilters?: boolean; markAll?: boolean };
  const showFilters = flags.showFilters === true;
  const showMarkAll = flags.markAll === true;

  const ctx = { sid, story: STORY, variant: assignment.variant, experimentKey: assignment.experimentKey };

  const lastOpen = useRef<boolean | null>(null);
  useEffect(() => {
    if (!open) {
      lastOpen.current = false;
      return;
    }
    if (lastOpen.current === true) return;
    lastOpen.current = true;
    const unread = notifications.filter((n) => !n.read).length;
    track(
      "notif_panel_opened",
      { count: notifications.length, unread, read_failed: unavailable !== null },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const onBell = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (open) next.set("panel", "closed");
        else next.set("panel", "notifications");
        return next;
      },
      { preventScrollReset: true },
    );
  }, [open, setSearchParams]);

  const onFilter = useCallback(
    (category: string) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (category) next.set("filter", category);
          else next.delete("filter");
          return next;
        },
        { preventScrollReset: true },
      );
      const count = filterByCategory(notifications, category).length;
      track("notif_filter_applied", { filter: category || "all", count }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [setSearchParams, notifications],
  );

  const onMarkRead = useCallback(
    (n: { id: string; type: string }) => {
      track("notif_marked_read", { id: n.id, type: n.type }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onMarkAll = useCallback(
    (count: number) => {
      track("notif_mark_all_read", { count }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  return (
    <NotificationsStageView
      open={open}
      onBell={onBell}
      notifications={notifications}
      filter={filter}
      now={now}
      showFilters={showFilters}
      showMarkAll={showMarkAll}
      unavailable={unavailable}
      onFilter={onFilter}
      onMarkRead={onMarkRead}
      onMarkAll={onMarkAll}
    />
  );
}
