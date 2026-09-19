import { expect, it, vi } from "vitest";
import { unavailablePurchase } from "./unavailable-actions";

it("does not produce a purchase receipt or contact a wallet", async () => {
  const request = vi.fn();
  vi.stubGlobal("ethereum", { request });
  try {
    await expect(unavailablePurchase()).rejects.toThrow(/not available/i);
    expect(request).not.toHaveBeenCalled();
  } finally {
    vi.unstubAllGlobals();
  }
});
