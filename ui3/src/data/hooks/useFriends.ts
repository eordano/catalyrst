import { useEffect, useMemo, useState } from "react";
import { useQuery, type QueryClient } from "@tanstack/react-query";

import { qk, STALE } from "../queryKeys";
import { getBridge, subscribeBridge } from "../../overlay/bridge";
import type { FriendEntry } from "../../generated/bridge/FriendEntry";
import type { FriendRef } from "../../generated/bridge/FriendRef";
import type { FriendRequestEntry } from "../../generated/bridge/FriendRequestEntry";
import type { OverlayPush } from "../../generated/bridge/OverlayPush";

export { FRIEND_ACTIONS, requestFriendAction } from "./friendActions";

const SELF_KEY = "self";

const MONTHS = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
  "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

type NameColor = { r?: unknown; g?: unknown; b?: unknown } | null | undefined;

type RawFriend = Partial<FriendEntry> & {
  nameColor?: NameColor;
  online?: boolean;
  where?: string;
};

type RawRequest = Partial<Omit<FriendRequestEntry, "friend">> & {
  friend?: Partial<FriendRef> & { nameColor?: NameColor };
};

type RawBlocked = Partial<FriendRef> & {
  nameColor?: NameColor;
  blockedAt?: number;
};

export type FriendStatus = "online" | "away" | "offline";

export type NormalizedFriend = {
  address: string;
  name: string;
  tag: string;
  hue: number;
  online: boolean;
  status: FriendStatus;
  where: string;
  profilePictureUrl: string;
  hasClaimedName: boolean;
};

export type NormalizedRequest = {
  id: string;
  address: string;
  name: string;
  tag: string;
  hue: number;
  date: string;
  message: string;
  profilePictureUrl: string;
  hasClaimedName: boolean;
};

export type NormalizedBlocked = {
  address: string;
  name: string;
  tag: string;
  hue: number;
  date: string;
  profilePictureUrl: string;
  hasClaimedName: boolean;
};

export type FriendsData = {
  self: unknown;
  friends: NormalizedFriend[];
  received: NormalizedRequest[];
  sent: NormalizedRequest[];
  blocked: NormalizedBlocked[];
  onlineCount: number;
};

function isHttpUrl(s: unknown): boolean {
  return typeof s === "string" && /^https?:\/\//i.test(s);
}

function shortTag(address: unknown): string {
  if (!address || typeof address !== "string") return "";
  const hex = address.replace(/^0x/i, "");
  return "#" + hex.slice(-4);
}

function hueFromNameColor(color: NameColor): number {
  if (!color) return 210;
  const r = Number(color.r) || 0;
  const g = Number(color.g) || 0;
  const b = Number(color.b) || 0;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  if (delta === 0) return 0;
  let h;
  if (max === r) h = ((g - b) / delta) % 6;
  else if (max === g) h = (b - r) / delta + 2;
  else h = (r - g) / delta + 4;
  h = Math.round(h * 60);
  return h < 0 ? h + 360 : h;
}

function formatDate(ms?: number | bigint): string {
  if (!ms) return "";
  const d = new Date(Number(ms));
  if (Number.isNaN(d.getTime())) return "";
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}`;
}

function normalizeStatus(raw: string | undefined, fallbackOnline: boolean | undefined): FriendStatus {
  if (raw === "online" || raw === "away" || raw === "offline") return raw;
  return fallbackOnline ? "online" : "offline";
}

function normalizeFriend(f: RawFriend | null | undefined): NormalizedFriend | null {
  if (!f) return null;
  const status = normalizeStatus(f.status, f.online);
  const online = status !== "offline";
  return {
    address: f.address ?? "",
    name: f.name ?? "unknown",
    tag: shortTag(f.address),
    hue: hueFromNameColor(f.nameColor),
    online,
    status,
    where: status === "online" ? f.where || "Decentraland" : status === "away" ? "Away" : "Offline",
    profilePictureUrl: isHttpUrl(f.profilePictureUrl) ? (f.profilePictureUrl as string) : "",
    hasClaimedName: !!f.hasClaimedName,
  };
}

function normalizeRequest(r: RawRequest | null | undefined): NormalizedRequest | null {
  if (!r) return null;
  const f = r.friend ?? {};
  return {
    id: r.id ?? f.address ?? "",
    address: f.address ?? "",
    name: f.name ?? "unknown",
    tag: shortTag(f.address),
    hue: hueFromNameColor(f.nameColor),
    date: formatDate(r.createdAt),
    message: r.message || "",
    profilePictureUrl: isHttpUrl(f.profilePictureUrl) ? (f.profilePictureUrl as string) : "",
    hasClaimedName: !!f.hasClaimedName,
  };
}

function normalizeBlocked(b: RawBlocked | null | undefined): NormalizedBlocked | null {
  if (!b) return null;
  return {
    address: b.address ?? "",
    name: b.name ?? "unknown",
    tag: shortTag(b.address),
    hue: hueFromNameColor(b.nameColor),
    date: formatDate(b.blockedAt),
    profilePictureUrl: isHttpUrl(b.profilePictureUrl) ? (b.profilePictureUrl as string) : "",
    hasClaimedName: !!b.hasClaimedName,
  };
}

type RawFriendsData = {
  self?: unknown;
  friends?: RawFriend[];
  received?: RawRequest[];
  sent?: RawRequest[];
  blocked?: RawBlocked[];
};

export function normalizeFriends(raw: unknown): FriendsData {
  const r = (raw ?? {}) as RawFriendsData;
  const friends = (r.friends ?? [])
    .map(normalizeFriend)
    .filter((x): x is NormalizedFriend => x !== null);
  const received = (r.received ?? [])
    .map(normalizeRequest)
    .filter((x): x is NormalizedRequest => x !== null);
  const sent = (r.sent ?? [])
    .map(normalizeRequest)
    .filter((x): x is NormalizedRequest => x !== null);
  const blocked = (r.blocked ?? [])
    .map(normalizeBlocked)
    .filter((x): x is NormalizedBlocked => x !== null);
  return {
    self: r.self ?? null,
    friends,
    received,
    sent,
    blocked,
    onlineCount: friends.filter((f) => f.online).length,
  };
}

type FriendsPush = Extract<OverlayPush, { kind: "friends" }>;

export function adaptBridgeFriends(push: FriendsPush | null): RawFriendsData | null {
  if (!push) return null;
  const friends = push.friends ?? [];
  const byAddress = new Map(friends.map((f) => [f.address.toLowerCase(), f]));
  return {
    friends: friends.map((f) => ({
      address: f.address,
      name: f.name,
      hasClaimedName: f.hasClaimedName,
      profilePictureUrl: f.profilePictureUrl,
      status: f.status,
    })),
    received: push.received ?? [],
    sent: push.sent ?? [],
    // push.blocked is the union of both directions (the chat filter set); the panel
    // must list only addresses the user blocked, never who blocked them. The wire
    // carries bare addresses, but blocked friends stay in the friends push, so the
    // row keeps its name and picture instead of degrading to "unknown".
    blocked: (push.blockedByMe ?? []).map((address) => {
      const f = byAddress.get(address.toLowerCase());
      return f
        ? {
            address,
            name: f.name,
            hasClaimedName: f.hasClaimedName,
            profilePictureUrl: f.profilePictureUrl,
          }
        : { address };
    }),
  };
}

function useBridgeFriendsPush(): FriendsPush | null {
  const [push, setPush] = useState<FriendsPush | null>(null);
  useEffect(() => {
    let unsub: () => void = () => {};
    let cancelled = false;
    let iv: ReturnType<typeof setInterval> | null = null;
    let to: ReturnType<typeof setTimeout> | null = null;
    const attach = () => {
      if (!getBridge()) return false;
      unsub = subscribeBridge((p: unknown) => {
        if (p && typeof p === "object" && (p as { kind?: unknown }).kind === "friends") {
          setPush(p as FriendsPush);
        }
      });
      return true;
    };
    if (!attach()) {
      iv = setInterval(() => {
        if (cancelled) return;
        if (attach() && iv) {
          clearInterval(iv);
          iv = null;
        }
      }, 250);
      to = setTimeout(() => iv && clearInterval(iv), 10000);
    }
    return () => {
      cancelled = true;
      if (iv) clearInterval(iv);
      if (to) clearTimeout(to);
      try {
        unsub();
      } catch {
      }
    };
  }, []);
  return push;
}

export async function fetchFriends(
  { signal }: { signal?: AbortSignal } = {},
): Promise<FriendsData> {
  if (signal?.aborted) {
    throw new DOMException("Friends read aborted", "AbortError");
  }
  return normalizeFriends({});
}

export function prefetchFriends(queryClient: QueryClient) {
  return queryClient.prefetchQuery({
    queryKey: qk.friends(SELF_KEY),
    queryFn: ({ signal }) => fetchFriends({ signal }),
    staleTime: STALE.friends,
  });
}

const EMPTY: readonly never[] = Object.freeze([]);

export function useFriends() {
  const query = useQuery({
    queryKey: qk.friends(SELF_KEY),
    queryFn: ({ signal }) => fetchFriends({ signal }),
    staleTime: STALE.friends,
  });

  const push = useBridgeFriendsPush();
  const data = useMemo(
    () => (push ? normalizeFriends(adaptBridgeFriends(push)) : query.data),
    [push, query.data],
  );
  const friends = data?.friends ?? EMPTY;
  const received = data?.received ?? EMPTY;
  const sent = data?.sent ?? EMPTY;
  const blocked = data?.blocked ?? EMPTY;

  const isEmpty = useMemo(
    () =>
      !!data &&
      friends.length === 0 &&
      received.length === 0 &&
      sent.length === 0 &&
      blocked.length === 0,
    [data, friends, received, sent, blocked],
  );

  return {
    ...query,
    data,
    friends,
    received,
    sent,
    blocked,
    onlineCount: data?.onlineCount ?? 0,
    reqCount: received.length + sent.length,
    isEmpty,
  };
}

