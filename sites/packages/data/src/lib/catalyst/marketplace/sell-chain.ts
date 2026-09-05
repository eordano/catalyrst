import { decodeFunctionResult, encodeFunctionData } from "viem";

import { toAddress, type Hex } from "./trade";
import type { Eip1193Provider } from "../../auth/wallet";

export const APPROVAL_FOR_ALL_ABI = [
  {
    type: "function",
    name: "isApprovedForAll",
    stateMutability: "view",
    inputs: [
      { name: "owner", type: "address" },
      { name: "operator", type: "address" },
    ],
    outputs: [{ name: "", type: "bool" }],
  },
  {
    type: "function",
    name: "setApprovalForAll",
    stateMutability: "nonpayable",
    inputs: [
      { name: "operator", type: "address" },
      { name: "approved", type: "bool" },
    ],
    outputs: [],
  },
] as const;

export const SIGNATURE_INDEX_ABI = [
  {
    type: "function",
    name: "contractSignatureIndex",
    stateMutability: "view",
    inputs: [],
    outputs: [{ name: "", type: "uint256" }],
  },
  {
    type: "function",
    name: "signerSignatureIndex",
    stateMutability: "view",
    inputs: [{ name: "signer", type: "address" }],
    outputs: [{ name: "", type: "uint256" }],
  },
] as const;

function asHex(value: unknown, what: string): Hex {
  if (typeof value !== "string" || !/^0x[0-9a-fA-F]*$/.test(value)) {
    throw new Error(`listing unavailable: the wallet returned an invalid ${what}`);
  }
  return value as Hex;
}

async function ethCall(
  provider: Eip1193Provider,
  to: Hex,
  data: Hex,
): Promise<Hex> {
  return asHex(
    await provider.request({ method: "eth_call", params: [{ to, data }, "latest"] }),
    "contract read",
  );
}

export async function readChainId(provider: Eip1193Provider): Promise<number> {
  const raw = await provider.request({ method: "eth_chainId" });
  if (typeof raw === "number" && Number.isInteger(raw)) return raw;
  return Number(BigInt(asHex(raw, "chain id")));
}

export type ApprovalTarget = {
  contractAddress: string;
  owner: string;
  operator: string;
};

export async function readIsApprovedForAll(
  provider: Eip1193Provider,
  target: ApprovalTarget,
): Promise<boolean> {
  const data = encodeFunctionData({
    abi: APPROVAL_FOR_ALL_ABI,
    functionName: "isApprovedForAll",
    args: [
      toAddress(target.owner, "the seller wallet"),
      toAddress(target.operator, "the marketplace contract"),
    ],
  });
  const result = await ethCall(
    provider,
    toAddress(target.contractAddress, "the item contract"),
    data,
  );
  return decodeFunctionResult({
    abi: APPROVAL_FOR_ALL_ABI,
    functionName: "isApprovedForAll",
    data: result,
  });
}

export async function sendSetApprovalForAll(
  provider: Eip1193Provider,
  target: ApprovalTarget,
): Promise<Hex> {
  const data = encodeFunctionData({
    abi: APPROVAL_FOR_ALL_ABI,
    functionName: "setApprovalForAll",
    args: [toAddress(target.operator, "the marketplace contract"), true],
  });
  const raw = await provider.request({
    method: "eth_sendTransaction",
    params: [
      {
        from: toAddress(target.owner, "the seller wallet"),
        to: toAddress(target.contractAddress, "the item contract"),
        data,
      },
    ],
  });
  return asHex(raw, "transaction hash");
}

export type TransactionReceipt = { txHash: Hex; blockNumber: number };

export type WaitOptions = {
  pollIntervalMs?: number;
  timeoutMs?: number;
  signal?: AbortSignal;
  sleep?: (ms: number) => Promise<void>;
  now?: () => number;
};

const DEFAULT_POLL_MS = 3_000;
const DEFAULT_TIMEOUT_MS = 300_000;

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

type RawReceipt = { status?: string; blockNumber?: string } | null;

export async function waitForTransaction(
  provider: Eip1193Provider,
  txHash: Hex,
  opts: WaitOptions = {},
): Promise<TransactionReceipt> {
  const now = opts.now ?? (() => Date.now());
  const sleep = opts.sleep ?? defaultSleep;
  const deadline = now() + (opts.timeoutMs ?? DEFAULT_TIMEOUT_MS);

  for (;;) {
    if (opts.signal?.aborted) {
      throw new Error("listing cancelled while the approval was confirming");
    }
    const receipt = (await provider.request({
      method: "eth_getTransactionReceipt",
      params: [txHash],
    })) as RawReceipt;

    if (receipt) {
      const status = receipt.status;
      if (status !== "0x1") {
        throw new Error(
          `listing unavailable: approval transaction ${txHash} did not succeed (status ${status ?? "unknown"})`,
        );
      }
      return {
        txHash,
        blockNumber: Number(BigInt(receipt.blockNumber ?? "0x0")),
      };
    }
    if (now() >= deadline) {
      throw new Error(
        `listing unavailable: approval transaction ${txHash} was not confirmed in time; nothing was signed`,
      );
    }
    await sleep(opts.pollIntervalMs ?? DEFAULT_POLL_MS);
  }
}

export type EnsureApprovalResult = { txHash: Hex | null };

export async function ensureApprovalForAll(
  provider: Eip1193Provider,
  target: ApprovalTarget,
  opts: WaitOptions = {},
): Promise<EnsureApprovalResult> {
  if (await readIsApprovedForAll(provider, target)) return { txHash: null };

  const txHash = await sendSetApprovalForAll(provider, target);
  await waitForTransaction(provider, txHash, opts);

  if (!(await readIsApprovedForAll(provider, target))) {
    throw new Error(
      `listing unavailable: approval transaction ${txHash} confirmed but the marketplace is still not approved`,
    );
  }
  return { txHash };
}

export type SignatureIndexes = {
  contractSignatureIndex: number;
  signerSignatureIndex: number;
};

function toIndex(value: bigint, what: string): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`listing unavailable: ${what} is out of range`);
  }
  return Number(value);
}

export async function readSignatureIndexes(
  provider: Eip1193Provider,
  marketplaceAddress: string,
  signer: string,
): Promise<SignatureIndexes> {
  const to = toAddress(marketplaceAddress, "the marketplace contract");
  const signerAddress = toAddress(signer, "the seller wallet");

  const contractRaw = await ethCall(
    provider,
    to,
    encodeFunctionData({
      abi: SIGNATURE_INDEX_ABI,
      functionName: "contractSignatureIndex",
    }),
  );
  const signerRaw = await ethCall(
    provider,
    to,
    encodeFunctionData({
      abi: SIGNATURE_INDEX_ABI,
      functionName: "signerSignatureIndex",
      args: [signerAddress],
    }),
  );

  return {
    contractSignatureIndex: toIndex(
      decodeFunctionResult({
        abi: SIGNATURE_INDEX_ABI,
        functionName: "contractSignatureIndex",
        data: contractRaw,
      }),
      "the marketplace signature index",
    ),
    signerSignatureIndex: toIndex(
      decodeFunctionResult({
        abi: SIGNATURE_INDEX_ABI,
        functionName: "signerSignatureIndex",
        data: signerRaw,
      }),
      "the seller signature index",
    ),
  };
}
