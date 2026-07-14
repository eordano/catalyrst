import { z } from "zod";

import { postJSON } from "../client";
import { okDataTotalOf } from "../envelope";
import { eventsApiPath } from "../typed";
import type { AuthIdentity } from "../../auth/types";
import type {
  CommitFn,
  RsvpResult,
} from "@features/stories/landings/rsvp-event/machine";

const AttendeeListEnvelope = okDataTotalOf(z.array(z.unknown()));

export function buildRsvpCommit(identity: AuthIdentity | null): CommitFn {
  return async ({ eventId, direction, count, signal }): Promise<RsvpResult> => {
    if (!identity) throw new Error("Sign in to RSVP.");
    if (!eventId) throw new Error("This event is no longer available.");

    const going = direction === "going";
    const path = eventsApiPath(
      going ? "post" : "delete",
      "/api/events/{event_id}/attendees",
      { event_id: eventId },
    );
    const raw = await postJSON<unknown>(path, undefined, {
      identity,
      method: going ? "POST" : "DELETE",
      signal,
    });

    const env = AttendeeListEnvelope.safeParse(raw);
    if (!env.success) return { count };
    return { count: env.data.total ?? env.data.data.length };
  };
}
