import { PendingTopupStoreSchema } from "../../persisted-schemas";

import { createPendingStore } from "./pending-store";

export const PENDING_TOPUP_KEY = "dcl:mk:pending-mana-topup:v1";

const PENDING_TTL_MS = 7 * 24 * 60 * 60 * 1000;

export type PendingTopup = {
  txHash: string;
  ts: number;
};

const store = createPendingStore<PendingTopup>(
  PENDING_TOPUP_KEY,
  PENDING_TTL_MS,
  (entry) => typeof entry.txHash === "string" && entry.txHash !== "",
  PendingTopupStoreSchema,
  "persisted/pending-topup",
);

export function getPendingTopup(
  signer: string | null | undefined,
): PendingTopup | null {
  return store.get(signer);
}

export function setPendingTopup(
  signer: string | null | undefined,
  entry: PendingTopup,
): void {
  store.set(signer, entry);
}

export function clearPendingTopup(signer: string | null | undefined): void {
  store.clear(signer);
}
