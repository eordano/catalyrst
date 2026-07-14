import { check } from "../../validate";

import { SignProxyOkSchema, ThirdwebAuthResultSchema } from "./thirdwebSchema";

export const THIRDWEB_API_BASE = "https://api.thirdweb.com";

export const LOGIN_CHAIN_ID = 1;


function viteEnv(name: string): string {
  try {
    const v = (import.meta.env as Record<string, unknown> | undefined)?.[name];
    return typeof v === "string" ? v : "";
  } catch {
    return "";
  }
}

export function thirdwebClientId(): string {
  if (typeof window !== "undefined") {
    const injected = window.__DCL_PUBLIC__?.thirdwebClientId;
    if (injected) return injected;
  }
  return viteEnv("VITE_DCL_THIRDWEB_CLIENT_ID");
}

export function thirdwebSignProxyUrl(): string {
  if (typeof window !== "undefined") {
    const injected = window.__DCL_PUBLIC__?.thirdwebSignProxy;
    if (injected) return injected;
  }
  return viteEnv("VITE_DCL_THIRDWEB_SIGN_PROXY") || "/internal/thirdweb-sign";
}

export function hasThirdwebClientId(): boolean {
  return thirdwebClientId().length > 0;
}


export class ThirdwebError extends Error {
  readonly status: number;
  readonly correlationId?: string;
  constructor(message: string, status: number, correlationId?: string) {
    super(message);
    this.name = "ThirdwebError";
    this.status = status;
    this.correlationId = correlationId;
  }
}

export type ThirdwebAuthResult = {
  isNewUser: boolean;
  token: string;
  userId: string;
  walletAddress: string;
  type: string;
};

export type ThirdwebSocialProvider =
  | "google"
  | "apple"
  | "discord"
  | "facebook"
  | "github"
  | "telegram"
  | "x";

export type Eip712TypedData = {
  domain: Record<string, unknown>;
  types: Record<string, Array<{ name: string; type: string }>>;
  primaryType: string;
  message: Record<string, unknown>;
};

type FetchOpts = {
  method: "GET" | "POST";
  token?: string;
  body?: unknown;
  signal?: AbortSignal;
};

/**
 * Returns the decoded body as `unknown`. Each caller validates its own shape:
 * a generic would have this function assert a type it never looked at, which is
 * the claim the schemas exist to stop making.
 */
async function twFetch(path: string, opts: FetchOpts): Promise<unknown> {
  const clientId = thirdwebClientId();
  if (!clientId) {
    throw new ThirdwebError(
      "Sign-in is temporarily unavailable (no thirdweb client id configured).",
      0,
    );
  }

  const headers: Record<string, string> = {
    accept: "application/json",
    "x-client-id": clientId,
  };
  if (opts.token) headers["authorization"] = `Bearer ${opts.token}`;
  if (opts.body !== undefined) headers["content-type"] = "application/json";

  let res: Response;
  try {
    res = await fetch(`${THIRDWEB_API_BASE}${path}`, {
      method: opts.method,
      headers,
      signal: opts.signal,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    });
  } catch (err) {
    throw new ThirdwebError(
      `thirdweb request failed: ${(err as Error)?.message ?? "network error"}`,
      0,
    );
  }

  const text = await res.text();
  let parsed: unknown = undefined;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
    }
  }

  // Read loosely and left unvalidated on purpose: an error body is a courtesy,
  // not a contract, and validating it would replace a real 401 with a complaint
  // about the shape of its explanation.
  if (!res.ok) {
    const obj = (parsed ?? {}) as { message?: string; correlationId?: string };
    throw new ThirdwebError(
      obj.message ?? `thirdweb returned ${res.status} ${res.statusText}`,
      res.status,
      obj.correlationId,
    );
  }

  return parsed;
}

/** Nothing to validate: this endpoint answers 200 with an empty body. */
export async function initiateEmailLogin(
  email: string,
  signal?: AbortSignal,
): Promise<void> {
  await twFetch("/v1/auth/initiate", {
    method: "POST",
    body: { method: "email", email },
    signal,
  });
}

export async function completeEmailLogin(
  email: string,
  code: string,
  signal?: AbortSignal,
): Promise<ThirdwebAuthResult> {
  const raw = await twFetch("/v1/auth/complete", {
    method: "POST",
    body: { method: "email", email, code },
    signal,
  });
  return check(ThirdwebAuthResultSchema, raw, "external-http/thirdweb/auth-complete");
}

export function socialLoginUrl(
  provider: ThirdwebSocialProvider,
  redirectUrl: string,
): string {
  const params = new URLSearchParams({
    provider,
    redirectUrl,
    clientId: thirdwebClientId(),
  });
  return `${THIRDWEB_API_BASE}/v1/auth/social?${params.toString()}`;
}

async function proxySign(body: Record<string, unknown>): Promise<string> {
  let res: Response;
  try {
    res = await fetch(thirdwebSignProxyUrl(), {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw new ThirdwebError(
      `sign request failed: ${(err as Error)?.message ?? "network error"}`,
      0,
    );
  }
  const text = await res.text();
  let parsed: unknown = undefined;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
    }
  }
  // The failure test runs first and stays loose. A 503 carries `{error}`, and
  // validating the success shape ahead of the status would answer "the server
  // is not configured for signing" with a complaint about a missing signature.
  const loose = (parsed ?? {}) as { signature?: unknown; error?: string };
  if (!res.ok || !loose.signature) {
    throw new ThirdwebError(
      loose.error ?? `sign proxy returned ${res.status}`,
      res.status,
    );
  }
  return check(SignProxyOkSchema, parsed, "external-http/thirdweb/sign-proxy").signature;
}

export type InAppSigner = {
  address: string;
  token: string;
  personalSign: (message: string) => Promise<string>;
  signTypedData: (typedData: Eip712TypedData, chainId: number) => Promise<string>;
};

export function makeInAppSigner(
  auth: Pick<ThirdwebAuthResult, "token" | "walletAddress">,
): InAppSigner {
  const address = auth.walletAddress.toLowerCase();
  const token = auth.token;
  return {
    address,
    token,
    personalSign: (message) =>
      proxySign({
        kind: "message",
        token,
        from: address,
        message,
        chainId: LOGIN_CHAIN_ID,
      }),
    signTypedData: (typedData, chainId) =>
      proxySign({ kind: "typedData", token, from: address, typedData, chainId }),
  };
}

/**
 * Reads the `?authResult=` parameter thirdweb's social redirect appends.
 *
 * Deliberately NOT a `check` boundary, and not an omission to be tidied up
 * later: this string arrives in a URL anyone can compose, so it is untrusted
 * input rather than a contract with a service. Throwing on a shape that does
 * not match -- which is the right answer for a service that changed under us --
 * would make a crafted link a dev-mode denial of service and a production
 * console flood. Returning null keeps a bad link indistinguishable from no
 * link, which is what the caller already does with it.
 */
export function parseAuthResult(
  raw: string,
): Pick<ThirdwebAuthResult, "token" | "walletAddress"> | null {
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof v !== "object" || v === null) return null;
  const o = v as {
    token?: unknown;
    walletAddress?: unknown;
    storedToken?: {
      jwtToken?: unknown;
      cookieString?: unknown;
      authDetails?: { walletAddress?: unknown };
    };
  };
  const token = [o.token, o.storedToken?.jwtToken, o.storedToken?.cookieString].find(
    (t): t is string => typeof t === "string" && t.length > 0,
  );
  const walletAddress = [
    o.walletAddress,
    o.storedToken?.authDetails?.walletAddress,
  ].find((a): a is string => typeof a === "string" && a.length > 0);
  if (!token || !walletAddress) return null;
  return { token, walletAddress };
}
