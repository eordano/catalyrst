export async function unavailablePurchase(): Promise<never> {
  throw new Error("Purchases are not available here yet. No approval, signature or transaction was requested.");
}
