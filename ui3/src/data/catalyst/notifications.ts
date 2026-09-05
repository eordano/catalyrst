import { getJSON, sendJSON, type RequestOpts } from "./client";
import type { Notification } from "./notificationsView";
import { serviceBase, signedFetch } from "./client";
import { field, isRecord, keepRows } from "./rows";
import { ListEnvelopeSchema, NotificationSchema } from "./schemas/notifications";

export * from "./notificationsView";

export { NotificationSchema };

/**
 * What this list cannot do without. The sort below is `b.timestamp -
 * a.timestamp`, so a row carrying anything else makes the comparator NaN and the
 * unread order whatever the service happened to send; `categoryForType` and
 * `humanizeType` switch on `type`; `id` keys the row and is what mark-read sends
 * back. Everything else is metadata the renderer already scans defensively.
 *
 * `parseNotificationsLoose` (notificationsView.ts) applies the same gate by hand
 * for the bridge path and is unaffected by perf mode. This is that gate, for the
 * strict path -- which until now had it only in the checking build.
 */
export function isUsableNotification(row: unknown): boolean {
  return (
    isRecord(row) &&
    typeof row.id === "string" &&
    row.id !== "" &&
    typeof row.type === "string" &&
    row.type !== "" &&
    typeof row.timestamp === "number"
  );
}

export function parseNotifications(raw: unknown): Notification[] {
  let rows: unknown = raw;
  if (!Array.isArray(raw)) {
    const env = ListEnvelopeSchema.safeParse(raw);
    rows = env.success ? field(env.data, "notifications") : [];
  }
  const out: Notification[] = keepRows(
    rows,
    NotificationSchema,
    isUsableNotification,
    (row) => row,
  );
  out.sort((a, b) => b.timestamp - a.timestamp);
  return out;
}

export async function fetchLiveNotifications(
  opts: RequestOpts & { address?: string | null } = {},
): Promise<Notification[]> {
  const bridge = typeof window !== "undefined" ? window.dclBridge : undefined;
  if (bridge && typeof bridge.send === "function" && !opts.headers) {
    const res = await signedFetch(`${serviceBase("notifications")}/notifications`, {
      method: "GET",
    });
    if (res.status !== 200) {
      throw new Error(`notifications fetch failed: ${res.status}`);
    }
    return parseNotifications(JSON.parse(res.body));
  }
  const raw = await getJSON("/notifications", { service: "notifications", ...opts });
  return parseNotifications(raw);
}

export async function markNotificationsRead(
  ids: string[],
  opts: RequestOpts = {},
): Promise<void> {
  if (ids.length === 0) return;
  const bridge = typeof window !== "undefined" ? window.dclBridge : undefined;
  if (bridge && typeof bridge.send === "function" && !opts.headers) {
    const res = await signedFetch(`${serviceBase("notifications")}/notifications/read`, {
      method: "PUT",
      body: { notificationIds: ids },
    });
    if (res.status < 200 || res.status >= 300) {
      throw new Error(`notifications mark-read failed: ${res.status}`);
    }
    return;
  }
  await sendJSON("/notifications/read", {
    service: "notifications",
    method: "PUT",
    body: { notificationIds: ids },
    ...opts,
  });
}
