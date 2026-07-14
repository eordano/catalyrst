import { PendingCheckoutStoreSchema } from "../../persisted-schemas";

import { createPendingStore } from "./pending-store";

export const PENDING_CHECKOUT_KEY = "dcl:mk:pending-checkout:v1";

const PENDING_TTL_MS = 24 * 60 * 60 * 1000;

export type PendingCheckout = {
  checkoutId: number;
  ts: number;
};

const store = createPendingStore<PendingCheckout>(
  PENDING_CHECKOUT_KEY,
  PENDING_TTL_MS,
  (entry) => typeof entry.checkoutId === "number",
  PendingCheckoutStoreSchema,
  "persisted/pending-checkout",
);

export function getPendingCheckout(
  signer: string | null | undefined,
): PendingCheckout | null {
  return store.get(signer);
}

export function setPendingCheckout(
  signer: string | null | undefined,
  entry: PendingCheckout,
): void {
  store.set(signer, entry);
}

export function clearPendingCheckout(signer: string | null | undefined): void {
  store.clear(signer);
}
