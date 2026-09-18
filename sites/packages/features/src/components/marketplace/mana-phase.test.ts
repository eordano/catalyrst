import { describe, expect, it } from "vitest";

import { MANA_POLYGON } from "@data/lib/catalyst/marketplace/mana-pay";
import type {
  ManaTopupQuote,
  PaymentsConfig,
} from "@data/lib/catalyst/marketplace/topup";

import {
  MANA_MISCONFIGURED,
  MANA_UNAVAILABLE,
  defaultPayMethod,
  manaIdlePhase,
  manaQuoteState,
} from "./mana-phase";

const NEED = "39580378408756389366";
const CONFIG: PaymentsConfig = {
  chainId: 137,
  enabled: true,
  manaToken: MANA_POLYGON.address,
  payTo: "0x2a7fa4cad84d8ca921bd6c04f4462d99e35db6a2",
};
const QUOTE = { weiSuggested: NEED } as ManaTopupQuote;
const QUOTED = manaQuoteState(CONFIG, QUOTE);

describe("manaQuoteState", () => {
  it("quotes when the rail is enabled against the real MANA contract", () => {
    expect(QUOTED).toEqual({ step: "quoted", quote: QUOTE, config: CONFIG });
  });

  it("is honest about a disabled or misconfigured rail", () => {
    expect(manaQuoteState({ ...CONFIG, enabled: false }, QUOTE)).toEqual({
      step: "unavailable",
      why: MANA_UNAVAILABLE,
    });
    expect(manaQuoteState({ ...CONFIG, payTo: null }, QUOTE).step).toBe("unavailable");
    expect(
      manaQuoteState({ ...CONFIG, manaToken: "0x0000000000000000000000000000000000000001" }, QUOTE),
    ).toEqual({ step: "unavailable", why: MANA_MISCONFIGURED });
  });
});

describe("manaIdlePhase", () => {
  it("stays on loading until both the quote and the balance are known", () => {
    expect(manaIdlePhase({ step: "loading" }, 5n)).toEqual({ step: "loading" });
    expect(manaIdlePhase(QUOTED, undefined)).toEqual({ step: "loading" });
  });

  it("stops a short wallet before any signing, naming both amounts", () => {
    expect(manaIdlePhase(QUOTED, 0n)).toEqual({
      step: "insufficient",
      haveWei: "0",
      needWei: NEED,
    });
    expect(manaIdlePhase(QUOTED, BigInt(NEED) - 1n).step).toBe("insufficient");
  });

  it("offers Continue for a covered wallet, and when the balance could not be read", () => {
    expect(manaIdlePhase(QUOTED, BigInt(NEED))).toEqual({
      step: "ready",
      quote: QUOTE,
      config: CONFIG,
    });
    expect(manaIdlePhase(QUOTED, null).step).toBe("ready");
  });

  it("passes an unavailable rail through untouched", () => {
    const off = { step: "unavailable", why: MANA_UNAVAILABLE } as const;
    expect(manaIdlePhase(off, 5n)).toEqual(off);
  });
});

describe("defaultPayMethod", () => {
  it("preselects MANA only for a funded wallet", () => {
    expect(defaultPayMethod(1n)).toBe("mana");
    expect(defaultPayMethod(0n)).toBe("card");
    expect(defaultPayMethod(null)).toBe("card");
  });
});
