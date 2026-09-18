import { expect, it, vi } from "vitest";
import { unavailablePurchase, unavailableNameClaim } from "./unavailable-actions";

it("does not produce a purchase or minted NAME receipt or contact a wallet", async () => {
  const request = vi.fn();
  vi.stubGlobal("ethereum", { request });
  try {
    await expect(unavailablePurchase()).rejects.toThrow(/not available/i);
    await expect(unavailableNameClaim()).rejects.toThrow(/no NAME was minted/i);
    expect(request).not.toHaveBeenCalled();
  } finally {
    vi.unstubAllGlobals();
  }
});
