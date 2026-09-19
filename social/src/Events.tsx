import { useEffect, useRef, useState } from "react";
import { api, execute, type WalletIdentity } from "./api";
import type { FriendsState } from "./friends";
import {
  eventAttendees,
  invalidateAttendance,
  type Attendee,
} from "./event-attendance";
import { downloadCalendar, googleCalendar } from "./event-calendar";
import { Picture } from "./Picture";
import { destinationUrl, worldName } from "./destinations";
import "./discovery.css";
export type SocialEvent = {
  id: string;
  name: string;
  description: string;
  image?: string;
  start_at: string;
  finish_at: string;
  next_start_at?: string;
  next_finish_at?: string;
  total_attendees: number;
  attending?: boolean;
  world?: boolean;
  world_name?: string;
  server?: string;
  url?: string;
  coordinates?: number[];
  position?: number[];
  scene_name?: string;
};
type Props = {
  identity: WalletIdentity | null;
  onConnect: () => void;
  friendsState?: FriendsState;
};
const start = (event: SocialEvent) =>
  new Date(event.next_start_at || event.start_at);
const finish = (event: SocialEvent) =>
  new Date(event.next_finish_at || event.finish_at);
export function eventDestination(event: SocialEvent): string | null {
  if (event.world) {
    let realm = event.world_name || event.server || "";
    if (!worldName(realm) && event.url) {
      try {
        realm = new URL(event.url).searchParams.get("realm") || "";
      } catch {}
    }
    const name = worldName(realm);
    return name ? destinationUrl({ x: 0, y: 0, world: name }) : null;
  }
  const p = event.position || event.coordinates;
  return p?.length === 2 &&
    p.every((n) => Number.isInteger(n) && Math.abs(n) <= 150)
    ? destinationUrl({ x: p[0], y: p[1] })
    : null;
}
function useEvents(
  search = "",
  identity: WalletIdentity | null = null,
  mine = false,
) {
  const [events, setEvents] = useState<SocialEvent[]>([]),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [revision, retry] = useState(0),
    [offset, setOffset] = useState(0),
    [more, setMore] = useState(false);
  useEffect(() => {
    setOffset(0);
    setEvents([]);
  }, [search, mine, identity?.address]);
  useEffect(() => {
    const abort = new AbortController();
    setLoading(true);
    setError("");
    const timer = setTimeout(
      () =>
        (mine
          ? identity
            ? execute<{ data: SocialEvent[] }>(
                identity,
                { type: "my_events" },
                () => !abort.signal.aborted,
              )
            : Promise.resolve({ data: [] })
          : api<{ data: SocialEvent[] | { events: SocialEvent[] } }>(
              "/events?search=" +
                encodeURIComponent(search) +
                "&offset=" +
                offset,
              { signal: abort.signal },
            )
        )
          .then((r) => {
            const rows = (
              Array.isArray(r.data) ? r.data : r.data.events
            ).filter(
              (e) =>
                Number.isFinite(start(e).getTime()) &&
                Number.isFinite(finish(e).getTime()),
            );
            if (abort.signal.aborted) return;
            const visible = mine
              ? rows
                  .filter(
                    (e) =>
                      e.attending === true &&
                      finish(e).getTime() > Date.now() &&
                      e.name.toLowerCase().includes(search.toLowerCase()),
                  )
                  .sort((a, b) => start(a).getTime() - start(b).getTime())
              : rows;
            setEvents((old) =>
              offset
                ? [
                    ...old,
                    ...visible.filter((e) => !old.some((o) => o.id === e.id)),
                  ]
                : visible,
            );
            setMore(!mine && rows.length === 24);
          })
          .catch((e) => {
            if (e.name !== "AbortError")
              setError("Events could not be loaded.");
          })
          .finally(() => {
            if (!abort.signal.aborted) setLoading(false);
          }),
      search ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  }, [search, offset, revision, mine, identity?.address]);
  return {
    events,
    error,
    loading,
    more,
    loadMore: () => setOffset(offset + 24),
    retry: () => retry(revision + 1),
  };
}
function FriendsAttending({
  eventId,
  friendsState,
}: {
  eventId: string;
  friendsState?: FriendsState;
}) {
  const [attendees, setAttendees] = useState<Attendee[]>([]);
  const addresses =
    friendsState?.friends
      .map((f) => f.address.toLowerCase())
      .sort()
      .join(",") || "";
  useEffect(() => {
    let active = true;
    setAttendees([]);
    if (!addresses) return;
    const refresh = () => {
      if (document.visibilityState === "visible")
        eventAttendees(eventId)
          .then((rows) => {
            if (active) setAttendees(rows);
          })
          .catch(() => {});
    };
    refresh();
    const timer = setInterval(refresh, 60000);
    const changed = (event: Event) => {
      if ((event as CustomEvent).detail === eventId) refresh();
    };
    window.addEventListener("social:event-rsvp", changed);
    return () => {
      active = false;
      clearInterval(timer);
      window.removeEventListener("social:event-rsvp", changed);
    };
  }, [eventId, addresses]);
  const going =
    friendsState?.friends.filter((f) =>
      attendees.some((a) => a.user.toLowerCase() === f.address.toLowerCase()),
    ) || [];
  if (!going.length) return null;
  return (
    <span
      className="event-friends"
      title={going.map((f) => f.name || f.address).join(", ")}
    >
      {going
        .slice(0, 2)
        .map((f) => f.name || `${f.address.slice(0, 6)}\u2026`)
        .join(" and ")}
      {going.length > 2 ? ` +${going.length - 2} friends` : ""}{" "}
      {going.length === 1 ? "is" : "are"} going
    </span>
  );
}
function EventCard({
  event,
  onOpen,
  compact = false,
  friendsState,
}: {
  event: SocialEvent;
  onOpen: () => void;
  compact?: boolean;
  friendsState?: FriendsState;
}) {
  const date = start(event),
    live = date.getTime() <= Date.now() && finish(event).getTime() > Date.now();
  return (
    <button
      className={`event-card${compact ? " compact" : ""}`}
      onClick={onOpen}
    >
      <div className="event-art">
        <Picture src={event.image} fallback="&#x25f7;" />
        {live && <span className="discovery-live">Live now</span>}
      </div>
      <div className="event-card-body">
        <time className="event-date" dateTime={date.toISOString()}>
          <b>{date.toLocaleDateString(undefined, { day: "numeric" })}</b>
          <span>{date.toLocaleDateString(undefined, { month: "short" })}</span>
        </time>
        <div>
          <small>
            {date.toLocaleTimeString(undefined, {
              hour: "numeric",
              minute: "2-digit",
            })}
            {event.world ? " \u00b7 World" : ""}
          </small>
          <strong>{event.name}</strong>
          <span>{event.total_attendees || 0} going</span>
          <FriendsAttending eventId={event.id} friendsState={friendsState} />
        </div>
      </div>
    </button>
  );
}
function EventDetail({
  event,
  identity,
  onConnect,
  onClose,
  friendsState,
}: { event: SocialEvent; onClose: () => void } & Props) {
  const session = useRef(0);
  const dialog = useRef<HTMLDialogElement>(null),
    [attending, setAttending] = useState<boolean | null>(null),
    [count, setCount] = useState(event.total_attendees || 0),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    let current = true;
    session.current++;
    setBusy(false);
    setAttending(null);
    setError("");
    if (identity)
      eventAttendees(event.id)
        .then((data) => ({ data }))
        .then((r) => {
          if (current) {
            setAttending(
              r.data.some(
                (a) =>
                  typeof a.user === "string" &&
                  a.user.toLowerCase() === identity.address.toLowerCase(),
              ),
            );
            setCount(r.data.length);
          }
        })
        .catch(() => {
          if (current)
            setError(
              "Your RSVP could not be checked. Reopen this event to retry.",
            );
        });
    return () => {
      current = false;
      session.current++;
    };
  }, [identity, event.id]);
  async function rsvp() {
    if (!identity) {
      onConnect();
      return;
    }
    if (attending === null) return;
    const generation = session.current;
    const isCurrent = () => generation === session.current;
    setBusy(true);
    setError("");
    try {
      await execute(
        identity,
        {
          type: "event_rsvp",
          event_id: event.id,
          attending: !attending,
        },
        isCurrent,
      );
      if (!isCurrent()) return;
      setCount((n) => Math.max(0, n + (attending ? -1 : 1)));
      setAttending(!attending);
      invalidateAttendance(event.id);
    } catch (e) {
      if (isCurrent())
        setError(e instanceof Error ? e.message : "RSVP could not be saved.");
    } finally {
      if (isCurrent()) setBusy(false);
    }
  }
  const destination = eventDestination(event);
  return (
    <dialog
      ref={dialog}
      className="dialog event-dialog"
      onCancel={onClose}
      onClick={(e) => {
        if (e.target === dialog.current) onClose();
      }}
      aria-label={event.name}
    >
      <div>
        <header>
          <h2>{event.name}</h2>
          <button aria-label="Close event" onClick={onClose}>
            &#xd7;
          </button>
        </header>
        <div className="event-detail-art">
          <Picture src={event.image} fallback="&#x25f7;" />
        </div>
        <p className="event-detail-time">
          {start(event).toLocaleString(undefined, {
            weekday: "long",
            month: "long",
            day: "numeric",
            hour: "numeric",
            minute: "2-digit",
          })}
        </p>
        <p className="muted">
          {event.scene_name ||
            (event.world ? "Decentraland World" : "Genesis City")}{" "}
          &#xb7; {count} going
        </p>
        <FriendsAttending eventId={event.id} friendsState={friendsState} />
        <p className="event-description">{event.description}</p>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <footer className="discovery-actions">
          <button
            className={attending ? "outline-button" : "primary"}
            disabled={busy || (!!identity && attending === null)}
            onClick={() => void rsvp()}
          >
            {busy
              ? "Saving\u2026"
              : !identity
                ? "Connect to RSVP"
                : attending === null
                  ? "Checking RSVP\u2026"
                  : attending
                    ? "Going \u2713 \u00b7 Cancel RSVP"
                    : "RSVP"}
          </button>
          {destination && (
            <a
              className="outline-button"
              href={destination}
              target="_blank"
              rel="noreferrer"
            >
              {event.world ? "Visit world" : "Visit place"} &#x2197;
            </a>
          )}
        </footer>
        <div className="discovery-actions event-calendar">
          <button
            className="text-button"
            onClick={() =>
              downloadCalendar(
                event,
                destination ||
                  `https://events.decentraland.org/event/?id=${event.id}`,
              )
            }
          >
            Download calendar file
          </button>
          <a
            className="text-button"
            target="_blank"
            rel="noreferrer"
            href={googleCalendar(
              event,
              destination ||
                `https://events.decentraland.org/event/?id=${event.id}`,
            )}
          >
            Google Calendar &#x2197;
          </a>
        </div>
      </div>
    </dialog>
  );
}
export function EventsPreview(props: Props) {
  const { events, error, loading, retry } = useEvents(),
    [selected, setSelected] = useState<SocialEvent | null>(null);
  return (
    <>
      <div className="events-preview">
        {events.slice(0, 3).map((event) => (
          <EventCard
            key={event.id}
            event={event}
            friendsState={props.friendsState}
            compact
            onOpen={() => setSelected(event)}
          />
        ))}
      </div>
      {loading && (
        <p className="muted" role="status">
          Finding events&#x2026;
        </p>
      )}
      {error && (
        <p role="alert">
          {error}{" "}
          <button className="text-button" onClick={retry}>
            Retry
          </button>
        </p>
      )}
      {!loading && !error && !events.length && (
        <p className="muted">No upcoming events yet.</p>
      )}
      {selected && (
        <EventDetail
          {...props}
          event={selected}
          onClose={() => setSelected(null)}
        />
      )}
    </>
  );
}
export function EventsPage(props: Props & { search?: string; mine?: boolean }) {
  const search = props.search || "", mine = !!props.mine;
  const [selected, setSelected] = useState<SocialEvent | null>(null);
  const [detailError, setDetailError] = useState("");
  const { events, error, loading, more, loadMore, retry } = useEvents(
    search,
    props.identity,
    mine,
  );
  useEffect(() => {
    const changed = () => retry();
    window.addEventListener("social:event-rsvp", changed);
    return () => window.removeEventListener("social:event-rsvp", changed);
  }, [retry]);
  useEffect(() => {
    let generation = 0;
    const open = () => {
      const current = ++generation;
      const id = window.location.hash.match(
        /^#\/events\/([0-9a-f-]{36})$/i,
      )?.[1];
      setDetailError("");
      if (!id) return;
      api<{ data: SocialEvent }>(`/events/${id}`)
        .then((r) => {
          if (current !== generation) return;
          if (
            !Number.isFinite(start(r.data).getTime()) ||
            !Number.isFinite(finish(r.data).getTime())
          )
            throw new Error("Invalid event");
          setSelected(r.data);
        })
        .catch(() => {
          if (current === generation)
            setDetailError(
              "This event could not be opened. It may no longer be available.",
            );
        });
    };
    open();
    window.addEventListener("hashchange", open);
    return () => {
      generation++;
      window.removeEventListener("hashchange", open);
    };
  }, []);
  const groups = new Map<string, SocialEvent[]>();
  for (const event of events) {
    const day = start(event).toLocaleDateString(undefined, {
      weekday: "long",
      month: "long",
      day: "numeric",
    });
    groups.set(day, [...(groups.get(day) || []), event]);
  }
  return (
    <section className="discovery-page">
      {mine && !props.identity && <button className="primary" onClick={props.onConnect}>Connect to see your RSVPs</button>}
      {detailError && <p role="alert">{detailError}</p>}
      {Array.from(groups, ([day, rows]) => (
        <section className="event-day" key={day}>
          <h2>{day}</h2>
          <div className="event-grid">
            {rows.map((event) => (
              <EventCard
                key={event.id}
                event={event}
                friendsState={props.friendsState}
                onOpen={() => setSelected(event)}
              />
            ))}
          </div>
        </section>
      ))}
      {loading && (
        <p role="status" className="muted">
          Finding events&#x2026;
        </p>
      )}
      {error && (
        <p role="alert">
          {error} <button onClick={retry}>Retry</button>
        </p>
      )}
      {!loading && !error && !events.length && (!mine || !!props.identity) && (
        <p className="muted">
          {mine
            ? "No upcoming RSVPs. Find an event and make plans."
            : "No events found. Try another search."}
        </p>
      )}
      {more && !loading && (
        <button className="outline-button" onClick={loadMore}>
          More events
        </button>
      )}
      {selected && (
        <EventDetail
          {...props}
          event={selected}
          onClose={() => {
            setSelected(null);
            if (window.location.hash.startsWith("#/events/"))
              window.location.hash = "#/events";
          }}
        />
      )}
    </section>
  );
}

export { EventActivity } from "./EventActivity";
