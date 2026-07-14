
export type Notification = {
  id: string;
  type: string;
  address: string;
  timestamp: number;
  read: boolean;
  created_at: string;
  updated_at: string;
  metadata: Record<string, unknown>;
};

export const NOTIFICATION_CATEGORIES = [
  "friends",
  "badge",
  "gift",
  "community",
  "marketplace",
  "system",
];

export function categoryForType(type: unknown): string {
  const t = String(type ?? "").toLowerCase();
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

export function humanizeType(type: unknown): string {
  return String(type ?? "")
    .split("_")
    .map((w) => (w ? w.charAt(0).toUpperCase() + w.slice(1) : w))
    .join(" ");
}

function str(m: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const k of keys) {
    const v = m[k];
    if (typeof v === "string" && v) return v;
  }
  return undefined;
}

function nested(m: Record<string, unknown>, parent: string, ...keys: string[]): string | undefined {
  const p = m[parent];
  if (p && typeof p === "object") return str(p as Record<string, unknown>, ...keys);
  return undefined;
}

const cname = (m: Record<string, unknown>): string => str(m, "communityName") ?? "a community";

// Friendship notifications carry the OTHER user under metadata.sender (name/avatar) and have no
// metadata.title — show the friend's name + a readable action, mirroring unity-explorer.
const FRIENDSHIP_BODY: Record<string, string> = {
  social_service_friendship_request: "wants to be your friend!",
  social_service_friendship_accepted: "accepted your friend request.",
};

// Notification types whose metadata has NO server-rendered title (community_*, credit reminders, …):
// the generic path would show the raw humanized type ("Community Post Added"), so build the same
// readable copy unity-explorer uses (a header + a line composed from the metadata fields).
const NOTIFICATION_TEMPLATES: Record<
  string,
  (m: Record<string, unknown>) => { title: string; body?: string }
> = {
  community_post_added: (m) => ({
    title: "New Community Announcement",
    body: `A new announcement was posted in ${cname(m)}.`,
  }),
  community_invite_received: (m) => ({
    title: "Community Invite Received",
    body: `You've been invited to join ${cname(m)}.`,
  }),
  community_voice_chat_started: (m) => ({
    title: "Community Voice Stream Started",
    body: `${cname(m)} is streaming — click to join.`,
  }),
  community_renamed: (m) => ({
    title: "Community Renamed",
    body: `${str(m, "oldName") ?? "A community"} was renamed to ${str(m, "newName") ?? cname(m)}.`,
  }),
  community_request_to_join_received: (m) => ({
    title: "Membership Request Received",
    body: `${str(m, "userName", "memberName") ?? "Someone"} wants to join ${cname(m)}.`,
  }),
  community_request_to_join_accepted: (m) => ({
    title: "Membership Request Accepted",
    body: `You're now a member of ${cname(m)}.`,
  }),
  community_deleted: (m) => ({
    title: "Community Deleted",
    body: `${cname(m)} has been deleted.`,
  }),
  community_deleted_content_violation: (m) => ({
    title: "Community Deleted",
    body: `${cname(m)} was deleted for a content violation.`,
  }),
  community_member_banned: (m) => ({
    title: "Banned from Community",
    body: `You've been banned from ${cname(m)}.`,
  }),
  community_member_removed: (m) => ({
    title: "Removed from Community",
    body: `You've been removed from ${cname(m)}.`,
  }),
  community_ownership_transferred: (m) => ({
    title: "Community Ownership Transferred",
    body: `You're now the owner of ${cname(m)}.`,
  }),
  credits_reminder_claim_credits: () => ({
    title: "Marketplace Credits",
    body: "You have Credits waiting to be claimed.",
  }),
  credits_reminder_complete_goals: () => ({
    title: "Marketplace Credits",
    body: "Complete your goals to earn Credits.",
  }),
  credits_reminder_usage: () => ({
    title: "Marketplace Credits",
    body: "Don't forget to use your Credits.",
  }),
  credits_reminder_usage_24_hours: () => ({
    title: "Marketplace Credits",
    body: "Your Credits expire soon — use them now.",
  }),
  credits_reminder_do_not_miss_out: () => ({
    title: "Marketplace Credits",
    body: "Don't miss out on your Credits.",
  }),
  credits_new_season_reminder: () => ({
    title: "New Credits Season",
    body: "A new Marketplace Credits season has started.",
  }),
};

function templateFor(
  n: Notification | null | undefined,
): { title: string; body?: string } | undefined {
  const type = n?.type ?? "";
  const m = n?.metadata ?? {};
  if (FRIENDSHIP_BODY[type]) {
    return { title: nested(m, "sender", "name") ?? str(m, "title") ?? "Someone", body: FRIENDSHIP_BODY[type] };
  }
  const tmpl = NOTIFICATION_TEMPLATES[type];
  return tmpl ? tmpl(m) : undefined;
}

export function notificationTitle(n: Notification | null | undefined): string {
  const t = n?.metadata?.title;
  if (typeof t === "string" && t.trim()) return t;
  return templateFor(n)?.title ?? humanizeType(n?.type);
}

export function notificationBody(n: Notification | null | undefined): string {
  const d = n?.metadata?.description;
  if (typeof d === "string" && d) return d;
  return templateFor(n)?.body ?? "";
}

export function notificationLink(n: Notification | null | undefined): string | null {
  const l = n?.metadata?.link;
  return typeof l === "string" && /^https?:\/\//.test(l) ? l : null;
}

export function notificationImage(n: Notification | null | undefined): string | null {
  const m = n?.metadata ?? {};
  for (const k of ["badgeImageUrl", "thumbnailUrl", "tokenImage", "imageUrl", "image"]) {
    const v = m[k];
    if (typeof v === "string" && v.trim()) return v;
  }
  return null;
}

const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;

export function actorAddress(n: Notification | null | undefined): string | null {
  const recipient = (n?.address ?? "").toLowerCase();
  let found: string | null = null;
  const scan = (v: unknown, depth: number): void => {
    if (found) return;
    if (typeof v === "string") {
      if (ADDR_RE.test(v) && v.toLowerCase() !== recipient) found = v;
      return;
    }
    if (v && typeof v === "object" && depth < 2) {
      for (const val of Object.values(v as Record<string, unknown>)) scan(val, depth + 1);
    }
  };
  scan(n?.metadata ?? {}, 0);
  return found;
}

export function unreadCount(rows?: Notification[] | null): number {
  return (rows ?? []).filter((n) => !n.read).length;
}

export function parseNotificationsLoose(raw: unknown): Notification[] {
  const rows: unknown[] = Array.isArray(raw)
    ? raw
    : raw && typeof raw === "object" &&
        Array.isArray((raw as { notifications?: unknown }).notifications)
      ? ((raw as { notifications: unknown[] }).notifications)
      : [];
  const out: Notification[] = [];
  for (const item of rows) {
    if (!item || typeof item !== "object") continue;
    const r = item as Record<string, unknown>;
    if (typeof r.id !== "string" || !r.id || typeof r.type !== "string" || !r.type) continue;
    out.push({
      id: r.id,
      type: r.type,
      address: typeof r.address === "string" ? r.address : "",
      timestamp: typeof r.timestamp === "number" ? r.timestamp : 0,
      read: typeof r.read === "boolean" ? r.read : false,
      created_at: typeof r.created_at === "string" ? r.created_at : "",
      updated_at: typeof r.updated_at === "string" ? r.updated_at : "",
      metadata:
        r.metadata && typeof r.metadata === "object" && !Array.isArray(r.metadata)
          ? (r.metadata as Record<string, unknown>)
          : {},
    });
  }
  out.sort((a, b) => b.timestamp - a.timestamp);
  return out;
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
