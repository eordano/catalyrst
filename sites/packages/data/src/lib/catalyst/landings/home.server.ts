import { fetchEvents, type Event } from "../places/events";
import { fetchMostActivePlaces } from "../places/index";
import type { GetOptions } from "../client";
import {
  eventToRitualItem,
  eventWeekday,
  parseHomeContent,
  placeToHotspotItem,
  withHotspots,
  withLiveEvents,
  withRituals,
  type HomeContent,
  type HotspotRailItem,
  type RitualRailItem,
} from "./home";

const LIVE_RAIL_LIMIT = 3;
const HOTSPOT_RAIL_LIMIT = 4;
const RITUAL_RAIL_LIMIT = 5;
const RITUAL_FETCH_LIMIT = 100;
const EVENT_LISTS = ["highlight", "trending", "active"] as const;

const HOME_CONTENT: HomeContent = {
  hero: {
    kicker: "Decentraland now on mobile.",
    title: "Hang Out From Anywhere",
    subtitle: "Close the Feed. Come Hang Out.",
    downloads: null,
    downloadsLabel: "downloads",
    cta: {
      desktop: {
        label: "Jump in \u{2014} no download",
        href: "/play/",
      },
      epic: {
        label: "Play in your browser",
        href: "/play/",
      },
    },
    platforms: [
      { id: "ios", label: "iOS", href: "/play/" },
      { id: "android", label: "Android", href: "/play/" },
    ],
  },
  rails: [
    {
      id: "whatson",
      kind: "events",
      title: "Jump Into What's Happening",
      viewAll: { label: "View All", href: "/whats-on" },
      items: [],
    },
    {
      id: "vibe",
      kind: "hotspots",
      title: "Catch the Vibe",
      viewAll: null,
      items: [],
    },
    {
      id: "rituals",
      kind: "rituals",
      title: "Your Weekly Rituals",
      viewAll: { label: "View All", href: "/whats-on" },
      items: [],
    },
  ],
  comeHangOut: {
    title: "Come Hang Out",
    downloads: null,
    downloadsLabel: "downloads",
  },
};

export function loadHomeContent(): HomeContent {
  return parseHomeContent(HOME_CONTENT);
}

/**
 * A rail whose read failed is REMOVED from `content.rails`, not left empty.
 * Each loader below answers `null` for "we could not ask" and a list for "we
 * asked" -- collapsing the two would render the "Jump Into What's Happening"
 * rail with nothing under it when the events service is merely unreachable,
 * which reads as nothing happening. `unreadable` names the dropped rails so a
 * caller can say so.
 */
export async function loadHome(
  opts: GetOptions = {},
): Promise<{ content: HomeContent; live: boolean; unreadable: string[] }> {
  let content = loadHomeContent();

  const [events, hotspots, rituals] = await Promise.all([
    loadRailEvents(opts),
    loadHotspots(opts),
    loadRituals(opts),
  ]);

  const unreadable: string[] = [];
  const live = (events?.length ?? 0) > 0;
  if (events === null) unreadable.push("events");
  else if (live) content = withLiveEvents(content, events);

  if (hotspots === null) unreadable.push("hotspots");
  else content = withHotspots(content, hotspots);

  if (rituals === null) unreadable.push("rituals");
  else content = withRituals(content, rituals);

  if (unreadable.length > 0) {
    content = {
      ...content,
      rails: content.rails.filter((rail) => !unreadable.includes(rail.kind)),
    };
  }

  return { content, live, unreadable };
}

/** `null` when every list read failed, so we know nothing about what is on. */
async function loadRailEvents(opts: GetOptions): Promise<Event[] | null> {
  const results = await Promise.all(
    EVENT_LISTS.map((list) =>
      fetchEvents({ list, limit: LIVE_RAIL_LIMIT }, opts)
        .then((r) => r.data)
        .catch(() => null),
    ),
  );
  if (results.every((events) => events === null)) return null;
  return results.find((events) => events !== null && events.length > 0) ?? [];
}

async function loadHotspots(opts: GetOptions): Promise<HotspotRailItem[] | null> {
  const places = await fetchMostActivePlaces(
    { limit: HOTSPOT_RAIL_LIMIT },
    opts,
  ).catch(() => null);
  return places === null ? null : places.map(placeToHotspotItem);
}

async function loadRituals(opts: GetOptions): Promise<RitualRailItem[] | null> {
  const events = await fetchEvents({ limit: RITUAL_FETCH_LIMIT }, opts)
    .then((r) => r.data)
    .catch(() => null);
  if (events === null) return null;
  if (!events.length) return [];

  const picked = events.filter((e) => e.recurrent_frequency === "WEEKLY");
  if (picked.length < 2) {
    for (const e of events) {
      if (picked.length >= RITUAL_RAIL_LIMIT) break;
      if (e.recurrent && !picked.some((p) => p.id === e.id)) picked.push(e);
    }
  }
  if (picked.length < 2) {
    for (const e of events) {
      if (picked.length >= RITUAL_RAIL_LIMIT) break;
      if (!picked.some((p) => p.id === e.id)) picked.push(e);
    }
  }

  return sortByWeekday(picked)
    .slice(0, RITUAL_RAIL_LIMIT)
    .map(eventToRitualItem);
}

function weekdayKey(e: Event): number {
  const day = eventWeekday(e);
  return day == null ? 7 : (day + 6) % 7;
}

function sortByWeekday(events: Event[]): Event[] {
  return [...events].sort((a, b) => weekdayKey(a) - weekdayKey(b));
}
