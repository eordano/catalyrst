import { randomBytes, timingSafeEqual } from "node:crypto";

export const PAIR_TTL_MS = 5 * 60_000;
const MAX_SESSIONS = 500;

export type PairSession = {
  id: string;
  pollToken: string;
  ephemeral: string;
  expiration: string;
  message: string;
  createdAt: number;
  expiresAt: number;
  completed: { signer: string; signature: string } | null;
};

export type PairPollResult =
  | { state: "missing" }
  | { state: "forbidden" }
  | { state: "expired" }
  | { state: "pending" }
  | { state: "completed"; signer: string; signature: string };

export type PairCompleteResult = "ok" | "missing" | "expired" | "already";

export type PairStore = {
  create(input: {
    ephemeral: string;
    expiration: string;
    message: string;
  }): PairSession | null;
  get(id: string): PairSession | null;
  complete(id: string, signer: string, signature: string): PairCompleteResult;
  poll(id: string, pollToken: string): PairPollResult;
  cancel(id: string, pollToken: string): boolean;
};

function tokenMatches(a: string, b: string): boolean {
  const ab = Buffer.from(a);
  const bb = Buffer.from(b);
  return ab.length === bb.length && timingSafeEqual(ab, bb);
}

export function createMemoryPairStore(ttlMs = PAIR_TTL_MS): PairStore {
  const sessions = new Map<string, PairSession>();

  function sweep(): void {
    const now = Date.now();
    for (const [id, s] of sessions) {
      if (now > s.expiresAt) sessions.delete(id);
    }
  }

  return {
    create(input) {
      sweep();
      if (sessions.size >= MAX_SESSIONS) return null;
      const now = Date.now();
      const session: PairSession = {
        id: randomBytes(16).toString("base64url"),
        pollToken: randomBytes(32).toString("base64url"),
        ephemeral: input.ephemeral,
        expiration: input.expiration,
        message: input.message,
        createdAt: now,
        expiresAt: now + ttlMs,
        completed: null,
      };
      sessions.set(session.id, session);
      return session;
    },
    get(id) {
      sweep();
      return sessions.get(id) ?? null;
    },
    complete(id, signer, signature) {
      sweep();
      const s = sessions.get(id);
      if (!s) return "missing";
      if (Date.now() > s.expiresAt) {
        sessions.delete(id);
        return "expired";
      }
      if (s.completed) return "already";
      s.completed = { signer, signature };
      return "ok";
    },
    poll(id, pollToken) {
      sweep();
      const s = sessions.get(id);
      if (!s) return { state: "missing" };
      if (!tokenMatches(pollToken, s.pollToken)) return { state: "forbidden" };
      if (Date.now() > s.expiresAt) {
        sessions.delete(id);
        return { state: "expired" };
      }
      if (!s.completed) return { state: "pending" };
      sessions.delete(id);
      return { state: "completed", ...s.completed };
    },
    cancel(id, pollToken) {
      const s = sessions.get(id);
      if (!s || !tokenMatches(pollToken, s.pollToken)) return false;
      sessions.delete(id);
      return true;
    },
  };
}

export const pairStore: PairStore = createMemoryPairStore();

const RATE_WINDOW_MS = 5 * 60_000;
const RATE_MAX_CREATES = 30;
const rateBuckets = new Map<string, { windowStart: number; count: number }>();

export function allowPairCreate(ip: string, now = Date.now()): boolean {
  if (rateBuckets.size > 10_000) {
    for (const [key, b] of rateBuckets) {
      if (now - b.windowStart > RATE_WINDOW_MS) rateBuckets.delete(key);
    }
  }
  const bucket = rateBuckets.get(ip);
  if (!bucket || now - bucket.windowStart > RATE_WINDOW_MS) {
    rateBuckets.set(ip, { windowStart: now, count: 1 });
    return true;
  }
  bucket.count += 1;
  return bucket.count <= RATE_MAX_CREATES;
}
