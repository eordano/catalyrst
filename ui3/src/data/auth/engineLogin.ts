
import type { SetIdentityPayload } from "../../generated/bridge/SetIdentityPayload";
import { getBridge, sendBridge } from "../../overlay/bridge";
import type { AuthIdentity, AuthLink } from "./identity";

export const IDENTITY_STORAGE_KEY = "dcl-auth-identity";

export type StoredAuthIdentity = {
  ephemeralIdentity: { address: string; privateKey: string; publicKey?: string };
  expiration: string;
  authChain: AuthLink[];
};

export type EngineAuthStatus = "none" | "pending" | "signedIn";
export type EngineAuthState = { status: EngineAuthStatus; address: string | null };

export function toStoredIdentity(identity: AuthIdentity): StoredAuthIdentity {
  return {
    ephemeralIdentity: {
      address: identity.ephemeral.address,
      privateKey: identity.ephemeral.privateKey,
    },
    expiration: identity.expiration,
    authChain: identity.authChain,
  };
}

function isStoredIdentity(v: unknown): v is StoredAuthIdentity {
  const s = v as StoredAuthIdentity;
  return (
    typeof s === "object" &&
    s !== null &&
    typeof s.ephemeralIdentity === "object" &&
    s.ephemeralIdentity !== null &&
    typeof s.ephemeralIdentity.privateKey === "string" &&
    s.ephemeralIdentity.privateKey.length > 0 &&
    typeof s.expiration === "string" &&
    Array.isArray(s.authChain)
  );
}

export function isExpired(stored: StoredAuthIdentity, now = Date.now()): boolean {
  const exp = Date.parse(stored.expiration);
  return !Number.isFinite(exp) || exp <= now;
}

export function parseStoredIdentity(
  raw: string | null,
  now = Date.now(),
): StoredAuthIdentity | null {
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    return isStoredIdentity(parsed) && !isExpired(parsed, now) ? parsed : null;
  } catch {
    return null;
  }
}

export const AUTO_JUMP_IN_MIN_VALIDITY_MS = 24 * 60 * 60 * 1000;

export function shouldAutoJumpIn(raw: string | null, now = Date.now()): boolean {
  const stored = parseStoredIdentity(raw, now);
  if (!stored) return false;
  return (
    Date.parse(stored.expiration) - now > AUTO_JUMP_IN_MIN_VALIDITY_MS &&
    setIdentityPayload(stored) !== null
  );
}

export function identitySigner(stored: StoredAuthIdentity): string | null {
  const link = stored.authChain.find((l) => l.type === "SIGNER");
  const payload = link?.payload.trim();
  return payload ? payload.toLowerCase() : null;
}

export function setIdentityPayload(
  stored: StoredAuthIdentity,
): SetIdentityPayload | null {
  const signer = identitySigner(stored);
  const ephemeralLink = stored.authChain.find((l) => l.type === "ECDSA_EPHEMERAL");
  if (!signer || !ephemeralLink?.payload || !ephemeralLink.signature) return null;
  return {
    signer,
    ephemeralPrivateKey: stored.ephemeralIdentity.privateKey,
    message: ephemeralLink.payload,
    signature: ephemeralLink.signature,
  };
}

export function loginIdentityCommand(stored: StoredAuthIdentity): string {
  const bytes = new TextEncoder().encode(JSON.stringify(stored));
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return `/login_identity ${btoa(bin)}`;
}

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

type IdentityPush = {
  kind?: string;
  isGuest?: boolean | null;
  signerAddress?: string | null;
};

export type EngineAuthDeps = {
  bridge?: () => ReturnType<typeof getBridge>;
  send?: typeof sendBridge;
  storage?: () => StorageLike | null;
  now?: () => number;
  attachIntervalMs?: number;
};

const MAX_SEND_ATTEMPTS = 3;
const RESEND_INTERVAL_MS = 5000;

export function createEngineAuth(deps: EngineAuthDeps = {}) {
  const bridge = deps.bridge ?? getBridge;
  const send = deps.send ?? sendBridge;
  const now = deps.now ?? Date.now;
  const attachIntervalMs = deps.attachIntervalMs ?? 250;
  const storage =
    deps.storage ??
    (() => {
      try {
        return typeof localStorage !== "undefined" ? localStorage : null;
      } catch {
        return null;
      }
    });

  let pending: StoredAuthIdentity | null = null;
  let signedInAddress: string | null = null;
  let engineIdentitySeen = false;
  let attempts = 0;
  let lastSendAt = 0;
  let watcherArmed = false;
  let attachTimer: ReturnType<typeof setTimeout> | null = null;
  const listeners = new Set<(s: EngineAuthState) => void>();

  function getState(): EngineAuthState {
    if (signedInAddress) return { status: "signedIn", address: signedInAddress };
    if (pending) return { status: "pending", address: identitySigner(pending) };
    return { status: "none", address: null };
  }

  function notify() {
    const s = getState();
    for (const cb of listeners) {
      try {
        cb(s);
      } catch {
      }
    }
  }

  function persist(stored: StoredAuthIdentity) {
    try {
      storage()?.setItem(IDENTITY_STORAGE_KEY, JSON.stringify(stored));
    } catch {
    }
  }

  function loadPersisted(): StoredAuthIdentity | null {
    try {
      const raw = storage()?.getItem(IDENTITY_STORAGE_KEY) ?? null;
      const stored = parseStoredIdentity(raw, now());
      if (raw && !stored) storage()?.removeItem(IDENTITY_STORAGE_KEY);
      return stored;
    } catch {
      return null;
    }
  }

  function deliver() {
    if (!pending) return;
    if (isExpired(pending, now())) {
      pending = null;
      notify();
      return;
    }
    const payload = setIdentityPayload(pending);
    if (!payload) {
      pending = null;
      notify();
      return;
    }
    if (attempts >= MAX_SEND_ATTEMPTS) return;
    if (now() - lastSendAt < RESEND_INTERVAL_MS) return;
    attempts += 1;
    lastSendAt = now();
    send("SetIdentity", payload);
  }

  function onPush(push: unknown) {
    const p = push as IdentityPush;
    if (!p || p.kind !== "identity") return;
    engineIdentitySeen = true;
    if (p.isGuest === false && p.signerAddress) {
      const addr = p.signerAddress.toLowerCase();
      if (!pending || identitySigner(pending) === addr) {
        pending = null;
        if (signedInAddress !== addr) {
          signedInAddress = addr;
          notify();
        }
      }
      return;
    }
    if (signedInAddress) {
      signedInAddress = null;
      notify();
    }
    deliver();
  }

  function armWatcher() {
    if (watcherArmed) return;
    watcherArmed = true;
    const attach = () => {
      const b = bridge();
      if (!b) {
        attachTimer = setTimeout(attach, attachIntervalMs);
        return;
      }
      attachTimer = null;
      try {
        b.onState(onPush);
      } catch {
        watcherArmed = false;
      }
    };
    attach();
  }

  function loginWithIdentity(identity: AuthIdentity | StoredAuthIdentity): boolean {
    const stored =
      "ephemeralIdentity" in identity ? identity : toStoredIdentity(identity);
    if (!isStoredIdentity(stored) || isExpired(stored, now())) return false;
    if (!setIdentityPayload(stored)) return false;
    pending = stored;
    signedInAddress = null;
    attempts = 0;
    lastSendAt = 0;
    persist(stored);
    notify();
    armWatcher();
    if (engineIdentitySeen) deliver();
    return true;
  }

  function init(): void {
    if (pending || signedInAddress) return;
    const stored = loadPersisted();
    if (stored) {
      pending = stored;
      attempts = 0;
      lastSendAt = 0;
      notify();
    }
    armWatcher();
    if (pending && engineIdentitySeen) deliver();
  }

  function signOut(): void {
    pending = null;
    signedInAddress = null;
    attempts = 0;
    try {
      storage()?.removeItem(IDENTITY_STORAGE_KEY);
    } catch {
    }
    notify();
  }

  function subscribe(cb: (s: EngineAuthState) => void): () => void {
    listeners.add(cb);
    return () => listeners.delete(cb);
  }

  function dispose() {
    if (attachTimer) clearTimeout(attachTimer);
    listeners.clear();
  }

  return {
    loginWithIdentity,
    init,
    signOut,
    subscribe,
    getState,
    dispose,
  };
}

const singleton = createEngineAuth();

export const loginWithIdentity = singleton.loginWithIdentity;
export const initEngineAuth = singleton.init;
export const signOutEngineAuth = singleton.signOut;
export const subscribeEngineAuth = singleton.subscribe;
export const getEngineAuthState = singleton.getState;
