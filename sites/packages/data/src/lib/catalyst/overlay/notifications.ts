import { z } from "zod";

import { NotificationItemSchema } from "../generated-schemas/notifications";

export const NOTIFICATION_CATEGORIES = [
  "friends",
  "badge",
  "gift",
  "community",
  "marketplace",
  "system",
] as const;
export type NotificationCategory = (typeof NOTIFICATION_CATEGORIES)[number];

export function categoryForType(type: string): NotificationCategory {
  const t = type.toLowerCase();
  if (t.startsWith("social_service_friendship")) return "friends";
  if (t === "badge_granted") return "badge";
  if (t === "tip_received" || t === "transfer_received") return "gift";
  if (t.startsWith("community_")) return "community";
  if (
    t === "item_sold" ||
    t === "item_published" ||
    t === "bid_accepted" ||
    t === "bid_received" ||
    t === "royalties_earned" ||
    t.startsWith("credits_") ||
    t.startsWith("rental_")
  ) {
    return "marketplace";
  }
  return "system";
}

/**
 * Validation truth is the generated NotificationItemSchema: every field of
 * catalyrst-notifications' NotificationItem is required on the wire, so a row
 * with no `read` or no `timestamp` is dropped instead of arriving as an
 * unread notification from 1970 that nobody sent.
 */
export const NotificationSchema = NotificationItemSchema;
export type Notification = z.infer<typeof NotificationSchema>;

export const ListEnvelopeSchema = z.object({
  notifications: z.array(z.unknown()),
});

export function parseNotifications(raw: unknown): Notification[] {
  const env = ListEnvelopeSchema.safeParse(raw);
  const rows = env.success ? env.data.notifications : [];
  const out: Notification[] = [];
  for (const item of rows) {
    const r = NotificationSchema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  // The wire carries timestamp as an epoch-millis STRING (i64 serialized as
  // str for JS precision); epoch millis fit a double, so Number() is exact.
  out.sort((a, b) => Number(b.timestamp) - Number(a.timestamp));
  return out;
}

export function humanizeType(type: string): string {
  return type
    .split("_")
    .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : w))
    .join(" ");
}

export function unreadCount(rows: Notification[]): number {
  return rows.filter((n) => !n.read).length;
}

export function filterByCategory(
  rows: Notification[],
  category: string,
): Notification[] {
  if (!category || category === "all") return rows;
  return rows.filter((n) => categoryForType(n.type) === category);
}

export function parseFilter(raw: string | null | undefined): string {
  const v = (raw ?? "").trim().toLowerCase();
  return (NOTIFICATION_CATEGORIES as readonly string[]).includes(v) ? v : "";
}

export function markRead(
  rows: Notification[],
  ids: "all" | readonly string[],
): Notification[] {
  const all = ids === "all";
  const set = all ? null : new Set(ids);
  return rows.map((n) =>
    n.read || (set ? !set.has(n.id) : !all) ? n : { ...n, read: true },
  );
}

export function relativeTime(timestamp: number, now: number): string {
  const diff = Math.max(0, now - timestamp);
  const min = Math.floor(diff / 60000);
  if (min < 1) return "Just Now";
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  if (day === 1) return "Yesterday";
  return `${day}d`;
}
