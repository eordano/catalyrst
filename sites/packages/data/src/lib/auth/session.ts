import { isIdentityExpired } from "./expiry";
import type { AuthIdentity } from "./types";
import { clearWalletCookie, serializeWalletCookie } from "./wallet-cookie";

function syncWalletCookie(identity: AuthIdentity | null): void {
  if (typeof document === "undefined") return;
  document.cookie = identity
    ? serializeWalletCookie(identity.signer, identity.expiration)
    : clearWalletCookie();
}

export const SESSION_STORAGE_KEY = "dcl:auth:identity:v1";

function readStorage(): AuthIdentity | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(SESSION_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AuthIdentity;
    if (
      !parsed?.signer ||
      !parsed?.ephemeral?.privateKey ||
      !Array.isArray(parsed.authChain)
    ) {
      return null;
    }
    if (isIdentityExpired(parsed)) {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function writeStorage(identity: AuthIdentity | null): void {
  if (typeof window === "undefined") return;
  try {
    if (identity) {
      window.localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(identity));
    } else {
      window.localStorage.removeItem(SESSION_STORAGE_KEY);
    }
  } catch {
  }
  syncWalletCookie(identity);
}


let current: AuthIdentity | null = null;
let hydrated = false;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function ensureHydrated() {
  if (hydrated || typeof window === "undefined") return;
  current = readStorage();
  hydrated = true;
}

export function getIdentity(): AuthIdentity | null {
  ensureHydrated();
  return current;
}

export function setIdentity(identity: AuthIdentity | null): void {
  current = identity;
  hydrated = true;
  writeStorage(identity);
  emit();
}

export function clearIdentity(): void {
  setIdentity(null);
}

export function subscribe(listener: () => void): () => void {
  ensureHydrated();
  listeners.add(listener);
  const onStorage = (e: StorageEvent) => {
    if (e.key === SESSION_STORAGE_KEY) {
      current = readStorage();
      syncWalletCookie(current);
      emit();
    }
  };
  if (typeof window !== "undefined" && listeners.size === 1) {
    window.addEventListener("storage", onStorage);
  }
  return () => {
    listeners.delete(listener);
    if (typeof window !== "undefined" && listeners.size === 0) {
      window.removeEventListener("storage", onStorage);
    }
  };
}

export function getServerSnapshot(): AuthIdentity | null {
  return null;
}
