import type { QueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import Notifications from "../../explorer/components/Notifications";
import { useNotifications } from "../../data/hooks/useNotifications";
import {
  categoryForType,
  notificationTitle,
  notificationBody,
  notificationImage,
  relativeTime,
  fetchLiveNotifications,
  actorAddress,
  notificationLink,
} from "../../data/catalyst/notifications";
import type { Notification } from "../../data/catalyst/notifications";
import { qk, STALE } from "../../data/queryKeys";
import { getDeployIdentity } from "../../overlay/bridge";

const CATEGORY_GRADIENT: Record<string, string> = {
  friends: "linear-gradient(135deg,#ff8f5e,#c44dff)",
  gift: "linear-gradient(135deg,#5ee0ff,#3a6dff)",
  community: "linear-gradient(135deg,#6a2da8,#2d8cff)",
  badge: "linear-gradient(135deg,#ffb347,#ff7a00)",
  marketplace: "linear-gradient(135deg,#7a16a8,#c44dff)",
  system: "linear-gradient(135deg,#3a6dff,#6a2da8)",
};

type NotifAction = { label: string; primary?: boolean; to?: string; href?: string };

type NotifCategory =
  | "friends"
  | "badge"
  | "gift"
  | "community"
  | "marketplace"
  | "system";

type NotifCard = {
  id: string;
  type: NotifCategory;
  title: string;
  body: string;
  time: string;
  read: boolean;
  avatar?: string;
  thumb?: string;
  iconBg?: string;
  friendAddress?: string;
  actions?: NotifAction[];
};

function actionsForType(type: string | undefined): NotifAction[] | undefined {
  const t = String(type ?? "").toLowerCase();
  if (t.startsWith("social_service_friendship_request"))
    return [
      { label: "Accept", primary: true, to: "Explorer/Pages/Friends" },
      { label: "Decline", to: "Explorer/Pages/Friends" },
    ];
  if (t === "badge_granted")
    return [{ label: "View Badge", primary: true, to: "Explorer/Pages/BadgesDetails" }];
  if (t === "tip_received" || t === "transfer_received")
    return [{ label: "Open Gift", primary: true }];
  if (t === "community_voice_chat_started")
    return [{ label: "Join Stream", primary: true, to: "Explorer/Components/CommunityStream" }];
  if (t.startsWith("credits_"))
    return [{ label: "Claim", primary: true }];
  if (t === "reward_assignment")
    return [{ label: "View in Backpack", to: "Explorer/Pages/Backpack" }];
  if (
    t === "item_sold" ||
    t === "item_published" ||
    t === "bid_accepted" ||
    t === "bid_received" ||
    t === "royalties_earned"
  )
    return [{ label: "View", primary: true }];
  return undefined;
}

function toCard(n: Notification, now: number): NotifCard {
  const cat = categoryForType(n.type) as NotifCategory;
  const grad = CATEGORY_GRADIENT[cat] ?? CATEGORY_GRADIENT.system;
  const img = notificationImage(n);
  const bg = img ? `url("${img}") center/cover` : grad;

  const card: NotifCard = {
    id: n.id,
    type: cat,
    title: notificationTitle(n),
    body: notificationBody(n),
    time: relativeTime(n.timestamp, now),
    read: n.read,
  };

  if (cat === "friends" || cat === "gift") {
    card.avatar = bg;
  } else if (cat === "community" || img) {
    card.thumb = bg;
  } else {
    card.iconBg = grad;
  }

  let actions = actionsForType(n.type);
  if (n.type === "friend_first_wear") {
    const link = notificationLink(n);
    if (link) actions = [{ label: "Buy in Shop", primary: true, href: link }];
  }
  if (actions) card.actions = actions;
  if (cat === "friends") {
    const a = actorAddress(n);
    if (a) card.friendAddress = a;
  }
  return card;
}

export function prefetch(queryClient: QueryClient) {
  const address = getDeployIdentity()?.signerAddress;
  const headers =
    typeof window !== "undefined" ? window.__DCL_AUTH_HEADERS__ || undefined : undefined;
  return queryClient
    .prefetchQuery({
      queryKey: qk.notifications(address),
      queryFn: ({ signal }) => fetchLiveNotifications({ signal, headers, address }),
      staleTime: STALE.notifications,
      retry: false,
    })
    .catch(() => {});
}

type NotificationsPanelProps = { floating?: boolean };

export default function NotificationsPanel({ floating = false }: NotificationsPanelProps) {
  const { notifications, markRead, markAllRead } = useNotifications();

  const items = useMemo<NotifCard[] | undefined>(() => {
    if (!notifications) return undefined;
    const now = Date.now();
    return notifications.map((n: Notification) => toCard(n, now));
  }, [notifications]);

  const onMarkRead = useCallback((id: string) => markRead([id]), [markRead]);

  return (
    <Notifications
      bare
      floating={floating}
      onMarkRead={onMarkRead}
      onMarkAllRead={markAllRead}
      {...(items ? { items } : {})}
    />
  );
}
