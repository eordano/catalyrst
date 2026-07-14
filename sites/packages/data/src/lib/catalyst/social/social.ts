import type { AuthIdentity } from "../../auth/types";
import { getJSON, postJSON, signedGetJSON } from "../client";

function unwrapData<T = unknown>(env: unknown): T {
  const e = env as { data?: T } | null;
  return (e?.data ?? env) as T;
}

export type CommunityPost = {
  id: string;
  authorAddress: string;
  authorName: string | null;
  content: string;
  createdAt: string;
};

function toPost(node: unknown): CommunityPost | null {
  const o = (node ?? {}) as Record<string, unknown>;
  const id = typeof o.id === "string" ? o.id : null;
  const content = typeof o.content === "string" ? o.content : null;
  if (!id || content == null) return null;
  const address = typeof o.authorAddress === "string" ? o.authorAddress : "";
  const rawName = typeof o.authorName === "string" ? o.authorName.trim() : "";
  const name =
    rawName && rawName.toLowerCase() !== address.toLowerCase() && !/^0x[0-9a-f]{40}$/i.test(rawName)
      ? rawName
      : null;
  return {
    id,
    authorAddress: address,
    authorName: name,
    content,
    createdAt: typeof o.createdAt === "string" ? o.createdAt : "",
  };
}

export async function loadCommunityPosts(
  id: string,
  opts: { signal?: AbortSignal; limit?: number } = {},
): Promise<{ posts: CommunityPost[]; total: number }> {
  const raw = await getJSON<unknown>(`/v1/communities/${encodeURIComponent(id)}/posts`, {
    query: { limit: opts.limit ?? 50 },
    signal: opts.signal,
  });
  const data = unwrapData<{ posts?: unknown[]; total?: number }>(raw);
  const posts = Array.isArray(data?.posts)
    ? data.posts.map(toPost).filter((p): p is CommunityPost => p !== null)
    : [];
  const total = typeof data?.total === "number" ? data.total : posts.length;
  posts.reverse();
  return { posts, total };
}

export type CommunityMemberRow = {
  address: string;
  name: string | null;
  avatarUrl: string | null;
  role: string;
  isFriend: boolean;
};

const FRIENDSHIP_ACCEPTED = 3;

export async function loadCommunityMembers(
  id: string,
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<CommunityMemberRow[]> {
  const raw = await signedGetJSON<unknown>(
    `/v1/communities/${encodeURIComponent(id)}/members`,
    { identity, query: { limit: 100 }, signal },
  );
  const data = unwrapData<{ results?: unknown[] }>(raw);
  const list = Array.isArray(data?.results) ? data.results : [];
  return list
    .map((node): CommunityMemberRow | null => {
      const o = (node ?? {}) as Record<string, unknown>;
      const address = typeof o.memberAddress === "string" ? o.memberAddress : null;
      if (!address) return null;
      const rawName = typeof o.name === "string" ? o.name.trim() : "";
      const name =
        rawName && rawName.toLowerCase() !== address.toLowerCase() && !/^0x[0-9a-f]{40}$/i.test(rawName)
          ? rawName
          : null;
      const pic = typeof o.profilePictureUrl === "string" && o.profilePictureUrl ? o.profilePictureUrl : null;
      return {
        address,
        name,
        avatarUrl: pic,
        role: typeof o.role === "string" ? o.role : "member",
        isFriend: o.friendshipStatus === FRIENDSHIP_ACCEPTED,
      };
    })
    .filter((m): m is CommunityMemberRow => m !== null);
}

export async function postCommunityMessage(
  id: string,
  content: string,
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<CommunityPost> {
  const raw = await postJSON<unknown>(
    `/v1/communities/${encodeURIComponent(id)}/posts`,
    { content },
    { identity, signal },
  );
  const post = toPost(unwrapData(raw));
  if (!post) throw new Error("The channel returned an unexpected response.");
  return post;
}

export type Friend = {
  address: string;
  name: string | null;
  hasClaimedName: boolean;
  avatarUrl: string | null;
};

export type DirectMessage = {
  id: string;
  from: string;
  to: string;
  body: string;
  sentAt: string;
};

export async function loadFriends(
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<Friend[]> {
  const raw = await signedGetJSON<unknown>("/v1/friends", { identity, signal });
  const obj = (raw ?? {}) as { friends?: unknown[] };
  const list = Array.isArray(obj.friends) ? obj.friends : [];
  return list
    .map((node): Friend | null => {
      const o = (node ?? {}) as Record<string, unknown>;
      const address = typeof o.address === "string" ? o.address : null;
      if (!address) return null;
      return {
        address,
        name: typeof o.name === "string" && o.name.trim() ? o.name.trim() : null,
        hasClaimedName: o.hasClaimedName === true,
        avatarUrl: typeof o.avatarUrl === "string" && o.avatarUrl ? o.avatarUrl : null,
      };
    })
    .filter((f): f is Friend => f !== null);
}

function toDm(node: unknown): DirectMessage | null {
  const o = (node ?? {}) as Record<string, unknown>;
  const id = typeof o.id === "string" ? o.id : null;
  const body = typeof o.body === "string" ? o.body : null;
  if (!id || body == null) return null;
  return {
    id,
    from: typeof o.from === "string" ? o.from : "",
    to: typeof o.to === "string" ? o.to : "",
    body,
    sentAt: typeof o.sentAt === "string" ? o.sentAt : "",
  };
}

export async function loadDmThread(
  peer: string,
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<DirectMessage[]> {
  const raw = await signedGetJSON<unknown>(
    `/v1/friends/${encodeURIComponent(peer)}/messages`,
    { identity, query: { limit: 100 }, signal },
  );
  const obj = (raw ?? {}) as { messages?: unknown[] };
  const list = Array.isArray(obj.messages) ? obj.messages : [];
  return list.map(toDm).filter((m): m is DirectMessage => m !== null);
}

export async function sendDm(
  peer: string,
  body: string,
  identity: AuthIdentity,
  signal?: AbortSignal,
): Promise<DirectMessage> {
  const raw = await postJSON<unknown>(
    `/v1/friends/${encodeURIComponent(peer)}/messages`,
    { body },
    { identity, signal },
  );
  const obj = (raw ?? {}) as { message?: unknown };
  const msg = toDm(obj.message ?? raw);
  if (!msg) throw new Error("The message couldn't be sent.");
  return msg;
}

export function shortAddress(addr: string): string {
  const a = addr.trim();
  if (a.length <= 12) return a;
  return `${a.slice(0, 6)}\u{2026}${a.slice(-4)}`;
}

export function friendLabel(f: Friend): string {
  return f.name ?? shortAddress(f.address);
}

export function relativeTime(iso: string): string {
  if (!iso) return "";
  const t = Date.parse(iso.endsWith("Z") || /[+-]\d\d:?\d\d$/.test(iso) ? iso : `${iso}Z`);
  if (Number.isNaN(t)) return "";
  const diff = Date.now() - t;
  const s = Math.floor(diff / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  return new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
