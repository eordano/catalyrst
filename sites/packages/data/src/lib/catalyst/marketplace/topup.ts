import { z } from "zod";

import { getJSON, postJSON, CatalystError } from "../client";
import type { AuthIdentity } from "../../auth/types";

import {
  ManaTopupOutSchema,
  ManaTopupQuoteOutSchema,
  MockTopupOutSchema,
} from "../generated-schemas/credits";
import {
  PaymentsConfigSchema,
  PaymentsNonceOutSchema,
} from "../generated-schemas/economy";

export {
  MockTopupOutSchema as MockTopupSchema,
  ManaTopupQuoteOutSchema as ManaTopupQuoteSchema,
  PaymentsConfigSchema,
};

export type MockTopup = z.infer<typeof MockTopupOutSchema>;

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

// The 202 answer is an ad-hoc `json!({ "status": "pending" })` in
// catalyrst-credits' topup handler; no ts-rs DTO exists for it, so this stays
// the one hand-written schema in the module.
const ManaTopupPendingSchema = z.object({ status: z.literal("pending") });

export type ManaTopupResult =
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

export function isMockCardOff(err: unknown): boolean {
  return err instanceof CatalystError && err.status === 501;
}
