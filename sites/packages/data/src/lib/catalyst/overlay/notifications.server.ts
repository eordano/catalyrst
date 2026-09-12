import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { parseNotifications, type Notification } from "./notifications";

export type NotificationsData = {
  address: string;
  notifications: Notification[];
  unavailable: string | null;
};

function unreadable(reason: string): NotificationsData {
  return { address: "", notifications: [], unavailable: reason };
}

export async function loadNotifications(
  opts: GetOptions = {},
): Promise<NotificationsData> {
  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/notifications", opts);
  } catch (err) {
    return unreadable((err as Error)?.message ?? "network error");
  }

  const rows = (raw as { notifications?: unknown } | null)?.notifications;
  if (!Array.isArray(rows)) {
    return unreadable("/notifications did not return a notifications array");
  }

  const parsed = parseNotifications(raw);
  return {
    address: parsed[0]?.address ?? "",
    notifications: parsed,
    unavailable: null,
  };
}
