import { toHex } from "viem";
import { z } from "zod";

import { WalletError, walletProvider, type Eip1193Provider } from "../../auth/wallet";
import {
  encodeSetDelegate,
  requireAddress,
  ZERO_ADDRESS,
  type DelegateRegistryConfig,
} from "./delegate-registry";
import type { DelegateArgs, DelegateReceipt, DelegateVpFn } from "./delegate-vp";

export class DelegationUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DelegationUnavailableError";
  }
}

export type DelegateTxRequest = {
  from: `0x${string}`;
  to: `0x${string}`;
  data: `0x${string}`;
};

export function buildDelegateTx(args: {
  registry: DelegateRegistryConfig;
  space: string;
  from: string;
  delegate: string;
}): DelegateTxRequest {
  const from = requireAddress(args.from, "wallet address");
  const delegate = requireAddress(args.delegate, "delegate");
  if (delegate === ZERO_ADDRESS) {
    throw new Error("delegation refused: 0x0 is not a delegate");
  }
  if (delegate === from) {
    throw new Error("delegation refused: you cannot delegate to your own wallet");
  }
  return {
    from,
    to: args.registry.address,
    data: encodeSetDelegate(args.space, delegate),
  };
}

const ReceiptSchema = z
  .object({
    status: z.string().nullish(),
    blockNumber: z.string().nullish(),
  })
  .nullable();

export type DelegateWalletOptions = {
  registry: DelegateRegistryConfig | null;
  provider?: Eip1193Provider;
  currentDelegate?: string | null;
  pollIntervalMs?: number;
  confirmTimeoutMs?: number;
};

function providerCode(error: unknown): number | null {
  const code = (error as { code?: unknown } | null)?.code;
  return typeof code === "number" ? code : null;
}

function providerMessage(error: unknown): string {
  const raw = (error as { message?: unknown } | null)?.message;
  return typeof raw === "string" && raw.trim() ? raw.trim() : String(error);
}

async function connectedAddress(provider: Eip1193Provider): Promise<string> {
  const existing = (await provider.request({ method: "eth_accounts" })) as string[] | null;
  const current = existing?.[0];
  if (current) return current;
  const granted = (await provider.request({ method: "eth_requestAccounts" })) as
    | string[]
    | null;
  const account = granted?.[0];
  if (!account) throw new WalletError("Wallet returned no accounts.");
  return account;
}

async function readChainId(provider: Eip1193Provider): Promise<number> {
  const hex = (await provider.request({ method: "eth_chainId" })) as string;
  const chainId = Number.parseInt(hex, 16);
  if (!Number.isInteger(chainId)) throw new WalletError("Wallet returned an invalid chain id.");
  return chainId;
}

async function requireChain(provider: Eip1193Provider, chainId: number): Promise<void> {
  if ((await readChainId(provider)) === chainId) return;
  try {
    await provider.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: toHex(chainId) }],
    });
  } catch (error) {
    const code = providerCode(error);
    if (code === 4001) {
      throw new WalletError(`Network switch rejected. Delegation happens on chain ${chainId}.`, 4001);
    }
    if (code === 4902) {
      throw new WalletError(`Add chain ${chainId} to your wallet to delegate.`, 4902);
    }
    throw new WalletError(
      `Could not switch to chain ${chainId}: ${providerMessage(error)}`,
      code ?? undefined,
    );
  }
  if ((await readChainId(provider)) !== chainId) {
    throw new WalletError(`Wrong network: switch your wallet to chain ${chainId} to delegate.`);
  }
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}

async function waitForReceipt(args: {
  provider: Eip1193Provider;
  txHash: string;
  pollIntervalMs: number;
  confirmTimeoutMs: number;
  signal?: AbortSignal;
}): Promise<{ status: "confirmed" | "pending"; blockNumber: number | null }> {
  const deadline = Date.now() + args.confirmTimeoutMs;
  for (;;) {
    const raw = await args.provider.request({
      method: "eth_getTransactionReceipt",
      params: [args.txHash],
    });
    const receipt = ReceiptSchema.parse(raw ?? null);
    if (receipt) {
      if (receipt.status === "0x0") {
        throw new Error(`delegation transaction reverted on chain (${args.txHash})`);
      }
      const blockNumber = receipt.blockNumber ? Number.parseInt(receipt.blockNumber, 16) : null;
      return {
        status: "confirmed",
        blockNumber: Number.isFinite(blockNumber) ? blockNumber : null,
      };
    }
    if (args.signal?.aborted || Date.now() >= deadline) {
      return { status: "pending", blockNumber: null };
    }
    await sleep(args.pollIntervalMs, args.signal);
  }
}

export function buildDelegateVp(options: DelegateWalletOptions): DelegateVpFn {
  return async ({ space, delegate, vp, signal }: DelegateArgs): Promise<DelegateReceipt> => {
    const registry = options.registry;
    if (!registry) {
      throw new DelegationUnavailableError(
        "delegation unavailable: the Snapshot delegate registry contract and chain are not configured",
      );
    }

    const provider = options.provider ?? walletProvider();
    const from = await connectedAddress(provider);
    const target = requireAddress(delegate, "delegate");
    const request = buildDelegateTx({ registry, space, from, delegate: target });

    if (options.currentDelegate?.trim().toLowerCase() === target) {
      throw new Error("delegation refused: your voting power is already delegated to this address");
    }

    await requireChain(provider, registry.chainId);

    const txHash = (await provider.request({
      method: "eth_sendTransaction",
      params: [request],
    })) as string;
    if (typeof txHash !== "string" || !/^0x[0-9a-f]{64}$/i.test(txHash)) {
      throw new WalletError("Wallet returned an invalid transaction hash.");
    }

    const { status, blockNumber } = await waitForReceipt({
      provider,
      txHash,
      pollIntervalMs: options.pollIntervalMs ?? 4_000,
      confirmTimeoutMs: options.confirmTimeoutMs ?? 120_000,
      signal,
    });

    return {
      space,
      delegate: target,
      vp,
      txHash,
      chainId: registry.chainId,
      status,
      blockNumber,
    };
  };
}
