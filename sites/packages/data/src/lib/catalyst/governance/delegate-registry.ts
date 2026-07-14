import { decodeFunctionResult, encodeFunctionData, isAddress, stringToHex } from "viem";
import { z } from "zod";

export const DELEGATE_REGISTRY_ABI = [
  {
    type: "function",
    name: "setDelegate",
    stateMutability: "nonpayable",
    inputs: [
      { name: "id", type: "bytes32" },
      { name: "delegate", type: "address" },
    ],
    outputs: [],
  },
  {
    type: "function",
    name: "delegation",
    stateMutability: "view",
    inputs: [
      { name: "delegator", type: "address" },
      { name: "id", type: "bytes32" },
    ],
    outputs: [{ name: "", type: "address" }],
  },
] as const;

export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";
export const GLOBAL_SPACE_ID = `0x${"0".repeat(64)}` as `0x${string}`;

export const REGISTRY_ADDRESS_ENV = "SNAPSHOT_DELEGATE_CONTRACT_ADDRESS";
export const REGISTRY_CHAIN_ENV = "SNAPSHOT_DELEGATE_CHAIN_ID";
export const REGISTRY_RPC_ENV = "SNAPSHOT_DELEGATE_RPC_URL";

export type DelegateRegistryConfig = {
  address: `0x${string}`;
  chainId: number;
};

export type DelegateRegistrySetup = {
  config: DelegateRegistryConfig | null;
  rpcUrl: string | null;
  blockers: string[];
};

export type DelegationScope = "space" | "global" | "none";

export type DelegationState = {
  delegate: string | null;
  scope: DelegationScope;
};

type Env = Record<string, string | undefined>;

function processEnv(): Env {
  return typeof process !== "undefined" && process.env ? process.env : {};
}

export function resolveDelegateRegistry(env: Env = processEnv()): DelegateRegistrySetup {
  const blockers: string[] = [];
  const rawAddress = env[REGISTRY_ADDRESS_ENV]?.trim() ?? "";
  const rawChainId = env[REGISTRY_CHAIN_ENV]?.trim() ?? "";
  const rpcUrl = env[REGISTRY_RPC_ENV]?.trim() ?? "";

  let config: DelegateRegistryConfig | null = null;
  if (!rawAddress || !rawChainId) {
    blockers.push(
      `delegation is not configured: set ${REGISTRY_ADDRESS_ENV} and ${REGISTRY_CHAIN_ENV}`,
    );
  } else if (!isAddress(rawAddress)) {
    blockers.push(`${REGISTRY_ADDRESS_ENV} is not a contract address`);
  } else {
    const chainId = Number(rawChainId);
    if (!Number.isInteger(chainId) || chainId <= 0) {
      blockers.push(`${REGISTRY_CHAIN_ENV} is not a chain id`);
    } else {
      config = { address: rawAddress.toLowerCase() as `0x${string}`, chainId };
    }
  }

  if (!rpcUrl) {
    blockers.push(
      `current delegation is unknown: set ${REGISTRY_RPC_ENV} to read the delegate registry`,
    );
  }

  return { config, rpcUrl: rpcUrl || null, blockers };
}

export function snapshotSpaceId(space: string): `0x${string}` {
  const trimmed = space.trim();
  if (!trimmed) throw new Error("snapshot space is empty");
  if (new TextEncoder().encode(trimmed).length > 32) {
    throw new Error(`snapshot space "${trimmed}" does not fit in bytes32`);
  }
  return stringToHex(trimmed, { size: 32 });
}

export function requireAddress(value: string, label: string): `0x${string}` {
  const trimmed = value.trim();
  if (!isAddress(trimmed)) throw new Error(`${label} is not an ethereum address: ${value}`);
  return trimmed.toLowerCase() as `0x${string}`;
}

export function encodeSetDelegate(space: string, delegate: string): `0x${string}` {
  return encodeFunctionData({
    abi: DELEGATE_REGISTRY_ABI,
    functionName: "setDelegate",
    args: [snapshotSpaceId(space), requireAddress(delegate, "delegate")],
  });
}

export function encodeDelegationQuery(
  delegator: string,
  id: `0x${string}`,
): `0x${string}` {
  return encodeFunctionData({
    abi: DELEGATE_REGISTRY_ABI,
    functionName: "delegation",
    args: [requireAddress(delegator, "delegator"), id],
  });
}

const RpcResponseSchema = z.object({
  result: z.string().optional(),
  error: z.object({ message: z.string().optional() }).nullish(),
});

export async function ethCall(args: {
  rpcUrl: string;
  to: string;
  data: `0x${string}`;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}): Promise<`0x${string}`> {
  const doFetch = args.fetchImpl ?? fetch;
  const res = await doFetch(args.rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [{ to: args.to, data: args.data }, "latest"],
    }),
    signal: args.signal,
  });
  if (!res.ok) throw new Error(`delegate registry RPC returned ${res.status}`);
  const body = RpcResponseSchema.parse(await res.json());
  if (body.error) {
    throw new Error(`delegate registry RPC error: ${body.error.message ?? "unknown"}`);
  }
  const result = body.result ?? "";
  if (!/^0x[0-9a-f]*$/i.test(result) || result.length < 66) {
    throw new Error("delegate registry returned no address \u{2014} wrong contract or chain");
  }
  return result as `0x${string}`;
}

function decodeDelegate(raw: `0x${string}`): string | null {
  const address = decodeFunctionResult({
    abi: DELEGATE_REGISTRY_ABI,
    functionName: "delegation",
    data: raw,
  });
  const lower = String(address).toLowerCase();
  return lower === ZERO_ADDRESS ? null : lower;
}

export async function readDelegation(args: {
  registry: DelegateRegistryConfig;
  rpcUrl: string;
  space: string;
  delegator: string;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}): Promise<DelegationState> {
  const spaceScoped = await ethCall({
    rpcUrl: args.rpcUrl,
    to: args.registry.address,
    data: encodeDelegationQuery(args.delegator, snapshotSpaceId(args.space)),
    fetchImpl: args.fetchImpl,
    signal: args.signal,
  });
  const scoped = decodeDelegate(spaceScoped);
  if (scoped) return { delegate: scoped, scope: "space" };

  const global = await ethCall({
    rpcUrl: args.rpcUrl,
    to: args.registry.address,
    data: encodeDelegationQuery(args.delegator, GLOBAL_SPACE_ID),
    fetchImpl: args.fetchImpl,
    signal: args.signal,
  });
  const fallback = decodeDelegate(global);
  return fallback
    ? { delegate: fallback, scope: "global" }
    : { delegate: null, scope: "none" };
}
