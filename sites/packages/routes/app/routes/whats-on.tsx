import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";

import LdWhatsOnPage, { type LdWhatsOnEventLink } from "@ui/landings/pages/LdWhatsOnPage";

import {
  effectiveStartAt,
  fetchEvents,
  groupEventsByDay,
  toLiveNowCard,
  toUpcomingCard,
  type DayEvent,
  type Event,
} from "@data/lib/catalyst/places/events";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/whats-on";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "whatsOn" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "admin/whats-on";
const LIVE_LIMIT = 8;
const UPCOMING_LIMIT = 24;
const FETCH_LIMIT = 100;

const FILTERS = [
  { id: "", label: "All" },
  { id: "today", label: "Today" },
  { id: "week", label: "This week" },
  { id: "recurring", label: "Recurring" },
] as const;
type FilterId = (typeof FILTERS)[number]["id"];

function applyFilter(events: Event[], filter: FilterId, now: Date): Event[] {
  if (!filter) return events;
  const dayStart = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  const dayMs = 86_400_000;
  return events.filter((e) => {
    if (filter === "recurring") return e.recurrent === true;
    const iso = effectiveStartAt(e, now);
    if (!iso) return false;
    const t = new Date(iso).getTime();
    if (Number.isNaN(t)) return false;
    if (filter === "today") return t >= dayStart && t < dayStart + dayMs;
    return t >= dayStart && t < dayStart + 7 * dayMs;
  });
}

const FALLBACK: Assignment = {
  variant: "live_feed",
  flags: { liveRail: true },
  experimentKey: "lp_whatson_feed",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const rawFilter = url.searchParams.get("filter")?.trim() ?? "";
  const filter: FilterId = (FILTERS.some((f) => f.id === rawFilter) ? rawFilter : "") as FilterId;
  const search = url.searchParams.get("search")?.trim() ?? "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const now = new Date();

  const [live, active] = await Promise.all([
    fetchEvents({
      list: "live",
      search: search || undefined,
      limit: LIVE_LIMIT,
    })
      .then((r) => r.data)
      .catch(() => [] as Event[]),
    fetchEvents({
      list: "active",
      search: search || undefined,
      limit: FETCH_LIMIT,
    })
      .then((r) => r.data)
      .catch(() => [] as Event[]),
  ]);

  const sorted = [...active].sort((a, b) => {
    const ta = new Date(effectiveStartAt(a, now) ?? 8.64e15).getTime();
    const tb = new Date(effectiveStartAt(b, now) ?? 8.64e15).getTime();
    return ta - tb;
  });
  const upcoming = applyFilter(sorted, filter, now).slice(0, UPCOMING_LIMIT);

  const liveIds = new Set(live.map((e) => e.id));
  const { allDays, dayLabels } = groupEventsByDay(sorted, liveIds, 7, now);

  const payload = {
    sid,
    filter,
    search,
    live,
    upcoming,
    allDays,
    dayLabels,
  };

  return wrap(payload);
}

export default function WhatsOn({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <WhatsOnFeed
      sid={d.sid}
      filter={d.filter}
      search={d.search}
      live={d.live}
      upcoming={d.upcoming}
      allDays={d.allDays}
      dayLabels={d.dayLabels}
    />
  );
}

type FeedProps = {
  sid: string;
  filter: FilterId;
  search: string;
  live: Event[];
  upcoming: Event[];
  allDays: DayEvent[][];
  dayLabels: string[];
};

function WhatsOnFeed({ sid, filter, search, live, upcoming, allDays, dayLabels }: FeedProps) {
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  useWhatsOnView(sid, live.length, upcoming.length, filter, search);

  const liveCards = live.map(toLiveNowCard);
  const upcomingCards = upcoming.map(toUpcomingCard);

  const eventLinks: LdWhatsOnEventLink[] = dedupeById([...live, ...upcoming]).map((e) => ({
    id: e.id,
    href: `/whats-on/${encodeURIComponent(e.id)}`,
    name: e.name ?? "Untitled event",
    live: e.live,
  }));

  function updateParam(key: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(key, value);
        else next.delete(key);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  return (
    <LdWhatsOnPage
      liveNow={liveCards}
      upcoming={upcomingCards}
      allDays={allDays}
      dayLabels={dayLabels}
      filters={[...FILTERS]}
      activeFilter={filter}
      onFilterSelect={(id) => updateParam("filter", id)}
      eventLinks={eventLinks}
      onEventLinkClick={(link, e) => {
        e.preventDefault();
        track("lp_event_card_clicked", { event_id: link.id }, { sid, story: STORY });
        navigate(link.href);
      }}
    />
  );
}

function dedupeById(events: Event[]): Event[] {
  const seen = new Set<string>();
  const out: Event[] = [];
  for (const e of events) {
    if (seen.has(e.id)) continue;
    seen.add(e.id);
    out.push(e);
  }
  return out;
}

function useWhatsOnView(
  sid: string,
  liveCount: number,
  upcomingCount: number,
  filter: string,
  search: string,
) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${filter}|${search}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "lp_whatson_viewed",
      {
        live_count: liveCount,
        upcoming_count: upcomingCount,
        filter: filter || null,
        search: search || null,
      },
      { sid, story: STORY },
    );
  }, [sid, liveCount, upcomingCount, filter, search]);
}
