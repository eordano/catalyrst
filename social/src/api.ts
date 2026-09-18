import { reportError } from "./telemetry";
import { walletSession } from "./session";
export const basePath = new URL(".", document.baseURI).pathname;

export type AuthLink = { type: string; payload: string; signature: string };
export type Scene = { x: number; y: number; world?: string };
export type MemberAction = {kind:"role";address:string;role:"member"|"moderator"} | {kind:"remove"|"ban"|"unban"|"invite";address:string} | {kind:"bans";offset:number} | {kind:"leave"};
export type ChannelConfig = {private:boolean;members:string[];read_only:boolean;slow_mode:number};
export type EventDraft = {name: string; description: string; start_at: string; duration: number; scene: Scene};
export type Operation =
  | {type:"create_event"; event: EventDraft}
  | { type: "active_community_voice_chats" }
  | { type: "community_places"; community_id: string; offset?: number }
  | { type: "add_community_places"; community_id: string; place_ids: string[] }
  | { type: "remove_community_place"; community_id: string; place_id: string }

  | {type:"my_events"}
  | {type:"event_attendees";event_id:string}
  | {type:"event_rsvp";event_id:string;attending:boolean}
  | {type:"community_reports";community_id:string}
  | {type:"configure_channel";community_id:string;channel:string;config:ChannelConfig}
  | { type: "my_communities" | "private_chat_token" | "social_connection" | "own_location" }
  | { type: "create_channel"; community_id: string; name: string; private: boolean }
  | { type: "open_community"; community_id: string; channel?: string }
  | { type: "community_posts" | "community_details" | "join_community" | "request_community_join"; community_id: string }
  | { type: "create_community"; name: string; description: string; privacy: "public" | "private"; visibility?: "all" | "unlisted"; thumbnail?: string }
  | { type: "update_community"; community_id: string; name: string; description: string; privacy: "public" | "private"; visibility?: "all" | "unlisted"; thumbnail?: string }
  | { type: "manage_community"; community_id:string; action:MemberAction }
  | { type: "my_join_requests" | "my_community_invitations"; offset?: number }
  | { type: "cancel_community_request"; community_id: string; request_id: string }
  | { type: "community_requests"; community_id: string; offset: number }
  | { type: "resolve_request"; community_id: string; request_id: string; accept: boolean }
  | { type: "community_members"; community_id: string; offset: number }
  | { type: "message_action"; community_id: string; message_id: string; action: "heart" | "celebrate" | "pin" | "unpin" | "report" | "dismiss" | "remove" }
  | { type: "reply"; community_id: string; message_id: string; text: string }
  | { type: "publish_post"; community_id: string; content: string }
  | {
      type: "send_message";
      channel?: string;
      community_id: string;
      text: string;
      scene: Scene | null;
    };
export type Prepared = {
  id: string;
  wallet: string;
  operation: Operation;
  url: string;
  method: string;
  body: string | null;
  payload: string;
  metadata: string;
  timestamp: string;
  expiresAt: number;
};
/** Existing SDK/wallet session supplies this bridge. It owns all signing keys. */
export interface WalletIdentity {
  address: string;
  signRequest(request: Prepared): Promise<AuthLink[]>;
  /** Only true for an already-authorized ephemeral signer, never for wallet prompts. */
  canSignSilently?: boolean;
  expiresAt?: number;
}
declare global {
  interface Window {
    dclSocialIdentity?: WalletIdentity;
    ethereum?: {
      request(args: { method: string; params?: unknown[] }): Promise<unknown>;
      on?(event: string, listener: () => void): void;
      removeListener?(event: string, listener: () => void): void;
    };
  }
}
export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
  }
}
export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${basePath}api${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...init?.headers },
    cache: "no-store",
  }).catch((error: unknown) => {
    if (!(error instanceof DOMException && error.name === "AbortError")) reportError(error, "network");
    throw error;
  });
  if (response.status >= 500) reportError(`API returned ${response.status}`, "api");
  const body = await response.json().catch(() => {
    throw new ApiError(
      "The server returned an unreadable response. Try again.",
      response.status,
    );
  });
  if (!response.ok)
    throw new ApiError(body.error || "Request failed", response.status);
  return body;
}
export async function existingIdentity(connect = false): Promise<WalletIdentity | null> {
  if (window.dclSocialIdentity) return window.dclSocialIdentity;
  return walletSession(connect);
}
export async function execute<T>(
  identity: WalletIdentity,
  operation: Operation,
  isCurrent: () => boolean = () => true,
): Promise<T> {
  const prepared = await api<Prepared>("/actions", {
    method: "POST",
    body: JSON.stringify({ wallet: identity.address, operation }),
  });
  if (!isCurrent()) throw new DOMException("Navigation changed", "AbortError");
  // The host wallet receives the complete immutable intent for its approval UI.
  const authChain = await identity.signRequest(prepared);
  if (!isCurrent()) throw new DOMException("Navigation changed", "AbortError");
  return api<T>(`/actions/${prepared.id}/complete`, {
    method: "POST",
    body: JSON.stringify({ authChain }),
  });
}
export type Community = {
  id: string;
  name: string;
  description: string;
  membersCount: number;
  role?: string;
  privacy?: "public" | "private";
  visibility?: "all" | "unlisted";
  pendingRequestToJoin?: boolean;
  thumbnails?: Record<string, string>;
};
export type Message = {
  replyCount?: number;
  hasMoreReplies?: boolean;
  replies?: { seq:number; id: string; wallet: string; text: string; createdAt: number }[];
  reactions?: { emoji: string; wallet: string }[];
  pinned?: boolean;
  seq: number;
  id: string;
  wallet: string;
  text: string;
  scene: Scene | null;
  createdAt: number;
};
export type Post = {
  id: string;
  content: string;
  authorName?: string;
  authorAddress: string;
  createdAt: string;
};
export type Opened = {
  channelConfig?:ChannelConfig;
  channel?: string;
  channels?: {name: string; private: boolean}[];
  community: Community;
  messages: Message[];
  readToken: string;
  expiresAt: number;
};
export const shortWallet = (wallet?: string) =>
  wallet ? `${wallet.slice(0, 6)}\u2026${wallet.slice(-4)}` : "Unknown member";
