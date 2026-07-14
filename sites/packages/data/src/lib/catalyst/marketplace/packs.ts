import { z } from "zod";

import { CatalystError, getJSON, postJSON } from "../client";
import type { GetOptions } from "../client";
import type { AuthIdentity } from "../../auth/types";

import type { PackOut as RsPackOut } from "@ui/generated/catalyst/credits/PackOut";
import type { PackIntentOut as RsPackIntentOut } from "@ui/generated/catalyst/credits/PackIntentOut";
import {
  MockPurchaseOutSchema,
  PackOutSchema,
} from "../generated-schemas/credits";

export const PackSchema = PackOutSchema;
export type Pack = z.infer<typeof PackSchema>;

export const PacksSchema = z.array(PackSchema);

/**
 * `null` when the body is not a pack list. Every field of a pack is a term of
 * sale -- sku, credits, price, currency -- so a row that fails here has no price
 * to show; `loadPacks` turns the null into "unavailable" rather than a store
 * that appears to sell nothing.
 */
export function parsePacks(raw: unknown): Pack[] | null {
  const r = PacksSchema.safeParse(raw);
  if (r.success) return r.data;
  if (typeof process !== "undefined" && process.env?.NODE_ENV !== "production") {
    console.warn("[catalyst] packs failed schema validation", r.error.issues);
  }
  return null;
}

export async function fetchPacks(opts: GetOptions = {}): Promise<Pack[]> {
  const path = "/credits/packs";
  const packs = parsePacks(await getJSON<unknown>(path, opts));
  if (packs === null) {
    throw new CatalystError("packs response did not match the pack list shape", path);
  }
  return packs;
}

export const PackIntentSchema = z.object({
  clientSecret: z.string(),
  paymentIntentId: z.string().nullish().transform((v) => v ?? null),
});
export type PackIntent = z.infer<typeof PackIntentSchema>;

export async function createPackIntent(
  identity: AuthIdentity,
  sku: string,
  signal?: AbortSignal,
): Promise<PackIntent> {
  const raw = await postJSON<unknown>(
    `/credits/packs/${encodeURIComponent(sku)}/intent`,
    {},
    { identity, signal },
  );
  return PackIntentSchema.parse(raw);
}

export const MockPurchaseSchema = MockPurchaseOutSchema;
export type MockPurchase = z.infer<typeof MockPurchaseSchema>;

export async function mockPurchasePack(
  identity: AuthIdentity,
  sku: string,
  signal?: AbortSignal,
): Promise<MockPurchase> {
  const raw = await postJSON<unknown>(
    `/credits/packs/${encodeURIComponent(sku)}/mock-purchase`,
    {},
    { identity, signal },
  );
  return MockPurchaseSchema.parse(raw);
}

export function formatPrice(cents: number, currency: string): string {
  const amount = cents / 100;
  try {
    return new Intl.NumberFormat(undefined, {
      style: "currency",
      currency: currency.toUpperCase(),
    }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency.toUpperCase()}`;
  }
}

export function formatCredits(credits: string): string {
  const trimmed = credits.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
  return trimmed || credits;
}

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export type _DriftPack = Assert<Equal<Pack, RsPackOut>>;
export type _DriftPackIntent = Assert<
  AssignableTo<RsPackIntentOut, z.input<typeof PackIntentSchema>>
>;
