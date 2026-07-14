import { useEffect, useState, type ReactNode } from "react";

import { createIdentityFromPrivateKey } from "@data/lib/auth/identity";
import { SESSION_STORAGE_KEY, setIdentity } from "@data/lib/auth/session";

export const STORY_SIGNER = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";
const STORY_SIGNER_PK =
  "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d" as const;

const WALLET_COOKIE = "dcl_wallet";

function readPersisted(): string | null {
  try {
    return window.localStorage.getItem(SESSION_STORAGE_KEY);
  } catch {
    return null;
  }
}

function writePersisted(prev: string | null): void {
  try {
    if (prev === null) window.localStorage.removeItem(SESSION_STORAGE_KEY);
    else window.localStorage.setItem(SESSION_STORAGE_KEY, prev);
  } catch {
  }
}

function readWalletCookie(): string | null {
  const m = document.cookie.match(/(?:^|;\s*)dcl_wallet=([^;]*)/);
  return m ? m[1] : null;
}

function writeWalletCookie(prev: string | null): void {
  const secure = location.protocol === "https:" ? "; Secure" : "";
  document.cookie =
    prev === null
      ? `${WALLET_COOKIE}=; Path=/; Max-Age=0${secure}`
      : `${WALLET_COOKIE}=${prev}; Path=/; SameSite=Lax; Max-Age=604800${secure}`;
}

export function SignedInScope({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let alive = true;
    const prevStorage = readPersisted();
    const prevCookie = readWalletCookie();
    void createIdentityFromPrivateKey(STORY_SIGNER_PK).then((id) => {
      if (!alive) return;
      setIdentity(id);
      writePersisted(prevStorage);
      writeWalletCookie(prevCookie);
      setReady(true);
    });
    return () => {
      alive = false;
      setIdentity(null);
      writePersisted(prevStorage);
      writeWalletCookie(prevCookie);
    };
  }, []);
  return ready ? <>{children}</> : null;
}
