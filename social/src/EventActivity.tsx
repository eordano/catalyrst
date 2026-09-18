import { useEffect, useRef } from "react";
import { api, type WalletIdentity } from "./api";
import type { FriendsState } from "./friends";
import type { SocialEvent } from "./Events";
import { eventAttendees, type Attendee } from "./event-attendance";
export type EventNotice = {
  id: string;
  title: string;
  body: string;
  href: string;
  createdAt: number;
  kind: "event";
};
export type AttendanceSnapshot = Record<string, Record<string, string>>;
export function newFriendAttendance(
  previous: Record<string, string> | undefined,
  attendees: Attendee[],
  friends: ReadonlySet<string>,
) {
  if (!previous) return [];
  return attendees.filter(
    (a) =>
      friends.has(a.user.toLowerCase()) &&
      previous[a.user.toLowerCase()] !== String(a.created_at || ""),
  );
}
/** Polls upcoming events only while the app is visible. First observations establish a baseline. */
export function EventActivity({
  identity,
  friendsState,
  onNotify,
}: {
  identity: WalletIdentity | null;
  friendsState: FriendsState;
  onNotify: (notice: EventNotice) => void;
}) {
  const notify = useRef(onNotify);
  notify.current = onNotify;
  const friends = useRef(friendsState.friends);
  friends.current = friendsState.friends;
  useEffect(() => {
    if (!identity || !friendsState.ready) return;
    let stopped = false,
      busy = false,
      offset = 0;
    const key = `dcl.social.event-activity.${identity.address.toLowerCase()}`;
    let snapshots: AttendanceSnapshot = {};
    try {
      const saved = JSON.parse(localStorage.getItem(key) || "{}");
      if (saved && typeof saved === "object" && !Array.isArray(saved))
        snapshots = saved;
    } catch {}
    async function poll() {
      if (
        stopped ||
        busy ||
        document.visibilityState !== "visible" ||
        !friends.current.length
      )
        return;
      busy = true;
      try {
        const response = await api<{
          data: SocialEvent[] | { events: SocialEvent[] };
        }>(`/events?offset=${offset}`);
        const rows = Array.isArray(response.data)
          ? response.data
          : response.data.events;
        const events = rows
          .slice(0, 24)
          .filter((e) => /^[0-9a-f-]{36}$/i.test(e.id));
        const nextOffset =
          rows.length === 24 && offset < 9984 ? offset + 24 : 0;
        const found = new Map(
          friends.current.map((f) => [
            f.address.toLowerCase(),
            f.name || f.address,
          ]),
        );
        const addresses = new Set(found.keys());
        for (let i = 0; i < events.length && !stopped; i += 4) {
          await Promise.all(
            events.slice(i, i + 4).map(async (event) => {
              try {
                const attendees = await eventAttendees(event.id);
                if (stopped) return;
                const fresh = newFriendAttendance(
                  snapshots[event.id],
                  attendees,
                  addresses,
                );
                delete snapshots[event.id];
                snapshots[event.id] = Object.fromEntries(
                  attendees
                    .slice(-2000)
                    .map((a) => [
                      a.user.toLowerCase(),
                      String(a.created_at || ""),
                    ]),
                );
                for (const attendee of fresh) {
                  const address = attendee.user.toLowerCase();
                  const friend = friends.current.find(
                    (f) => f.address.toLowerCase() === address,
                  );
                  if (!friend) continue;
                  notify.current({
                    id: `event:${event.id}:${address}:${attendee.created_at || "going"}`,
                    kind: "event",
                    title: `${friend.name || friend.address} is going`,
                    body: event.name,
                    href: `#/events/${event.id}`,
                    createdAt: Date.now(),
                  });
                }
              } catch {
                /* A failed read must not erase the previous baseline. */
              }
            }),
          );
        }
        if (!stopped) {
          offset = nextOffset;
          snapshots = Object.fromEntries(Object.entries(snapshots).slice(-240));
          try {
            localStorage.setItem(key, JSON.stringify(snapshots));
          } catch {}
        }
      } catch {
        /* Keep the last successful baseline; the next poll retries. */
      } finally {
        busy = false;
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), 60000);
    const visible = () => void poll();
    document.addEventListener("visibilitychange", visible);
    return () => {
      stopped = true;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [identity?.address, friendsState.ready]);
  return null;
}
