import { parseCookies } from "@core/lib/experiments/assign";

const WALLET_COOKIE = "dcl_wallet";

function secureSuffix(): string {
  return typeof location !== "undefined" && location.protocol === "https:"
    ? "; Secure"
    : "";
}

export function serializeWalletCookie(address: string, expirationIso: string): string {
  const maxAge = Math.max(0, Math.floor((Date.parse(expirationIso) - Date.now()) / 1000));
  return (
    [
      `${WALLET_COOKIE}=${encodeURIComponent(address)}`,
      "Path=/",
      "SameSite=Lax",
      `Max-Age=${maxAge}`,
    ].join("; ") + secureSuffix()
  );
}

export function clearWalletCookie(): string {
  return `${WALLET_COOKIE}=; Path=/; Max-Age=0` + secureSuffix();
}

export function readWallet(request: Request): string | null {
  const w = parseCookies(request.headers.get("cookie"))[WALLET_COOKIE];
  return w && /^0x[0-9a-f]{40}$/.test(w) ? w : null;
}
