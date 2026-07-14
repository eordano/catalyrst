import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import { eventsApiPath } from "../typed";
import { apiOkOf, okDataTotalOf } from "../envelope";

import {
  EventAttendeeRecordSchema,
  EventCategoryRecordSchema,
  EventRecordSchema,
} from "../generated-schemas/events";
import type { ApiOk as RsApiOk } from "@ui/generated/catalyst/events/ApiOk";
import type { EventAttendeeRecord as RsEventAttendee } from "@ui/generated/catalyst/events/EventAttendeeRecord";
import type { EventListData as RsEventListData } from "@ui/generated/catalyst/events/EventListData";
import type { EventRecord as RsEvent } from "@ui/generated/catalyst/events/EventRecord";
import type { EventCategoryRecord as RsEventCategory } from "@ui/generated/catalyst/events/EventCategoryRecord";
import { warnInvalid } from "../warn";

/**
 * Validation truth is the generated `EventRecordSchema` (the ts-rs image of
 * catalyrst-events' `EventRecord`): every flag and every measurement is
 * required on the wire, so nothing here defaults an unread value into a
 * reading. `Event` keeps the historical, wider view type (nullable name,
 * coordinates, attendee count) that every consumer was written against; a
 * parsed wire record satisfies it structurally, so "normalization" is the
 * widening itself and no field is rewritten.
 */
export type EventRecord = z.infer<typeof EventRecordSchema>;

export type Event = {
  id: string;
  name: string | null;
  image: string | null;
  image_vertical: string | null;
  description: string | null;
  start_at: string | null;
  finish_at: string | null;
  next_start_at: string | null;
  all_day: boolean;
  x: number | null;
  y: number | null;
  position: number[] | null;
  coordinates: number[] | null;
  url: string | null;
  user_name: string | null;
  scene_name: string | null;
  estate_name: string | null;
  live: boolean;
  highlighted: boolean;
  trending: boolean;
  recurrent: boolean;
  recurrent_frequency: string | null;
  total_attendees: number | null;
  place_id: string | null;
};

/** The UI reads one localized label out of the wire record's open i18n map. */
export type EventCategory = {
  name: string;
  active: boolean;
  i18n: { en: string | null };
};

export type EventAttendee = z.infer<typeof EventAttendeeRecordSchema>;

/**
 * Throws when the payload is not an `EventRecord`. The old fallback cast a
 * rejected payload to `Event` and shipped it; callers that can degrade already
 * wrap the fetch in try/catch, so a validation failure now surfaces the same
 * way an unreachable service does.
 */
export function parseEvent(raw: unknown): Event {
  const r = EventRecordSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("Event", r.error.issues);
  throw new CatalystError("event payload failed validation", "events");
}

/** Rows that do not parse are dropped with a warning, never cast. */
export function parseEvents(raw: unknown[]): Event[] {
  const out: Event[] = [];
  for (const row of raw ?? []) {
    const r = EventRecordSchema.safeParse(row);
    if (r.success) out.push(r.data);
    else warnInvalid("Event", r.error.issues);
  }
  return out;
}

function toEventCategory(c: z.infer<typeof EventCategoryRecordSchema>): EventCategory {
  const en = c.i18n["en"];
  return {
    name: c.name,
    active: c.active,
    i18n: { en: typeof en === "string" ? en : null },
  };
}

export type FetchEventsParams = {
  list?: "live" | "active" | "highlight" | "trending";
  search?: string;
  category?: string;
  limit?: number;
  offset?: number;
};

const EventListEnvelope = apiOkOf(
  z.union([
    z.array(z.unknown()),
    z.object({ events: z.array(z.unknown()), total: z.number() }),
  ]),
);
const EventDetailEnvelope = apiOkOf(z.unknown());
const CategoryListEnvelope = apiOkOf(z.array(EventCategoryRecordSchema));
const AttendeeListEnvelope = okDataTotalOf(z.array(EventAttendeeRecordSchema));

export type _DriftEventListEnvelope = Assert<
  AssignableTo<RsApiOk<RsEventListData>, z.input<typeof EventListEnvelope>>
>;
export type _DriftEventDetailEnvelope = Assert<
  AssignableTo<RsApiOk<RsEvent>, z.input<typeof EventDetailEnvelope>>
>;
export type _DriftCategoryListEnvelope = Assert<
  AssignableTo<RsApiOk<RsEventCategory[]>, z.input<typeof CategoryListEnvelope>>
>;
export type _DriftAttendeeListEnvelope = Assert<
  AssignableTo<RsApiOk<RsEventAttendee[]>, z.input<typeof AttendeeListEnvelope>>
>;
export async function fetchEvents(
  params: FetchEventsParams = {},
  opts: GetOptions = {},
): Promise<{ data: Event[]; total: number }> {
  const raw = await getJSON<unknown>(eventsApiPath("get", "/api/events"), {
    ...opts,
    query: {
      list: params.list,
      search: params.search,
      limit: params.limit,
      offset: params.offset,
    },
  });
  const env = EventListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("Events envelope", env.error.issues);
    return { data: [], total: 0 };
  }
  const payload = env.data.data;
  const data = parseEvents(Array.isArray(payload) ? payload : payload.events);
  return { data, total: Array.isArray(payload) ? data.length : payload.total };
}

export async function fetchEvent(id: string, opts: GetOptions = {}): Promise<Event> {
  const raw = await getJSON<unknown>(
    eventsApiPath("get", "/api/events/{event_id}", { event_id: id }),
    opts,
  );
  const env = EventDetailEnvelope.safeParse(raw);
  if (env.success) return parseEvent(env.data.data);
  warnInvalid("Event envelope", env.error.issues);
  return parseEvent(raw);
}

const EVENT_CATEGORIES_TTL_MS = 5 * 60_000;
let eventCategoriesCache: { at: number; value: EventCategory[] } | null = null;

export async function fetchEventCategories(
  opts: GetOptions = {},
): Promise<EventCategory[]> {
  const cacheable = !opts.fetchImpl;
  if (
    cacheable &&
    eventCategoriesCache &&
    Date.now() - eventCategoriesCache.at < EVENT_CATEGORIES_TTL_MS
  ) {
    return eventCategoriesCache.value;
  }
  const raw = await getJSON<unknown>(
    eventsApiPath("get", "/api/events/categories"),
    opts,
  );
  const env = CategoryListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("EventCategory envelope", env.error.issues);
    return [];
  }
  const value = env.data.data.map(toEventCategory);
  if (cacheable && value.length > 0) {
    eventCategoriesCache = { at: Date.now(), value };
  }
  return value;
}

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export async function fetchAttendees(
  id: string,
  opts: GetOptions = {},
): Promise<{ attendees: EventAttendee[]; count: number }> {
  const raw = await getJSON<unknown>(
    eventsApiPath("get", "/api/events/{event_id}/attendees", { event_id: id }),
    opts,
  );
  const env = AttendeeListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("EventAttendee envelope", env.error.issues);
    return { attendees: [], count: 0 };
  }
  const attendees = env.data.data;
  return { attendees, count: env.data.total ?? attendees.length };
}

export function isAttending(attendees: EventAttendee[], address: string): boolean {
  const a = address.toLowerCase();
  return attendees.some((x) => x.user.toLowerCase() === a);
}

function hueFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export type LiveNowCard = {
  id: string;
  title: string;
  /** null when the event carries no attendee count; the badge says so rather
   *  than showing a zero nobody counted */
  users: number | null;
  isEvent: boolean;
  creator: string;
  hue: number;
  image?: string;
  /** absent when the event has no map position -- see `eventJumpUrl` */
  jumpUrl?: string;
};

export type UpcomingCard = {
  id: string;
  name: string;
  creator: string;
  time: string;
  hue: number;
  image?: string;
  jumpUrl?: string;
};

export function effectiveStartAt(e: Event, now = new Date()): string | null {
  const start = e.start_at ? new Date(e.start_at) : null;
  if (start && !Number.isNaN(start.getTime()) && start >= now) return e.start_at;
  return e.next_start_at ?? e.start_at;
}

/** Null when the event carries no map position. `0,0` is a real parcel in
 *  Genesis City, so defaulting to it does not mean "unknown" -- it drops the
 *  visitor somewhere the organiser never chose. */
export function eventJumpUrl(e: Event): string | null {
  const [x, y] = eventPosition(e);
  if (x === null || y === null) return null;
  return `https://catalyst.example.com/play/?position=${x},${y}`;
}

function eventPosition(e: Event): [number | null, number | null] {
  return [e.x ?? e.position?.[0] ?? null, e.y ?? e.position?.[1] ?? null];
}

export function formatEventTime(iso: string | null): string {
  if (!iso) return "Soon";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "Soon";
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
    hour12: false,
  });
}

export function formatEventWhen(iso: string | null): string {
  if (!iso) return "Date to be announced";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "Date to be announced";
  return d
    .toLocaleString("en-US", {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: "UTC",
      hour12: true,
    })
    .toUpperCase();
}

/** Null, not `(0,0)`, when the event has no position: every caller either omits
 *  the location line or names it as unpublished. */
export function eventCoords(e: Event): string | null {
  const [x, y] = eventPosition(e);
  return x === null || y === null ? null : `(${x},${y})`;
}

export function toLiveNowCard(e: Event): LiveNowCard {
  return {
    id: e.id,
    title: e.name ?? "Untitled event",
    users: e.total_attendees,
    isEvent: true,
    creator: e.user_name ?? e.scene_name ?? "Decentraland",
    hue: hueFor(e.id),
    image: e.image ?? undefined,
    jumpUrl: eventJumpUrl(e) ?? undefined,
  };
}

export function toUpcomingCard(e: Event): UpcomingCard {
  return {
    id: e.id,
    name: e.name ?? "Untitled event",
    creator: e.user_name ?? e.scene_name ?? "Decentraland",
    time: formatEventTime(effectiveStartAt(e)),
    hue: hueFor(e.id),
    image: e.image ?? undefined,
    jumpUrl: eventJumpUrl(e) ?? undefined,
  };
}

export type DayEvent = {
  id: string;
  name: string;
  creator: string;
  time: string;
  live: boolean;
  users?: number;
  x?: number;
  y?: number;
  hue: number;
  image?: string | null;
};

const DAY_MS = 86_400_000;

export function groupEventsByDay(
  events: Event[],
  liveIds: Set<string>,
  days = 7,
  now = new Date(),
): { allDays: DayEvent[][]; dayLabels: string[] } {
  const dayStart = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  const allDays: DayEvent[][] = Array.from({ length: days }, () => []);
  for (const e of events) {
    const iso = effectiveStartAt(e, now);
    if (!iso) continue;
    const t = new Date(iso).getTime();
    if (Number.isNaN(t)) continue;
    const idx = Math.floor((t - dayStart) / DAY_MS);
    if (idx < 0 || idx >= days) continue;
    allDays[idx].push({
      id: e.id,
      name: e.name ?? "Untitled event",
      creator: e.user_name ?? e.scene_name ?? "Decentraland",
      time: new Date(iso).toLocaleTimeString("en-US", {
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
        timeZone: "UTC",
      }),
      live: liveIds.has(e.id),
      users: e.total_attendees ?? undefined,
      x: e.x ?? e.position?.[0] ?? undefined,
      y: e.y ?? e.position?.[1] ?? undefined,
      hue: hueFor(e.id),
      image: e.image ?? undefined,
    });
  }
  for (const day of allDays) day.sort((a, b) => a.time.localeCompare(b.time));
  const dayLabels = Array.from({ length: days }, (_, i) => {
    if (i === 0) return "Today";
    if (i === 1) return "Tomorrow";
    return new Date(dayStart + i * DAY_MS)
      .toLocaleDateString("en-US", { weekday: "short", day: "numeric", timeZone: "UTC" })
      .toUpperCase();
  });
  return { allDays, dayLabels };
}
