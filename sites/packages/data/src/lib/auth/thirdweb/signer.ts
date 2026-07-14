import { LOGIN_CHAIN_ID } from "./config";
import {
  ThirdwebError,
  type Eip712TypedData,
  type ThirdwebAuthResult,
} from "./api";

export type InAppSigner = {
  address: string;
  token: string;
  personalSign: (message: string) => Promise<string>;
  signTypedData: (typedData: Eip712TypedData, chainId: number) => Promise<string>;
};

const SIGN_PROXY = "/internal/thirdweb-sign";

async function proxySign(body: Record<string, unknown>): Promise<string> {
  let res: Response;
  try {
    res = await fetch(SIGN_PROXY, {
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
  let parsed: { signature?: string; error?: string } = {};
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
    }
  }
  if (!res.ok || !parsed.signature) {
    throw new ThirdwebError(
      parsed.error ?? `sign proxy returned ${res.status}`,
      res.status,
    );
  }
  return parsed.signature;
}

export function proxySignTypedData(
  token: string,
  from: string,
  typedData: Eip712TypedData,
  chainId: number,
): Promise<string> {
  return proxySign({ kind: "typedData", token, from, typedData, chainId });
}

export function makeInAppSigner(
  auth: Pick<ThirdwebAuthResult, "token" | "walletAddress">,
): InAppSigner {
  const address = auth.walletAddress.toLowerCase();
  const token = auth.token;
  return {
    address,
    token,
    personalSign: (message) =>
      proxySign({ kind: "message", token, from: address, message, chainId: LOGIN_CHAIN_ID }),
    signTypedData: (typedData, chainId) =>
      proxySign({ kind: "typedData", token, from: address, typedData, chainId }),
  };
}
