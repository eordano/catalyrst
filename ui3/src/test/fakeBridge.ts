import type { BridgeApi, BridgePayloads } from "../overlay/bridge";
import type { BridgeAction } from "../generated/bridge/BridgeAction";
import type { OverlayPush } from "../generated/bridge/OverlayPush";
import type { FriendEntry } from "../generated/bridge/FriendEntry";
import type { FriendRequestEntry } from "../generated/bridge/FriendRequestEntry";

export type PushKind = OverlayPush["kind"];
export type PushOf<K extends PushKind> = Extract<OverlayPush, { kind: K }>;
type PushOverrides<K extends PushKind> = Partial<Omit<PushOf<K>, "kind">>;

export type SentCommand = {
  [K in BridgeAction]: { action: K; payload: BridgePayloads[K] };
}[BridgeAction];

export type SentMatcher<K extends BridgeAction> =
  | Partial<BridgePayloads[K]>
  | ((payload: BridgePayloads[K]) => boolean);

const DEFAULT_ADDRESS = "0x1234567890abcdef1234567890abcdef12345678";
const OTHER_ADDRESS = "0xaaaa567890abcdef1234567890abcdefccccbbbb";

export function makeFriend(over: Partial<FriendEntry> = {}): FriendEntry {
  return {
    address: OTHER_ADDRESS,
    name: "Ripley",
    hasClaimedName: true,
    profilePictureUrl: "",
    status: "online",
    ...over,
  };
}

export function makeFriendRequest(
  over: Partial<FriendRequestEntry> = {},
): FriendRequestEntry {
  return {
    id: "req-1",
    createdAt: BigInt(Date.UTC(2026, 0, 15)),
    message: null,
    friend: {
      address: OTHER_ADDRESS,
      name: "Hicks",
      hasClaimedName: false,
      profilePictureUrl: "",
    },
    ...over,
  };
}

function payloadMatches(payload: unknown, partial: Record<string, unknown>): boolean {
  if (typeof payload !== "object" || payload === null) return false;
  const p = payload as Record<string, unknown>;
  return Object.entries(partial).every(
    ([k, v]) => JSON.stringify(p[k]) === JSON.stringify(v),
  );
}

export class FakeBridge implements BridgeApi {
  readonly sent: SentCommand[] = [];

  private subscribers = new Set<(push: unknown) => void>();

  wrapDispatch: (fn: () => void) => void = (fn) => fn();


  send = (action: string, payload?: unknown): void => {
    this.sent.push({ action, payload } as SentCommand);
  };

  onState = (cb: (push: unknown) => void): (() => void) => {
    this.subscribers.add(cb);
    return () => this.subscribers.delete(cb);
  };

  get subscriberCount(): number {
    return this.subscribers.size;
  }


  push(p: OverlayPush): OverlayPush {
    this.wrapDispatch(() => {
      for (const cb of [...this.subscribers]) cb(p);
    });
    return p;
  }

  pushIdentity(over: PushOverrides<"identity"> = {}): PushOf<"identity"> {
    return this.push({
      kind: "identity",
      address: DEFAULT_ADDRESS,
      signerAddress: DEFAULT_ADDRESS,
      isGuest: false,
      name: "TestPilot",
      tag: "1234",
      ...over,
    }) as PushOf<"identity">;
  }

  pushScene(over: PushOverrides<"scene"> = {}): PushOf<"scene"> {
    return this.push({
      kind: "scene",
      title: "Genesis Plaza",
      coords: "0,0",
      realm: "main",
      ...over,
    }) as PushOf<"scene">;
  }

  pushLoading(over: PushOverrides<"loading"> = {}): PushOf<"loading"> {
    return this.push({
      kind: "loading",
      percent: 100,
      ready: true,
      avatarLoaded: true,
      ...over,
    }) as PushOf<"loading">;
  }

  pushFriends(over: PushOverrides<"friends"> = {}): PushOf<"friends"> {
    const friends = over.friends ?? [];
    return this.push({
      kind: "friends",
      onlineCount:
        over.onlineCount ?? friends.filter((f) => f.status === "online").length,
      friends,
      received: [],
      sent: [],
      ...over,
    }) as PushOf<"friends">;
  }

  pushConnection(over: PushOverrides<"connection"> = {}): PushOf<"connection"> {
    return this.push({
      kind: "connection",
      sceneHealth: "ok",
      sceneRoom: false,
      globalRoom: true,
      ...over,
    }) as PushOf<"connection">;
  }

  pushChat(over: PushOverrides<"chat"> = {}): PushOf<"chat"> {
    return this.push({
      kind: "chat",
      senderName: "Ripley",
      senderAddress: OTHER_ADDRESS,
      message: "hello nearby",
      channel: "Nearby",
      timestamp: Date.now(),
      ...over,
    }) as PushOf<"chat">;
  }

  pushMic(over: PushOverrides<"mic"> = {}): PushOf<"mic"> {
    return this.push({
      kind: "mic",
      enabled: false,
      available: true,
      ...over,
    }) as PushOf<"mic">;
  }

  pushLoginCode(over: PushOverrides<"loginCode"> = {}): PushOf<"loginCode"> {
    return this.push({
      kind: "loginCode",
      code: 4242,
      url: "https://decentraland.org/auth/requests/test",
      ...over,
    }) as PushOf<"loginCode">;
  }

  pushPermissionRequest(
    over: PushOverrides<"permissionRequest"> = {},
  ): PushOf<"permissionRequest"> {
    return this.push({
      kind: "permissionRequest",
      id: 7,
      ty: "ChangeRealm",
      scene: "bafkscenehash",
      sceneName: "Genesis Plaza",
      additional: null,
      title: "Change Realm",
      request: "The scene wants permission to move you to a new realm",
      ...over,
    }) as PushOf<"permissionRequest">;
  }


  sentOf<K extends BridgeAction>(action: K): BridgePayloads[K][] {
    return this.sent
      .filter((s) => s.action === action)
      .map((s) => s.payload as BridgePayloads[K]);
  }

  lastSent<K extends BridgeAction>(action: K): BridgePayloads[K] | undefined {
    const all = this.sentOf(action);
    return all[all.length - 1];
  }

  expectSent<K extends BridgeAction>(
    action: K,
    matcher?: SentMatcher<K>,
  ): BridgePayloads[K] {
    const all = this.sentOf(action);
    const matches =
      matcher === undefined
        ? all
        : all.filter((p) =>
            typeof matcher === "function"
              ? matcher(p)
              : payloadMatches(p, matcher as Record<string, unknown>),
          );
    if (matches.length === 0) {
      const wanted =
        matcher && typeof matcher !== "function"
          ? ` matching ${JSON.stringify(matcher)}`
          : "";
      throw new Error(
        `expected bridge send "${action}"${wanted}; recorded sends: ` +
          JSON.stringify(
            this.sent.map((s) => ({ action: s.action, payload: s.payload })),
            (_k, v: unknown) => (typeof v === "bigint" ? String(v) : v),
          ),
      );
    }
    return matches[matches.length - 1] as BridgePayloads[K];
  }

  expectNotSent<K extends BridgeAction>(action: K, matcher?: SentMatcher<K>): void {
    try {
      this.expectSent(action, matcher);
    } catch {
      return;
    }
    throw new Error(`expected NO bridge send "${action}", but one was recorded`);
  }

  clearSent(): void {
    this.sent.length = 0;
  }
}
