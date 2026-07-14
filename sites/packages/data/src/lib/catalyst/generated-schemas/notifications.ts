// GENERATED from catalyrst/ui3/src/generated/catalyst/notifications by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { NotificationItem } from "@ui/generated/catalyst/notifications/NotificationItem";
import type { Subscription } from "@ui/generated/catalyst/notifications/Subscription";

export const NotificationItemSchema = z.object({
  id: z.string(),
  type: z.string(),
  address: z.string(),
  timestamp: z.number(),
  read: z.boolean(),
  created_at: z.string(),
  updated_at: z.string(),
  metadata: z.record(z.string(), z.unknown()),
});

export const SubscriptionSchema = z.object({
  address: z.string(),
  email: z.string().nullable(),
  unconfirmedEmail: z.string().optional(),
  details: z.record(z.string(), z.unknown()),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertNotificationItem = Assert<Mutual<NotificationItem, z.infer<typeof NotificationItemSchema>>>;
export type _AssertSubscription = Assert<Mutual<Subscription, z.infer<typeof SubscriptionSchema>>>;
