
import { z } from "zod";

const MetadataSchema = z.record(z.string(), z.unknown());

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
