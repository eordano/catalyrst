import { z } from "zod";

import { getJSON, postJSON, CatalystError } from "../client";
import type { AuthIdentity } from "../../auth/types";

import {
  ManaTopupOutSchema,
  ManaTopupQuoteOutSchema,
  MockTopupOutSchema,
} from "../generated-schemas/credits";
import {
  PaymentsBalanceOutSchema,
  PaymentsConfigSchema,
  PaymentsNonceOutSchema,
} from "../generated-schemas/economy";

type MockTopup = z.infer<typeof MockTopupOutSchema>;

export async function mockCardTopup(
  identity: AuthIdentity,
  credits: string,
  signal?: AbortSignal,
): Promise<MockTopup> {
  const raw = await postJSON<unknown>(
    "/credits/topup/mock-card",
    { credits },
    { identity, signal },
  );
  return MockTopupOutSchema.parse(raw);
}

export type ManaTopupQuote = z.infer<typeof ManaTopupQuoteOutSchema>;

export async function quoteManaTopup(
  credits: string,
  signal?: AbortSignal,
): Promise<ManaTopupQuote> {
  const raw = await getJSON<unknown>("/credits/topup/mana/quote", {
    query: { credits },
    signal,
  });
  return ManaTopupQuoteOutSchema.parse(raw);
}

const ManaTopupPendingSchema = z.object({ status: z.literal("pending") });

type ManaTopupResult =
  | { state: "granted"; creditsGranted: string; available: string }
  | { state: "pending" };

export async function redeemManaTopup(
  identity: AuthIdentity,
  txHash: string,
  signal?: AbortSignal,
): Promise<ManaTopupResult> {
  const raw = await postJSON<unknown>(
    "/credits/topup/mana",
    { txHash },
    { identity, signal },
  );
  const pending = ManaTopupPendingSchema.safeParse(raw);
  if (pending.success) return { state: "pending" };
  const granted = ManaTopupOutSchema.parse(raw);
  return {
    state: "granted",
    creditsGranted: granted.creditsGranted,
    available: granted.available,
  };
}

export type PaymentsConfig = z.infer<typeof PaymentsConfigSchema>;

export async function fetchPaymentsConfig(
  signal?: AbortSignal,
): Promise<PaymentsConfig> {
  const raw = await getJSON<unknown>("/v1/payments/config", { signal });
  return PaymentsConfigSchema.parse(raw);
}

export async function fetchManaNonce(
  address: string,
  signal?: AbortSignal,
): Promise<string> {
  const raw = await getJSON<unknown>(
    `/v1/payments/nonce/${encodeURIComponent(address.toLowerCase())}`,
    { signal },
  );
  return PaymentsNonceOutSchema.parse(raw).nonce;
}

const BALANCE_TIMEOUT_MS = 8000;

function withTimeout(signal: AbortSignal | undefined, ms: number): AbortSignal {
  const timeout = AbortSignal.timeout(ms);
  if (!signal) return timeout;
  return typeof AbortSignal.any === "function"
    ? AbortSignal.any([signal, timeout])
    : timeout;
}

export async function fetchManaBalance(
  address: string,
  opts: { signal?: AbortSignal; timeoutMs?: number; fetchImpl?: typeof fetch } = {},
): Promise<bigint> {
  const raw = await getJSON<unknown>(
    `/v1/payments/balance/${encodeURIComponent(address.toLowerCase())}`,
    {
      signal: withTimeout(opts.signal, opts.timeoutMs ?? BALANCE_TIMEOUT_MS),
      fetchImpl: opts.fetchImpl,
    },
  );
  return BigInt(PaymentsBalanceOutSchema.parse(raw).balance);
}

export function isMockCardOff(err: unknown): boolean {
  return err instanceof CatalystError && err.status === 501;
}
