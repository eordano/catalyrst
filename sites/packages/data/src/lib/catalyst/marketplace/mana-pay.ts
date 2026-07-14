import { catalystBase } from "../client";

const TRANSFER_SELECTOR = "a9059cbb";
const EXECUTE_META_TX_SELECTOR = "0c53c51c";

export const MANA_POLYGON = {
  address: "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
  domainName: "(PoS) Decentraland MANA",
  domainVersion: "1",
  chainId: 137,
} as const;

function strip0x(s: string): string {
  return s.startsWith("0x") ? s.slice(2) : s;
}

function to32Bytes(hexish: string): string {
  return strip0x(hexish).toLowerCase().padStart(64, "0");
}

function decimalToHex(dec: string): string {
  return BigInt(dec).toString(16);
}

export function chainIdSalt(chainId: number): string {
  return `0x${to32Bytes(chainId.toString(16))}`;
}

export function transferCalldata(to: string, weiDecimal: string): string {
  return `0x${TRANSFER_SELECTOR}${to32Bytes(to)}${to32Bytes(
    decimalToHex(weiDecimal),
  )}`;
}

export type MetaTxTypedData = {
  types: Record<string, Array<{ name: string; type: string }>>;
  domain: {
    name: string;
    version: string;
    verifyingContract: string;
    salt: string;
  };
  primaryType: string;
  message: Record<string, unknown>;
};

export function manaMetaTxTypedData(
  from: string,
  nonceDecimal: string,
  functionSignature: string,
): MetaTxTypedData {
  return {
    types: {
      EIP712Domain: [
        { name: "name", type: "string" },
        { name: "version", type: "string" },
        { name: "verifyingContract", type: "address" },
        { name: "salt", type: "bytes32" },
      ],
      MetaTransaction: [
        { name: "nonce", type: "uint256" },
        { name: "from", type: "address" },
        { name: "functionSignature", type: "bytes" },
      ],
    },
    domain: {
      name: MANA_POLYGON.domainName,
      version: MANA_POLYGON.domainVersion,
      verifyingContract: MANA_POLYGON.address,
      salt: chainIdSalt(MANA_POLYGON.chainId),
    },
    primaryType: "MetaTransaction",
    message: {
      nonce: nonceDecimal,
      from,
      functionSignature,
    },
  };
}

export function normalizeV(vHex: string): string {
  let v = parseInt(vHex, 16);
  if (v < 27) v += 27;
  if (v !== 27 && v !== 28) {
    throw new Error(`Invalid signature version "${vHex}"`);
  }
  return v.toString(16);
}

export function executeMetaTxCalldata(
  account: string,
  fullSignature: string,
  functionSignature: string,
): string {
  const signature = strip0x(fullSignature);
  const r = signature.substring(0, 64);
  const s = signature.substring(64, 128);
  const v = normalizeV(signature.substring(128, 130));

  const method = strip0x(functionSignature);
  const signatureLength = (method.length / 2).toString(16);
  const signaturePadding = Math.ceil(method.length / 64);

  return [
    "0x",
    EXECUTE_META_TX_SELECTOR,
    to32Bytes(account),
    to32Bytes("a0"),
    r,
    s,
    to32Bytes(v),
    to32Bytes(signatureLength),
    method.padEnd(64 * signaturePadding, "0"),
  ].join("");
}

type RelayResponse =
  | { ok: true; txHash: string }
  | { ok: false; error?: string; message?: string };

export async function relayMetaTx(
  from: string,
  txData: string,
  opts: { base?: string; signal?: AbortSignal } = {},
): Promise<string> {
  const base = catalystBase(opts.base);
  const res = await fetch(`${base}/v1/transactions`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      transactionData: {
        from,
        params: [MANA_POLYGON.address, txData],
      },
    }),
    signal: opts.signal,
  });
  const body = (await res.json().catch(() => null)) as RelayResponse | null;
  if (!res.ok || !body || body.ok !== true) {
    const message =
      body && "message" in body && body.message
        ? body.message
        : `relay returned ${res.status}`;
    throw new Error(`MANA payment could not be relayed: ${message}`);
  }
  return body.txHash;
}

const POLYGON_RPCS = [
  "https://polygon-bor-rpc.publicnode.com",
  "https://polygon.llamarpc.com",
];

export async function fetchManaBalance(
  address: string,
  signal?: AbortSignal,
): Promise<bigint | null> {
  const data = `0x70a08231${to32Bytes(address)}`;
  for (const rpc of POLYGON_RPCS) {
    try {
      const res = await fetch(rpc, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "eth_call",
          params: [{ to: MANA_POLYGON.address, data }, "latest"],
        }),
        signal,
      });
      const j = (await res.json()) as { result?: string };
      if (typeof j.result === "string" && j.result.startsWith("0x")) {
        return BigInt(j.result);
      }
    } catch {
    }
  }
  return null;
}
