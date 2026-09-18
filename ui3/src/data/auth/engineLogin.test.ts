import { describe, expect, test } from "vitest";

import type { SetIdentityPayload } from "../../generated/bridge/SetIdentityPayload";
import type { AuthIdentity } from "./identity";
import {
  AUTO_JUMP_IN_MIN_VALIDITY_MS,
  IDENTITY_STORAGE_KEY,
  createEngineAuth,
  identitySigner,
  isExpired,
  loginIdentityCommand,
  setIdentityPayload,
  shouldAutoJumpIn,
  toStoredIdentity,
  type StoredAuthIdentity,
} from "./engineLogin";

const SIGNER = "0xAbCd000000000000000000000000000000000001";
const EPHEMERAL_ADDRESS = "0x1111111111111111111111111111111111111111";
const EPHEMERAL_KEY =
  "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

function makeIdentity(expirationMs = Date.now() + 86_400_000): AuthIdentity {
  const expiration = new Date(expirationMs).toISOString();
  const message = [
    "Decentraland Login",
    `Ephemeral address: ${EPHEMERAL_ADDRESS}`,
    `Expiration: ${expiration}`,
  ].join("\n");
  return {
    signer: SIGNER.toLowerCase(),
    ephemeral: { address: EPHEMERAL_ADDRESS, privateKey: EPHEMERAL_KEY },
    expiration,
    authChain: [
      { type: "SIGNER", payload: SIGNER, signature: "" },
      { type: "ECDSA_EPHEMERAL", payload: message, signature: "0xsigsig" },
    ],
  };
}

type Sent = { action: string; payload: unknown };

function makeFakeBridge() {
  const sent: Sent[] = [];
  const subs = new Set<(p: unknown) => void>();
  const lastByKind = new Map<string, unknown>();
  const bridge = {
    send: (action: string, payload?: unknown) => {
      sent.push({ action, payload });
    },
    onState: (cb: (p: unknown) => void) => {
      subs.add(cb);
      for (const p of lastByKind.values()) cb(p);
      return () => subs.delete(cb);
    },
  };
  const push = (p: { kind: string } & Record<string, unknown>) => {
    lastByKind.set(p.kind, p);
    for (const cb of subs) cb(p);
  };
  return { bridge, sent, push };
}

function memStorage() {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => {
      m.set(k, v);
    },
    removeItem: (k: string) => {
      m.delete(k);
    },
    dump: () => m,
  };
}

describe("identity conversion", () => {
  test("setIdentityPayload matches the generated SetIdentityPayload contract", () => {
    const stored = toStoredIdentity(makeIdentity());
    const payload = setIdentityPayload(stored);
    expect(payload).not.toBeNull();
    const typed: SetIdentityPayload = payload!;
    expect(Object.keys(typed).sort()).toEqual([
      "ephemeralPrivateKey",
      "message",
      "signature",
      "signer",
    ]);
    expect(typed.signer).toBe(SIGNER.toLowerCase());
    expect(typed.ephemeralPrivateKey).toBe(EPHEMERAL_KEY);
    expect(typed.message).toContain("Ephemeral address:");
    expect(typed.signature).toBe("0xsigsig");
    expect(identitySigner(stored)).toBe(SIGNER.toLowerCase());
    expect(isExpired(stored)).toBe(false);
    expect(isExpired(toStoredIdentity(makeIdentity(Date.now() - 1000)))).toBe(true);
  });

  test("loginIdentityCommand emits the engine's /login_identity contract", () => {
    const stored = toStoredIdentity(makeIdentity());
    const cmd = loginIdentityCommand(stored);
    expect(cmd.startsWith("/login_identity ")).toBe(true);
    const b64 = cmd.slice("/login_identity ".length);
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    const decoded = JSON.parse(
      new TextDecoder().decode(bytes),
    ) as StoredAuthIdentity;
    expect(decoded.ephemeralIdentity.privateKey).toBe(EPHEMERAL_KEY);
    const signerLink = decoded.authChain.find((l) => l.type === "SIGNER");
    expect(signerLink?.payload).toBe(SIGNER);
    const delegate = decoded.authChain.filter((l) => l.type !== "SIGNER");
    expect(delegate.length).toBeGreaterThan(0);
    expect(delegate[0]?.type).toBe("ECDSA_EPHEMERAL");
    expect(delegate[0]?.signature).toBe("0xsigsig");
  });
});

describe("lobby skip decision (shouldAutoJumpIn)", () => {
  const raw = (expirationMs: number) =>
    JSON.stringify(toStoredIdentity(makeIdentity(expirationMs)));

  test("skips the lobby only with more than 24h of validity left (Unity-parity margin)", () => {
    const now = Date.now();
    expect(shouldAutoJumpIn(raw(now + 48 * 3_600_000))).toBe(true);
    expect(shouldAutoJumpIn(raw(now - 1000))).toBe(false);
    expect(shouldAutoJumpIn(raw(now + 3_600_000), now)).toBe(false);
    expect(
      shouldAutoJumpIn(raw(now + AUTO_JUMP_IN_MIN_VALIDITY_MS + 60_000), now),
    ).toBe(true);
  });

  test("keeps the lobby for nothing stored, malformed JSON, or an identity the engine cannot consume", () => {
    expect(shouldAutoJumpIn(null)).toBe(false);
    expect(shouldAutoJumpIn("not json{")).toBe(false);
    const stored = toStoredIdentity(makeIdentity(Date.now() + 48 * 3_600_000));
    stored.authChain = stored.authChain.filter((l) => l.type === "SIGNER");
    expect(shouldAutoJumpIn(JSON.stringify(stored))).toBe(false);
  });
});

describe("delivery timing", () => {
  test("restores a returning wallet in the lobby without a guest identity or scene", () => {
    const { bridge, sent, push } = makeFakeBridge();
    const storage = memStorage();
    const stored = toStoredIdentity(makeIdentity());
    storage.setItem(IDENTITY_STORAGE_KEY, JSON.stringify(stored));
    const auth = createEngineAuth({ bridge: () => bridge, send: bridge.send, storage: () => storage });
    auth.init();
    push({ kind: "lifecycle", snapshot: { realm: { phase: "connecting" } } });
    expect(sent).toEqual([]);
    push({ kind: "lifecycle", snapshot: { realm: { phase: "active" }, readiness: { scene: null } } });
    expect(sent).toEqual([{ action: "SetIdentity", payload: setIdentityPayload(stored) }]);
    expect(auth.getState().status).toBe("pending");
    push({ kind: "identity", isGuest: false, signerAddress: SIGNER });
    expect(auth.getState()).toEqual({ status: "signedIn", address: SIGNER.toLowerCase() });
    push({ kind: "lifecycle", snapshot: { realm: { phase: "active" } } });
    expect(sent).toHaveLength(1);
    auth.dispose();
  });

  test("uses a replayed lobby connection for a new wallet sign-in", () => {
    const { bridge, sent, push } = makeFakeBridge();
    push({ kind: "lifecycle", snapshot: { realm: { phase: "active" } } });
    const auth = createEngineAuth({ bridge: () => bridge, send: bridge.send, storage: () => memStorage() });
    auth.init();
    expect(sent).toEqual([]);
    auth.loginWithIdentity(makeIdentity());
    expect(sent).toHaveLength(1);
    expect(sent[0]?.action).toBe("SetIdentity");
    auth.dispose();
  });

  test("lobby sign-in: nothing is sent until the engine's first identity push, subscribers see each state", () => {
    const { bridge, sent, push } = makeFakeBridge();
    const storage = memStorage();
    const auth = createEngineAuth({
      bridge: () => bridge,
      send: (a: string, p?: unknown) => bridge.send(a, p),
      storage: () => storage,
    });
    const seen: string[] = [];
    const unsub = auth.subscribe((s) => seen.push(`${s.status}:${s.address ?? "-"}`));

    const identity = makeIdentity();
    expect(auth.loginWithIdentity(identity)).toBe(true);
    expect(sent).toEqual([]);
    expect(auth.getState()).toEqual({
      status: "pending",
      address: SIGNER.toLowerCase(),
    });

    push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    expect(sent.length).toBe(1);
    expect(sent[0]?.action).toBe("SetIdentity");
    expect(sent[0]?.payload).toEqual(setIdentityPayload(toStoredIdentity(identity)));

    push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    expect(sent.length).toBe(1);

    push({ kind: "identity", isGuest: false, signerAddress: SIGNER });
    expect(auth.getState()).toEqual({
      status: "signedIn",
      address: SIGNER.toLowerCase(),
    });
    expect(storage.getItem(IDENTITY_STORAGE_KEY)).not.toBeNull();
    expect(seen).toEqual([
      `pending:${SIGNER.toLowerCase()}`,
      `signedIn:${SIGNER.toLowerCase()}`,
    ]);
    unsub();
    auth.dispose();
  });

  test("sends immediately when the engine already pushed identity, and waits for a bridge that appears later", async () => {
    const early = makeFakeBridge();
    early.push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    const inWorld = createEngineAuth({
      bridge: () => early.bridge,
      send: (a: string, p?: unknown) => early.bridge.send(a, p),
      storage: () => memStorage(),
    });
    inWorld.init();
    expect(early.sent).toEqual([]);
    inWorld.loginWithIdentity(makeIdentity());
    expect(early.sent.length).toBe(1);
    expect(early.sent[0]?.action).toBe("SetIdentity");
    inWorld.dispose();

    const late = makeFakeBridge();
    let available = false;
    const auth = createEngineAuth({
      bridge: () => (available ? late.bridge : null),
      send: (a: string, p?: unknown) => late.bridge.send(a, p),
      storage: () => memStorage(),
      attachIntervalMs: 1,
    });
    auth.loginWithIdentity(makeIdentity());
    expect(late.sent).toEqual([]);
    available = true;
    await new Promise((r) => setTimeout(r, 20));
    late.push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    expect(late.sent.length).toBe(1);
    auth.dispose();
  });

  test("retries on later guest pushes, capped", () => {
    const { bridge, sent, push } = makeFakeBridge();
    let t = 1_000_000;
    const auth = createEngineAuth({
      bridge: () => bridge,
      send: (a: string, p?: unknown) => bridge.send(a, p),
      storage: () => memStorage(),
      now: () => t,
    });
    auth.loginWithIdentity(makeIdentity(t + 86_400_000));
    for (let i = 0; i < 6; i++) {
      push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
      t += 10_000;
    }
    expect(sent.length).toBe(3);
    auth.dispose();
  });
});

describe("persistence", () => {
  test("expired identities are rejected and purged", () => {
    const storage = memStorage();
    const { bridge, sent, push } = makeFakeBridge();
    const auth = createEngineAuth({
      bridge: () => bridge,
      send: (a: string, p?: unknown) => bridge.send(a, p),
      storage: () => storage,
    });
    expect(auth.loginWithIdentity(makeIdentity(Date.now() - 1))).toBe(false);
    expect(auth.getState().status).toBe("none");

    storage.setItem(
      IDENTITY_STORAGE_KEY,
      JSON.stringify(toStoredIdentity(makeIdentity(Date.now() - 1))),
    );
    auth.init();
    expect(storage.getItem(IDENTITY_STORAGE_KEY)).toBeNull();
    push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    expect(sent).toEqual([]);
    auth.dispose();
  });

  test("a returning visitor is re-delivered at boot, and signOut clears identity and state", () => {
    const storage = memStorage();
    const first = makeFakeBridge();
    const session1 = createEngineAuth({
      bridge: () => first.bridge,
      send: (a: string, p?: unknown) => first.bridge.send(a, p),
      storage: () => storage,
    });
    session1.loginWithIdentity(makeIdentity());
    expect(storage.getItem(IDENTITY_STORAGE_KEY)).not.toBeNull();
    session1.dispose();

    const second = makeFakeBridge();
    const session2 = createEngineAuth({
      bridge: () => second.bridge,
      send: (a: string, p?: unknown) => second.bridge.send(a, p),
      storage: () => storage,
    });
    session2.init();
    expect(session2.getState().status).toBe("pending");
    second.push({ kind: "identity", isGuest: true, signerAddress: "0xguest" });
    expect(second.sent.length).toBe(1);
    expect(second.sent[0]?.action).toBe("SetIdentity");

    session2.signOut();
    expect(storage.getItem(IDENTITY_STORAGE_KEY)).toBeNull();
    expect(session2.getState()).toEqual({ status: "none", address: null });
    session2.dispose();
  });
});
