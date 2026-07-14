import {
  AUTH_CHAIN_HEADER_PREFIX,
  AUTH_METADATA_HEADER,
  AUTH_TIMESTAMP_HEADER,
  buildRequestPayload,
  signAuthChain,
} from "@ui/data/auth/signerCore";

import type { AuthIdentity, AuthLink, SignedFetchMetadata } from "./types";
import { isIdentityExpired } from "./expiry";

export {
  AUTH_CHAIN_HEADER_PREFIX,
  AUTH_METADATA_HEADER,
  AUTH_TIMESTAMP_HEADER,
  buildRequestPayload,
};

export class AuthExpiredError extends Error {
  constructor() {
    super("Auth identity has expired. Reconnect your wallet to continue.");
    this.name = "AuthExpiredError";
  }
}

export function signedPath(input: string): string {
  if (input.startsWith("/")) return input.split("?")[0];
  try {
    return new URL(input).pathname;
  } catch {
    return `/${input}`.replace(/\/+/g, "/").split("?")[0];
  }
}

export type SignedRequest = {
  headers: Record<string, string>;
  authChain: AuthLink[];
  timestamp: string;
  metadata: SignedFetchMetadata;
  payload: string;
};

export async function signRequest(
  identity: AuthIdentity,
  method: string,
  url: string,
  metadata: SignedFetchMetadata = {},
): Promise<SignedRequest> {
  if (isIdentityExpired(identity)) throw new AuthExpiredError();

  const path = signedPath(url);
  const timestamp = Date.now().toString();
  const metadataStr = JSON.stringify(metadata);

  const { headers, authChain, payload } = await signAuthChain(
    identity.ephemeral.privateKey,
    identity.authChain,
    method,
    path,
    timestamp,
    metadataStr,
  );

  return { headers, authChain, timestamp, metadata, payload };
}

export async function signedFetch(
  identity: AuthIdentity,
  input: string,
  init: RequestInit & { metadata?: SignedFetchMetadata; signPath?: string } = {},
): Promise<Response> {
  const { metadata, headers: initHeaders, signPath, ...rest } = init;
  const method = (init.method ?? "GET").toUpperCase();
  const signed = await signRequest(identity, method, signPath ?? input, metadata ?? {});
  const headers = new Headers(initHeaders);
  for (const [k, v] of Object.entries(signed.headers)) headers.set(k, v);
  return fetch(input, { ...rest, method, headers });
}
