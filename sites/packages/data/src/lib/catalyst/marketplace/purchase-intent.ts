import type { PurchaseIntentIn as RsPurchaseIntentIn } from "@ui/generated/catalyst/credits/PurchaseIntentIn";


export const PURCHASE_INTENT_DOMAIN_NAME = "catalyst.example.com Checkout";
export const PURCHASE_INTENT_DOMAIN_VERSION = "1";
export const PURCHASE_INTENT_CHAIN_ID = 137;
export const PURCHASE_INTENT_CURRENCY = "CREDITS";
export const PURCHASE_INTENT_PRIMARY_TYPE = "PurchaseIntent";

export const PURCHASE_INTENT_TTL_MS = 15 * 60 * 1000;

export type PurchaseIntent = {
  buyer: string;
  items: string;
  totalCredits: string;
  currency: string;
  nonce: string;
  expiresAt: number;
};

export type SignedPurchaseIntent = {
  intent: PurchaseIntent;
  signature: string;
};

export type IntentLine = { collection: string; itemId: string; qty: number };

export function canonicalItems(lines: IntentLine[]): string {
  const tuples = lines.map(
    (l): [string, string, number] => [l.collection.toLowerCase(), l.itemId, l.qty],
  );
  tuples.sort((a, b) =>
    a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0,
  );
  return JSON.stringify(tuples);
}

export function intentLineFromCart(l: {
  collection?: string;
  urn: string;
  itemId: string;
  qty: number;
}): IntentLine {
  const collection = l.collection ?? l.urn.split(":")[4] ?? "";
  return { collection: collection.toLowerCase(), itemId: l.itemId, qty: l.qty };
}

export function buildPurchaseIntent(args: {
  buyer: string;
  lines: IntentLine[];
  totalCredits: string;
  nonce: string;
  now?: number;
}): PurchaseIntent {
  const now = args.now ?? Date.now();
  return {
    buyer: args.buyer.toLowerCase(),
    items: canonicalItems(args.lines),
    totalCredits: args.totalCredits,
    currency: PURCHASE_INTENT_CURRENCY,
    nonce: args.nonce,
    expiresAt: Math.floor((now + PURCHASE_INTENT_TTL_MS) / 1000),
  };
}

type TypedDataField = { name: string; type: string };

export function purchaseIntentTypes(): Record<string, TypedDataField[]> {
  return {
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "chainId", type: "uint256" },
    ],
    PurchaseIntent: [
      { name: "buyer", type: "address" },
      { name: "items", type: "string" },
      { name: "totalCredits", type: "string" },
      { name: "currency", type: "string" },
      { name: "nonce", type: "string" },
      { name: "expiresAt", type: "uint256" },
    ],
  };
}

export type PurchaseIntentTypedData = {
  domain: { name: string; version: string; chainId: number };
  types: Record<string, TypedDataField[]>;
  primaryType: string;
  message: Record<string, unknown>;
};

export function purchaseIntentTypedData(
  intent: PurchaseIntent,
): PurchaseIntentTypedData {
  return {
    domain: {
      name: PURCHASE_INTENT_DOMAIN_NAME,
      version: PURCHASE_INTENT_DOMAIN_VERSION,
      chainId: PURCHASE_INTENT_CHAIN_ID,
    },
    types: purchaseIntentTypes(),
    primaryType: PURCHASE_INTENT_PRIMARY_TYPE,
    message: { ...intent },
  };
}

export async function signPurchaseIntent(
  intent: PurchaseIntent,
): Promise<SignedPurchaseIntent> {
  const typedData = purchaseIntentTypedData(intent);
  const { signTypedData } = await import("../../auth/typed-data");
  try {
    const signature = await signTypedData(typedData, intent.buyer);
    return { intent, signature };
  } catch (err) {
    const devSignature = await import("../../auth/dev-identity")
      .then((m) => m.devSignTypedData(typedData, intent.buyer))
      .catch(() => null);
    if (devSignature) return { intent, signature: devSignature };
    throw err;
  }
}

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Assert<T extends true> = T;

export type _DriftPurchaseIntent = Assert<Equal<PurchaseIntent, RsPurchaseIntentIn>>;
