import { privateKeyToAccount } from "viem/accounts";

import type { AuthLink } from "./identity";

export const AUTH_CHAIN_HEADER_PREFIX = "x-identity-auth-chain-";
export const AUTH_TIMESTAMP_HEADER = "x-identity-timestamp";
export const AUTH_METADATA_HEADER = "x-identity-metadata";

export function buildRequestPayload(
  method: string,
  path: string,
  timestamp: string,
  metadataStr: string,
): string {
  return `${method}:${path}:${timestamp}:${metadataStr}`.toLowerCase();
}

export type SignedAuthChain = {
  headers: Record<string, string>;
  authChain: AuthLink[];
  payload: string;
};

export async function signAuthChain(
  ephemeralPrivateKey: `0x${string}`,
  authChain: AuthLink[],
  method: string,
  path: string,
  timestamp: string,
  metadataStr: string,
): Promise<SignedAuthChain> {
  const payload = buildRequestPayload(method, path, timestamp, metadataStr);
  const account = privateKeyToAccount(ephemeralPrivateKey);
  const signature = await account.signMessage({ message: payload });
  const chain: AuthLink[] = [
    ...authChain,
    { type: "ECDSA_SIGNED_ENTITY", payload, signature },
  ];
  const headers: Record<string, string> = {
    [AUTH_TIMESTAMP_HEADER]: timestamp,
    [AUTH_METADATA_HEADER]: metadataStr,
  };
  chain.forEach((link, i) => {
    headers[`${AUTH_CHAIN_HEADER_PREFIX}${i}`] = JSON.stringify(link);
  });
  return { headers, authChain: chain, payload };
}
