import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./cart", () => ({
  fetchCart: vi.fn(),
  addCartItem: vi.fn(),
}));
vi.mock("../client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../client")>();
  return { ...actual, postJSON: vi.fn(), signedGetJSON: vi.fn() };
});

import { postJSON } from "../client";
import { addCartItem, fetchCart } from "./cart";
import type { Cart, CartLine, ItemRef } from "./cart";
import type { AuthIdentity } from "../../auth/types";
import { quoteExpressItem, startExpressCheckout } from "./checkout";

const mFetchCart = vi.mocked(fetchCart);
const mAddCartItem = vi.mocked(addCartItem);
const mPostJSON = vi.mocked(postJSON);

const OWNER = "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295";
const IDENTITY: AuthIdentity = {
  signer: OWNER,
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

const REF: ItemRef = { collection: "0x1111111111111111111111111111111111111111", itemId: "7" };
const OTHER1: ItemRef = { collection: "0x2222222222222222222222222222222222222222", itemId: "3" };
const OTHER2: ItemRef = { collection: "0x3333333333333333333333333333333333333333", itemId: "5" };

const CHECKOUT_ID = 39;

function mkLine(
  collection: string,
  itemId: string,
  qty: number,
  unitPriceCredits = "10",
): CartLine {
  return {
    itemId,
    collection,
    urn: `urn:decentraland:matic:collections-v2:${collection}:${itemId}`,
    category: "wearable",
    qty,
    unitPriceCredits,
  };
}

function mkCart(items: CartLine[]): Cart {
  const total = items.reduce((n, i) => n + Number(i.unitPriceCredits) * i.qty, 0);
  return { address: OWNER, items, totalCredits: String(total) };
}

const refLine = mkLine(REF.collection, REF.itemId, 1, "50");
const other1Line = mkLine(OTHER1.collection, OTHER1.itemId, 2, "40");
const other2Line = mkLine(OTHER2.collection, OTHER2.itemId, 1, "25");

let log: string[];

beforeEach(() => {
  vi.clearAllMocks();
  log = [];

  mAddCartItem.mockImplementation(async (_identity, ref, qty) => {
    log.push(`add ${ref.collection}/${ref.itemId} x${qty ?? 1}`);
    return mkCart([mkLine(ref.collection, ref.itemId, qty ?? 1, "50")]);
  });

  mPostJSON.mockImplementation((async (path: string) => {
    log.push(`POST ${path}`);
    return { id: CHECKOUT_ID, status: "fulfilling", replayed: false };
  }) as unknown as typeof postJSON);
});

type CheckoutBody = { items?: Array<{ collection: string; itemId: string }> };

describe("startExpressCheckout \u{2014} money-safety scope (over-charge guard)", () => {
  it("POSTs a checkout scoped to exactly [ref], touching no other cart line, whether the shared cart is full or empty", async () => {
    mFetchCart.mockResolvedValue(mkCart([refLine, other1Line, other2Line]));

    const res = await startExpressCheckout(IDENTITY, REF, "idem-1");
    expect(res.id).toBe(CHECKOUT_ID);

    const fullPosts = mPostJSON.mock.calls.filter((c) => c[0] === "/credits/checkout");
    expect(fullPosts).toHaveLength(1);
    expect((fullPosts[0][1] as CheckoutBody).items).toEqual([
      { collection: REF.collection, itemId: REF.itemId },
    ]);
    expect(log).toEqual([
      `add ${REF.collection}/${REF.itemId} x1`,
      `POST /credits/checkout`,
    ]);
    const opts = fullPosts[0][2] as { headers?: Record<string, string> };
    expect(opts.headers?.["Idempotency-Key"]).toBe("idem-1");

    mPostJSON.mockClear();
    mAddCartItem.mockClear();
    log = [];
    mFetchCart.mockResolvedValue(mkCart([]));

    await startExpressCheckout(IDENTITY, REF, "idem-2");

    const emptyPosts = mPostJSON.mock.calls.filter((c) => c[0] === "/credits/checkout");
    expect(emptyPosts).toHaveLength(1);
    expect((emptyPosts[0][1] as CheckoutBody).items).toEqual([
      { collection: REF.collection, itemId: REF.itemId },
    ]);
    expect(mAddCartItem).toHaveBeenCalledTimes(1);
    expect(mAddCartItem).toHaveBeenCalledWith(IDENTITY, REF, 1, undefined);
    expect(log).toEqual([
      `add ${REF.collection}/${REF.itemId} x1`,
      `POST /credits/checkout`,
    ]);
  });
});

describe("quoteExpressItem \u{2014} non-destructive price read", () => {
  it("returns added:false and never writes when the line already exists", async () => {
    mFetchCart.mockResolvedValue(mkCart([refLine, other1Line]));

    const q = await quoteExpressItem(IDENTITY, REF);

    expect(q).not.toBeNull();
    expect(q?.added).toBe(false);
    expect(q?.line.itemId).toBe(REF.itemId);
    expect(mAddCartItem).not.toHaveBeenCalled();
  });

  it("returns added:true and adds the line (qty 1) when it must be priced, and null when the added line can't be found", async () => {
    mFetchCart.mockResolvedValue(mkCart([other1Line]));

    const q = await quoteExpressItem(IDENTITY, REF);

    expect(q?.added).toBe(true);
    expect(q?.line.itemId).toBe(REF.itemId);
    expect(mAddCartItem).toHaveBeenCalledTimes(1);
    expect(mAddCartItem).toHaveBeenCalledWith(IDENTITY, REF, 1, undefined);

    mFetchCart.mockResolvedValue(mkCart([]));
    mAddCartItem.mockResolvedValue(mkCart([other1Line]));
    expect(await quoteExpressItem(IDENTITY, REF)).toBeNull();
  });
});

describe("409 price-drift plumbing", () => {
  it("isPriceDriftError matches only 409 CatalystErrors and checkoutErrorMessage returns only genuine server messages", async () => {
    const { CatalystError } = await import("../client");
    const { checkoutErrorMessage, isPriceDriftError } = await import("./checkout");
    expect(isPriceDriftError(new CatalystError("moved", "u", 409, true))).toBe(true);
    expect(isPriceDriftError(new CatalystError("nope", "u", 402, true))).toBe(false);
    expect(isPriceDriftError(new CatalystError("nope", "u", 0))).toBe(false);
    expect(isPriceDriftError(new Error("409"))).toBe(false);
    expect(isPriceDriftError(null)).toBe(false);

    expect(checkoutErrorMessage(new CatalystError("total changed", "u", 409, true))).toBe(
      "total changed",
    );
    expect(
      checkoutErrorMessage(new CatalystError("Catalyst returned 409 Conflict", "u", 409)),
    ).toBeNull();
    expect(checkoutErrorMessage(new Error("boom"))).toBeNull();
  });

  it("applyFreshQuotes swaps fresh unit prices, recomputes the total exactly, and fails closed on any unquotable or malformed entry", async () => {
    const { applyFreshQuotes } = await import("./checkout");
    const lines = [
      mkLine(REF.collection, REF.itemId, 2, "50"),
      mkLine(OTHER1.collection, OTHER1.itemId, 1, "40"),
    ];
    const out = applyFreshQuotes(lines, ["7", "3"]);
    expect(out).not.toBeNull();
    expect(out!.lines.map((l) => l.unitPriceCredits)).toEqual(["7", "3"]);
    expect(out!.totalCredits).toBe("17");
    expect(lines[0].unitPriceCredits).toBe("50");

    const single = [mkLine(REF.collection, REF.itemId, 1, "50")];
    expect(applyFreshQuotes(single, [null])).toBeNull();
    expect(applyFreshQuotes(single, [])).toBeNull();
    expect(applyFreshQuotes(single, ["1.5"])).toBeNull();
    expect(applyFreshQuotes(single, ["-2"])).toBeNull();
    expect(applyFreshQuotes(single, ["abc"])).toBeNull();
  });
});
