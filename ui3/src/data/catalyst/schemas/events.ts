// Wire shapes for the events reader.
//
// Schemas and the WIRE types they infer, and nothing else. A perf build aliases
// this whole module to a generated stub (vite.validate.js), which is what lets
// zod leave the bundle -- so a transform put here would run in one build and not
// the other, and the stub would be changing behaviour rather than only changing
// what is checked.
//
// The nullish -> null normalization the exported `DclEvent` and `EventCategory`
// types promise lives in ../events.ts next to the accessors that read those
// fields, and runs in both modes.

import { z } from "zod";

const nullableStr = z.string().nullish();
const nullableNum = z.number().nullish();

/**
 * Required here means required in `EventRecord` (catalyrst-events), which
 * serializes every one of these unconditionally. `live` and `total_attendees`
 * are the sharp ones: defaulted, a failed lookup renders as "nobody is here".
 */
export const EventSchema = z.object({
  id: z.string(),
  name: nullableStr,
  image: nullableStr,
  image_vertical: nullableStr,
  description: nullableStr,
  start_at: nullableStr,
  finish_at: nullableStr,
  next_start_at: nullableStr,
  all_day: z.boolean(),
  x: nullableNum,
  y: nullableNum,
  position: z.array(z.number()),
  coordinates: z.array(z.number()),
  url: nullableStr,
  user_name: nullableStr,
  scene_name: nullableStr,
  estate_name: nullableStr,
  live: z.boolean(),
  highlighted: z.boolean(),
  trending: z.boolean(),
  recurrent: z.boolean(),
  total_attendees: z.number(),
  place_id: nullableStr,
  world: z.boolean(),
  server: nullableStr,
});

export type DclEventWire = z.infer<typeof EventSchema>;

export const EventCategorySchema = z.object({
  name: z.string(),
  active: z.boolean(),
  i18n: z.object({ en: nullableStr }),
});

export type EventCategoryWire = z.infer<typeof EventCategorySchema>;

/**
 * Row shape of `EventAttendeeRecord` (catalyrst-events), which serializes
 * `event_id`, `user` and `created_at` unconditionally.
 */
export const EventAttendeeSchema = z.object({
  event_id: z.string(),
  user: z.string(),
  user_name: nullableStr,
  created_at: z.string(),
});

export type EventAttendeeWire = z.infer<typeof EventAttendeeSchema>;
