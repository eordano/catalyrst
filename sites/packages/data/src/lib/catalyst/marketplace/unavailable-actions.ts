export async function unavailablePurchase(): Promise<never> {
  throw new Error("Purchases are not available here yet. No approval, signature or transaction was requested.");
}

export async function unavailableNameClaim(): Promise<never> {
  throw new Error("NAME registration is not available here yet. No signature was requested and no NAME was minted.");
}
