import { getJSON, sendSignedJSON, type RequestOpts } from "./client";
import { hasId, isRecord } from "./rows";
import { EventAttendeeSchema, EventCategorySchema, EventSchema } from "./schemas/events";
import type { DclEventWire, EventAttendeeWire, EventCategoryWire } from "./schemas/events";

export { EventAttendeeSchema, EventCategorySchema, EventSchema };

// The wire -> `DclEvent` normalization, which the schema module cannot carry: a
// perf build replaces it with an accepting stub, and a stub reproduces no
// transform. Here it runs whether or not validation did, so an event the
// service left blank arrives null in both builds rather than undefined in one.
//
// Every field a schema marks nullish is restated, so the type below is exactly
// what a caller gets and a field added to the schema shows up as its honest
// `| undefined` until it is normalized here too.

export function normalizeEvent(e: DclEventWire) {
  return {
    ...e,
    name: e.name ?? null,
    image: e.image ?? null,
    image_vertical: e.image_vertical ?? null,
    description: e.description ?? null,
    start_at: e.start_at ?? null,
    finish_at: e.finish_at ?? null,
    next_start_at: e.next_start_at ?? null,
    x: e.x ?? null,
    y: e.y ?? null,
    url: e.url ?? null,
    user_name: e.user_name ?? null,
    scene_name: e.scene_name ?? null,
    estate_name: e.estate_name ?? null,
    place_id: e.place_id ?? null,
    server: e.server ?? null,
  };
}

export type DclEvent = ReturnType<typeof normalizeEvent>;

/**
 * `i18n` is optional-chained because in perf mode the row reaching here was
 * never checked, so the object itself can be missing. Making it total is what
 * keeps one unlabelled row from taking the whole category list down.
 */
export function normalizeEventCategory(c: EventCategoryWire) {
  return { ...c, i18n: { en: c.i18n?.en ?? null } };
}

export type EventCategory = ReturnType<typeof normalizeEventCategory>;

export type EventsParams = {
  list?: string;
  search?: string;
  category?: string;
  limit?: number;
  offset?: number;
};

function isDev(): boolean {
  try {
    return Boolean(import.meta?.env?.DEV);
  } catch {
    return false;
  }
}

function warnInvalid(kind: string, issues: unknown): void {
  if (isDev()) console.warn(`[catalyst] ${kind} failed schema validation`, issues);
}

// What these readers can USE, as opposed to what their schemas declare. A perf
// build strips the schema and keeps the row, so the guard is the only thing left
// standing between a non-event and the list. See rows.ts.
//
// Both are identity: every accessor below is already total against a row that is
// merely thin -- `eventCoords` chains `x ?? position?.[0] ?? 0`, `eventStart`
// chains next_start_at then start_at, `formatEventTime` answers "Soon" to
// anything falsy -- so the one field an event cannot be without is the id its card
// is keyed by and its detail route addressed by. `normalizeEventCategory`
// optional-chains `i18n` for the same reason.

/**
 * `normalizeEventCategory` already tolerates a missing `i18n`; a missing `name`
 * is a chip with no key, no label and no filter behind it.
 */
function hasCategoryName(row: unknown): boolean {
  return isRecord(row) && typeof row.name === "string";
}

export function parseEvent(raw: unknown): DclEvent | null {
  const r = EventSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("Event", r.error.issues);
    return null;
  }
  return hasId(r.data) ? normalizeEvent(r.data) : null;
}

export function parseEvents(raw?: unknown[]): DclEvent[] {
  const out: DclEvent[] = [];
  for (const item of raw ?? []) {
    const parsed = parseEvent(item);
    if (parsed) out.push(parsed);
  }
  return out;
}

export function parseEventCategory(raw: unknown): EventCategory | null {
  const r = EventCategorySchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("EventCategory", r.error.issues);
    return null;
  }
  return hasCategoryName(r.data) ? normalizeEventCategory(r.data) : null;
}

export async function fetchEvents(
  params: EventsParams = {},
  opts: RequestOpts = {},
): Promise<{ data: DclEvent[]; total: number }> {
  const env = await getJSON<{ data?: unknown[]; total?: number }>("/api/events", {
    service: "events",
    ...opts,
    query: {
      list: params.list,
      search: params.search,
      category: params.category,
      limit: params.limit,
      offset: params.offset,
    },
  });
  const data = parseEvents(env?.data ?? []);
  return { data, total: env?.total ?? data.length };
}

export async function fetchEvent(id: string, opts: RequestOpts = {}): Promise<DclEvent | null> {
  const env = await getJSON<{ data?: unknown }>(
    `/api/events/${encodeURIComponent(id)}`,
    { service: "events", ...opts },
  );
  return parseEvent(env?.data);
}

export function normalizeEventAttendee(a: EventAttendeeWire) {
  return { ...a, user_name: a.user_name ?? null };
}

export type EventAttendee = ReturnType<typeof normalizeEventAttendee>;

/**
 * The only field the attendance readers dereference is `user`
 * (`isAttending` lowercases it); a row without one matches nobody.
 */
export function hasAttendeeUser(row: unknown): boolean {
  return isRecord(row) && typeof row.user === "string";
}

export function parseEventAttendee(raw: unknown): EventAttendee | null {
  const r = EventAttendeeSchema.safeParse(raw);
  if (!r.success) {
    warnInvalid("EventAttendee", r.error.issues);
    return null;
  }
  return hasAttendeeUser(r.data) ? normalizeEventAttendee(r.data) : null;
}

export function parseEventAttendees(raw?: unknown[]): EventAttendee[] {
  const out: EventAttendee[] = [];
  for (const item of raw ?? []) {
    const parsed = parseEventAttendee(item);
    if (parsed) out.push(parsed);
  }
  return out;
}

export function isAttending(
  attendees: EventAttendee[],
  address?: string | null,
): boolean {
  if (!address) return false;
  const a = address.toLowerCase();
  return attendees.some((row) => row.user.toLowerCase() === a);
}

export async function fetchEventAttendees(
  id: string,
  opts: RequestOpts = {},
): Promise<EventAttendee[]> {
  const env = await getJSON<{ data?: unknown[] }>(
    `/api/events/${encodeURIComponent(id)}/attendees`,
    { service: "events", ...opts },
  );
  return parseEventAttendees(env?.data ?? []);
}

export async function setEventAttendance(
  id: string,
  interested: boolean,
): Promise<EventAttendee[]> {
  const env = await sendSignedJSON<{ data?: unknown[] }>(
    `/api/events/${encodeURIComponent(id)}/attendees`,
    { service: "events", method: interested ? "POST" : "DELETE" },
  );
  return parseEventAttendees(env?.data ?? []);
}

export async function fetchEventCategories(opts: RequestOpts = {}): Promise<EventCategory[]> {
  const env = await getJSON<{ data?: unknown[] }>("/api/events/categories", {
    service: "events",
    ...opts,
  });
  const out: EventCategory[] = [];
  for (const raw of env?.data ?? []) {
    const parsed = parseEventCategory(raw);
    if (parsed) out.push(parsed);
  }
  return out;
}

export { hueFor } from "../format";

export function formatEventTime(iso?: string | null): string {
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

export function formatEventWhen(iso?: string | null): string {
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

export function eventCoords(e: DclEvent | null | undefined): string {
  const x = e?.x ?? e?.position?.[0] ?? e?.coordinates?.[0] ?? 0;
  const y = e?.y ?? e?.position?.[1] ?? e?.coordinates?.[1] ?? 0;
  return `(${x},${y})`;
}

export function eventStart(e: DclEvent | null | undefined): string | null {
  return e?.next_start_at ?? e?.start_at ?? null;
}

export function eventXY(e: DclEvent | null | undefined): { x: number; y: number } {
  const x = e?.x ?? e?.position?.[0] ?? e?.coordinates?.[0] ?? 0;
  const y = e?.y ?? e?.position?.[1] ?? e?.coordinates?.[1] ?? 0;
  return { x, y };
}
