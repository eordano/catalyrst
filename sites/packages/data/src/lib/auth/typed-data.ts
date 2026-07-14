import {
  getThirdwebSession,
  proxySignTypedData,
  type Eip712TypedData,
} from "./thirdweb";
import { signTypedData as injectedSignTypedData } from "./wallet";

type TypedDataInput = {
  domain: Record<string, unknown> & { chainId?: string | number; salt?: string };
  types: Record<string, Array<{ name: string; type: string }>>;
  primaryType: string;
  message: Record<string, unknown>;
};

const DEFAULT_CHAIN_ID = 137;

function resolveChainId(typedData: TypedDataInput): number {
  const { chainId, salt } = typedData.domain;
  if (typeof chainId === "number" && chainId > 0) return chainId;
  if (typeof chainId === "string" && chainId) {
    const n = Number(chainId);
    if (Number.isFinite(n) && n > 0) return n;
  }
  if (typeof salt === "string" && salt.startsWith("0x")) {
    try {
      const n = Number(BigInt(salt));
      if (Number.isFinite(n) && n > 0) return n;
    } catch {
    }
  }
  return DEFAULT_CHAIN_ID;
}

export async function signTypedData(
  typedData: TypedDataInput,
  from: string,
): Promise<string> {
  const tw = getThirdwebSession();
  if (tw && tw.address.toLowerCase() === from.toLowerCase()) {
    const types: Record<string, Array<{ name: string; type: string }>> = {
      ...typedData.types,
    };
    delete types.EIP712Domain;
    const enclaveTyped: Eip712TypedData = {
      domain: typedData.domain,
      types,
      primaryType: typedData.primaryType,
      message: typedData.message,
    };
    return proxySignTypedData(
      tw.token,
      from,
      enclaveTyped,
      resolveChainId(typedData),
    );
  }
  try {
    return await injectedSignTypedData(typedData, from);
  } catch (err) {
    const dev = await import("./dev-identity")
      .then((m) => m.devSignTypedData(typedData, from))
      .catch(() => null);
    if (dev) return dev;
    throw err;
  }
}
