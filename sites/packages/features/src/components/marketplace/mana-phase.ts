import type { MkPayMethod } from "@ui/marketplace/components/MkPaymentSection";

import {
  MANA_POLYGON,
  manaShortfallWei,
} from "@data/lib/catalyst/marketplace/mana-pay";
import type {
  ManaTopupQuote,
  PaymentsConfig,
} from "@data/lib/catalyst/marketplace/topup";

export type ManaBalance = bigint | null | undefined;

export type ManaQuoteState =
  | { step: "loading" }
  | { step: "unavailable"; why: string }
  | { step: "quoted"; quote: ManaTopupQuote; config: PaymentsConfig };

export type ManaIdlePhase =
  | { step: "loading" }
  | { step: "unavailable"; why: string }
  | { step: "insufficient"; haveWei: string; needWei: string }
  | { step: "ready"; quote: ManaTopupQuote; config: PaymentsConfig };

export const MANA_UNAVAILABLE = "MANA payments aren't available right now.";
export const MANA_MISCONFIGURED =
  "MANA payments are misconfigured \u{2014} please use another method.";

export function defaultPayMethod(balance: bigint | null): MkPayMethod {
  return balance != null && balance > 0n ? "mana" : "card";
}

export function manaQuoteState(
  config: PaymentsConfig,
  quote: ManaTopupQuote,
): ManaQuoteState {
  if (!config.enabled || !config.payTo || !config.manaToken) {
    return { step: "unavailable", why: MANA_UNAVAILABLE };
  }
  if (config.manaToken.toLowerCase() !== MANA_POLYGON.address) {
    return { step: "unavailable", why: MANA_MISCONFIGURED };
  }
  return { step: "quoted", quote, config };
}

export function manaIdlePhase(
  quoted: ManaQuoteState,
  balance: ManaBalance,
): ManaIdlePhase {
  if (quoted.step !== "quoted") return quoted;
  if (balance === undefined) return { step: "loading" };
  if (
    balance !== null &&
    manaShortfallWei(balance, quoted.quote.weiSuggested) != null
  ) {
    return {
      step: "insufficient",
      haveWei: balance.toString(),
      needWei: quoted.quote.weiSuggested,
    };
  }
  return { step: "ready", quote: quoted.quote, config: quoted.config };
}
