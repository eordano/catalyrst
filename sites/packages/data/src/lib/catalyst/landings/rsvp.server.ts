import { fetchEvent, fetchAttendees, fetchEvents } from "../places/events";
import type { Event, EventAttendee } from "../places/events";
import type { GetOptions } from "../client";

export type RsvpData = {
  event: Event | null;
  attendees: EventAttendee[];
  count: number;
  fromFixture: boolean;
};

const EMPTY: RsvpData = {
  event: null,
  attendees: [],
  count: 0,
  fromFixture: false,
};

async function resolveDefaultEventId(opts: GetOptions): Promise<string | null> {
  try {
    const { data } = await fetchEvents({ list: "active", limit: 1 }, opts);
    return data[0]?.id ?? null;
  } catch {
    return null;
  }
}

export async function loadRsvp(
  id: string | null | undefined,
  opts: GetOptions = {},
): Promise<RsvpData> {
  const eventId = id?.trim() || (await resolveDefaultEventId(opts));
  if (!eventId) return EMPTY;
  try {
    const event = await fetchEvent(eventId, opts);
    let attendees: EventAttendee[] = [];
    let count = event.total_attendees ?? 0;
    try {
      const roster = await fetchAttendees(eventId, opts);
      attendees = roster.attendees;
      count = roster.count;
    } catch {
    }
    return { event, attendees, count, fromFixture: false };
  } catch {
    return EMPTY;
  }
}
