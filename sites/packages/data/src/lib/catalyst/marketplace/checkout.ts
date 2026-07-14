import type { z } from "zod";

import { CatalystError, postJSON, signedGetJSON } from "../client";
import type { AuthIdentity } from "../../auth/types";
import {
  addCartItem,
  fetchCart,
  type CartLine,
  type ItemRef,
} from "./cart";
import type { SignedPurchaseIntent } from "./purchase-intent";

import type { BalanceOut as RsBalanceOut } from "@ui/generated/catalyst/credits/BalanceOut";
import type { CheckoutStartOut as RsCheckoutStartOut } from "@ui/generated/catalyst/credits/CheckoutStartOut";
import type { CheckoutOut as RsCheckoutOut } from "@ui/generated/catalyst/credits/CheckoutOut";
import {
  BalanceOutSchema,
  CheckoutOutSchema,
  CheckoutStartOutSchema,
} from "../generated-schemas/credits";

export const BalanceSchema = BalanceOutSchema;
export type Balance = z.infer<typeof BalanceSchema>;

export async function fetchBalance(
  identity: AuthIdentity,
  address: string,
  signal?: AbortSignal,
): Promise<Balance> {
  const raw = await signedGetJSON<unknown>(
    `/credits/wallet/${encodeURIComponent(address)}/balance`,
    { identity, signal },
  );
  return BalanceSchema.parse(raw);
}

export const CheckoutStartSchema = CheckoutStartOutSchema;
export type CheckoutStart = z.infer<typeof CheckoutStartSchema>;

export async function startCheckout(
  identity: AuthIdentity,
  idempotencyKey: string,
  signal?: AbortSignal,
  scope?: ItemRef[],
  signed?: SignedPurchaseIntent,
): Promise<CheckoutStart> {
  const body: Record<string, unknown> = {};
  if (scope && scope.length > 0) {
    body.items = scope.map((r) => ({ collection: r.collection, itemId: r.itemId }));
  }
  if (signed) {
    body.intent = signed.intent;
    body.intentSignature = signed.signature;
  }
  const raw = await postJSON<unknown>("/credits/checkout", body, {
    identity,
    headers: { "Idempotency-Key": idempotencyKey },
    signal,
  });
  return CheckoutStartSchema.parse(raw);
}


function lineMatches(line: CartLine, ref: ItemRef): boolean {
  return (
    line.itemId === ref.itemId &&
    (line.collection ?? "").toLowerCase() === ref.collection
  );
}

export type ExpressQuote = { line: CartLine; added: boolean };

export async function quoteExpressItem(
  identity: AuthIdentity,
  ref: ItemRef,
  signal?: AbortSignal,
): Promise<ExpressQuote | null> {
  const cart = await fetchCart(identity, signal);
  const existing = cart.items.find((l) => lineMatches(l, ref));
  if (existing) return { line: existing, added: false };
  const next = await addCartItem(identity, ref, 1, signal);
  const line = next.items.find((l) => lineMatches(l, ref));
  return line ? { line, added: true } : null;
}

export async function startExpressCheckout(
  identity: AuthIdentity,
  ref: ItemRef,
  idempotencyKey: string,
  signal?: AbortSignal,
  signed?: SignedPurchaseIntent,
): Promise<CheckoutStart> {
  await addCartItem(identity, ref, 1, signal);
  return startCheckout(identity, idempotencyKey, signal, [ref], signed);
}

export const CheckoutSchema = CheckoutOutSchema;
export type Checkout = z.infer<typeof CheckoutSchema>;

export async function fetchCheckout(
  identity: AuthIdentity,
  id: number,
  signal?: AbortSignal,
): Promise<Checkout> {
  const raw = await signedGetJSON<unknown>(`/credits/checkout/${id}`, {
    identity,
    signal,
  });
  return CheckoutSchema.parse(raw);
}

export const CHECKOUT_DONE = new Set(["fulfilled"]);
export const CHECKOUT_FAILED = new Set(["failed", "reversed"]);

export type CheckoutPhase = "pending" | "done" | "failed";

export function checkoutPhase(status: string): CheckoutPhase {
  const s = status.toLowerCase();
  if (CHECKOUT_DONE.has(s)) return "done";
  if (CHECKOUT_FAILED.has(s)) return "failed";
  return "pending";
}

export function isPriceDriftError(err: unknown): err is CatalystError {
  return err instanceof CatalystError && err.status === 409;
}

export function checkoutErrorMessage(err: unknown): string | null {
  if (err instanceof CatalystError && err.serverMessage) return err.message;
  return null;
}

export function applyFreshQuotes(
  lines: CartLine[],
  credits: (string | null)[],
): { lines: CartLine[]; totalCredits: string } | null {
  if (credits.length < lines.length) return null;
  let total = 0n;
  const next: CartLine[] = [];
  for (let i = 0; i < lines.length; i++) {
    const c = credits[i];
    if (c == null || !/^\d+$/.test(c)) return null;
    if (lines[i].qty < 0) return null;
    total += BigInt(c) * BigInt(lines[i].qty);
    next.push({ ...lines[i], unitPriceCredits: c });
  }
  return { lines: next, totalCredits: total.toString() };
}

export function newIdempotencyKey(): string {
  try {
    if (typeof crypto !== "undefined" && crypto.randomUUID) {
      return crypto.randomUUID();
    }
  } catch {
  }
  return `idem-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export type _DriftBalance = Assert<Equal<Balance, RsBalanceOut>>;
export type _DriftCheckoutStart = Assert<
  AssignableTo<RsCheckoutStartOut, z.input<typeof CheckoutStartSchema>>
>;
export type _DriftCheckout = Assert<Equal<Checkout, RsCheckoutOut>>;
