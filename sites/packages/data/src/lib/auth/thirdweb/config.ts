export const THIRDWEB_API_BASE = "https://api.thirdweb.com";

export const LOGIN_CHAIN_ID = 1;

declare global {
  interface Window {
    __DCL_PUBLIC__?: {
      thirdwebClientId?: string;
      thirdwebSignProxy?: string;
    };
  }
}

export function thirdwebClientId(): string {
  if (typeof process !== "undefined" && process.env?.THIRDWEB_CLIENT_ID) {
    return process.env.THIRDWEB_CLIENT_ID;
  }
  if (typeof window !== "undefined") {
    return window.__DCL_PUBLIC__?.thirdwebClientId ?? "";
  }
  return "";
}

export function hasThirdwebClientId(): boolean {
  return thirdwebClientId().length > 0;
}

export function thirdwebSecretKey(): string {
  if (typeof process !== "undefined" && process.env?.THIRDWEB_SECRET_KEY) {
    return process.env.THIRDWEB_SECRET_KEY;
  }
  return "";
}
