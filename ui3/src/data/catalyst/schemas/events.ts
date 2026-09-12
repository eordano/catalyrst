
import { z } from "zod";

const nullableStr = z.string().nullish();
const nullableNum = z.number().nullish();

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

export const EventAttendeeSchema = z.object({
  event_id: z.string(),
  user: z.string(),
  user_name: nullableStr,
  created_at: z.string(),
});

export type EventAttendeeWire = z.infer<typeof EventAttendeeSchema>;
