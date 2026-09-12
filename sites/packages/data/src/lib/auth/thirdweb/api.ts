import { check } from "@ui/validate";
import {
  EnclaveSignatureSchema,
  ThirdwebAuthResultSchema,
  WalletsMeSchema,
} from "@ui/data/auth/thirdwebSchema";

import { THIRDWEB_API_BASE, thirdwebClientId } from "./config";

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
  secretKey?: string;
};

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
  if (opts.secretKey) headers["x-secret-key"] = opts.secretKey;
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

export async function signMessageEnclave(
  token: string,
  from: string,
  message: string,
  chainId: number,
  signal?: AbortSignal,
  secretKey?: string,
): Promise<string> {
  const raw = await twFetch(
    "/v1/wallets/sign-message",
    { method: "POST", token, body: { from, chainId, message }, signal, secretKey },
  );
  const out = check(EnclaveSignatureSchema, raw, "external-http/thirdweb/enclave-sign");
  return out.result.signature;
}

export async function signTypedDataEnclave(
  token: string,
  from: string,
  typedData: Eip712TypedData,
  chainId: number,
  signal?: AbortSignal,
  secretKey?: string,
): Promise<string> {
  const domain =
    typedData.domain && typedData.domain.chainId != null
      ? { ...typedData.domain, chainId: String(typedData.domain.chainId) }
      : typedData.domain;
  const raw = await twFetch(
    "/v1/wallets/sign-typed-data",
    {
      method: "POST",
      token,
      body: {
        from,
        chainId,
        domain,
        types: typedData.types,
        primaryType: typedData.primaryType,
        message: typedData.message,
      },
      signal,
      secretKey,
    },
  );
  const out = check(EnclaveSignatureSchema, raw, "external-http/thirdweb/enclave-sign");
  return out.result.signature;
}

export async function getWalletForToken(
  token: string,
  signal?: AbortSignal,
): Promise<string | null> {
  let raw: unknown;
  try {
    raw = await twFetch("/v1/wallets/me", { method: "GET", token, signal });
  } catch {
    return null;
  }
  const out = check(WalletsMeSchema, raw, "external-http/thirdweb/wallets-me");
  const addr = out.result?.address ?? out.address ?? null;
  return typeof addr === "string" ? addr.toLowerCase() : null;
}
