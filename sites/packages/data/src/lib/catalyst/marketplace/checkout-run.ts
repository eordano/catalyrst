import type { FulfillFn } from "@features/stories/marketplace/checkout/machine";
import type { AuthIdentity } from "../../auth/types";
import type { ItemRef } from "./cart";
import {
  checkoutPhase,
  fetchCheckout,
  startCheckout,
  startExpressCheckout,
} from "./checkout";
import { clearPendingCheckout, setPendingCheckout } from "./pending-checkout";
import type { IntentLine, SignedPurchaseIntent } from "./purchase-intent";

export const POLL_MS = 2000;
export const POLL_MAX = 60;

export type CancelTarget = { label: string; href: string };

export type GetSigned = () => SignedPurchaseIntent | null;
export type CreateRun = (getSigned: GetSigned) => FulfillFn;

export type FreshLines = { totalCredits: string; intentLines: IntentLine[] };
export type RefreshLines = () => Promise<FreshLines | null>;

export function delay(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, ms);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });
}

export async function pollToTerminal(
  identity: AuthIdentity,
  start: { id: number; status: string },
  signal?: AbortSignal,
): ReturnType<FulfillFn> {
  let status = start.status;
  const id = start.id;
  setPendingCheckout(identity.signer, { checkoutId: id, ts: Date.now() });
  for (let i = 0; i < POLL_MAX && checkoutPhase(status) === "pending"; i++) {
    await delay(POLL_MS, signal);
    const c = await fetchCheckout(identity, id, signal);
    status = c.status;
  }
  const phase = checkoutPhase(status);
  if (phase !== "pending") clearPendingCheckout(identity.signer);
  return { checkoutId: id, status, phase };
}

export function makeRun(identity: AuthIdentity, getSigned: GetSigned): FulfillFn {
  return async ({ idempotencyKey, signal }) =>
    pollToTerminal(
      identity,
      await startCheckout(
        identity,
        idempotencyKey,
        signal,
        undefined,
        getSigned() ?? undefined,
      ),
      signal,
    );
}

export function makeExpressRun(
  identity: AuthIdentity,
  ref: ItemRef,
  getSigned: GetSigned,
): FulfillFn {
  return async ({ idempotencyKey, signal }) =>
    pollToTerminal(
      identity,
      await startExpressCheckout(
        identity,
        ref,
        idempotencyKey,
        signal,
        getSigned() ?? undefined,
      ),
      signal,
    );
}
