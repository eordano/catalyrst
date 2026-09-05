// Wire shapes for the notifications reader.
//
// Schemas and nothing else. A perf build aliases this whole module to a
// generated stub (vite.validate.js), which is what lets zod leave the bundle --
// so a transform put here would run in one build and not the other, and the
// stub would be changing behaviour rather than only changing what is checked.
//
// This module never carried one: `Notification` (../notificationsView.ts) is a
// hand-written total type that the shape below already satisfies field for
// field, so there is no nullish field to normalize and the reader hands a parsed
// row straight through. It is the perf-parity control for that reason.

import { z } from "zod";

const MetadataSchema = z.record(z.string(), z.unknown());

/**
 * `NotificationItem` (catalyrst-notifications) serializes every one of these.
 * A defaulted `timestamp` sorted unread mail to the bottom of the list under a
 * date nobody sent it at.
 */
export const NotificationSchema = z.object({
  id: z.string().min(1),
  type: z.string().min(1),
  address: z.string(),
  timestamp: z.number(),
  read: z.boolean(),
  created_at: z.string(),
  updated_at: z.string(),
  metadata: MetadataSchema,
});

export const ListEnvelopeSchema = z.object({
  notifications: z.array(z.unknown()),
});
