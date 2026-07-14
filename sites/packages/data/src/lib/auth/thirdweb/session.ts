export const THIRDWEB_SESSION_KEY = "dcl:auth:thirdweb:v1";

export type ThirdwebSession = {
  token: string;
  address: string;
};

export function getThirdwebSession(): ThirdwebSession | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(THIRDWEB_SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as ThirdwebSession;
    if (!parsed?.token || !parsed?.address) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function setThirdwebSession(session: ThirdwebSession | null): void {
  if (typeof window === "undefined") return;
  try {
    if (session) {
      window.localStorage.setItem(THIRDWEB_SESSION_KEY, JSON.stringify(session));
    } else {
      window.localStorage.removeItem(THIRDWEB_SESSION_KEY);
    }
  } catch {
  }
}

export function clearThirdwebSession(): void {
  setThirdwebSession(null);
}
