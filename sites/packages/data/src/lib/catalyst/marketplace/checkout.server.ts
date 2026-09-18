import type { Balance } from "./checkout";

type CheckoutLoad = {
  balance: Balance | null;
  isFixture: boolean;
};

export async function loadCheckout(): Promise<CheckoutLoad> {
  return { balance: null, isFixture: false };
}
