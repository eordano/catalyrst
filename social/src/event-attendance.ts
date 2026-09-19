import { api } from "./api";
export type Attendee = { user: string; created_at?: string };
const cache = new Map<string, { time: number; promise: Promise<Attendee[]> }>();
export function eventAttendees(id: string): Promise<Attendee[]> {
  const saved = cache.get(id);
  if (saved && Date.now() - saved.time < 55000) return saved.promise;
  const promise = api<{ data: Attendee[] }>(
    `/events/${encodeURIComponent(id)}/attendees`,
  )
    .then((r) => r.data.filter((a) => typeof a.user === "string"))
    .catch((error) => {
      cache.delete(id);
      throw error;
    });
  if (cache.size > 100) cache.clear();
  cache.set(id, { time: Date.now(), promise });
  return promise;
}
export function invalidateAttendance(id: string) {
  cache.delete(id);
  window.dispatchEvent(new CustomEvent("social:event-rsvp", { detail: id }));
}
