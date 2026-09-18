import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  sellMachine,
  SELL_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveSellSnapshot,
  slugToState,
  stateToSlug,
  isValidPrice,
  type TrackFn,
} from "./machine";
import {
  buildSellOrder,
  failClosedCreate,
  type CreateOrderFn,
  type CreateOrderResult,
  type OwnedAsset,
} from "@data/lib/catalyst/marketplace/sell";
import { manaToWei, weiToMana } from "@data/lib/catalyst/marketplace/money";

const ASSETS: OwnedAsset[] = [
  {
    id: "0xabc-101",
    contractAddress: "0xabc",
    tokenId: "101",
    itemId: "0",
    issuedId: "101",
    activeOrderId: null,
    owner: "0xowner",
    name: "Test Wearable",
    category: "wearable",
    rarity: "epic",
    network: "MATIC",
    chainId: 137,
    image: null,
    urn: null,
    bodyShape: "Unisex",
    isOnSale: false,
  },
];

const okCreate: CreateOrderFn = async ({ order }): Promise<CreateOrderResult> => ({
  order: { ...order, id: "trade-1" },
  approvalTxHash: "0xtesttxhash",
});
function inputFor(createOrder: CreateOrderFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-sell-list",
      variant: "wizard",
      experimentKey: "marketplace_sell_wizard",
    },
    assets: ASSETS,
    createOrder,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "selectAsset",
  "setPrice",
  "setExpiration",
  "approveNft",
  "signOrder",
  "confirm",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SELECT_ASSET" as const, assetId: "0xabc-101" },
  { type: "SET_PRICE" as const, priceMana: 1000 },
  { type: "SET_PRICE" as const, priceMana: 0 },
  { type: "SET_EXPIRATION" as const, expiresAt: 1893456000000 },
  { type: "APPROVE" as const },
  { type: "SIGN" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("sellMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to select-asset", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(sellMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(Object.values(STATE_TO_SLUG)).toEqual([
      "select-asset",
      "set-price",
      "set-expiration",
      "approve-nft",
      "sign-order",
      "confirm",
      "success",
      "error",
    ]);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.selectAsset);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("selectAsset");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("sell.ts \u{2014} MANA <-> wei + order build", () => {
  it("manaToWei/weiToMana round-trip, and buildSellOrder projects an open MANA-wei Order with no minted id", () => {
    expect(manaToWei(1000)).toBe("1000000000000000000000");
    expect(weiToMana(manaToWei(1000))).toBe(1000);
    expect(manaToWei(0)).toBe("0");
    expect(manaToWei(-1)).toBe("0");

    const order = buildSellOrder({
      asset: ASSETS[0],
      priceMana: 1500,
      expiresAt: 1893456000000,
    });
    expect(order.status).toBe("open");
    expect(order.contractAddress).toBe("0xabc");
    expect(order.tokenId).toBe("101");
    expect(order.price).toBe(manaToWei(1500));
    expect(order.buyer).toBeNull();
    expect(order.expiresAt).toBe(1893456000000);
    expect(order.id).toBe("");
    expect(order.marketplaceAddress).toBe("0xa40b1d129b8906888720686f3a01921ddf37716f");
  });
});

describe("sellMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots selectAsset without a snapshot, hydrates confirm silently, and only real transitions track", async () => {
    const track = vi.fn();
    const createOrder = vi.fn(okCreate);
    const trackCtx = inputFor(createOrder, track).trackCtx;
    expect(resolveSellSnapshot({ step: "selectAsset", trackCtx, assets: ASSETS })).toBeUndefined();

    const confirm = createActor(sellMachine, {
      input: inputFor(createOrder, track),
      snapshot: resolveSellSnapshot({ step: "confirm", trackCtx, assets: ASSETS, createOrder, track }),
    }).start();
    expect(confirm.getSnapshot().matches("confirm")).toBe(true);
    expect(confirm.getSnapshot().context.assetId).toBe("0xabc-101");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(createOrder).not.toHaveBeenCalled();
    expect(confirm.getSnapshot().matches("confirm")).toBe(true);

    const price = createActor(sellMachine, {
      input: inputFor(okCreate, track),
      snapshot: resolveSellSnapshot({ step: "setPrice", trackCtx, assets: ASSETS, track }),
    }).start();
    expect(price.getSnapshot().matches("setPrice")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    price.send({ type: "SET_PRICE", priceMana: 0 });
    expect(price.getSnapshot().matches("setPrice")).toBe(true);
    expect(names(track)).toContain(SELL_EVENTS.priceInvalid);
  });
});

describe("sellMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and confirm needs the full step funnel", () => {
    const paths = getShortestPaths(sellMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["setPrice", "setExpiration", "approveNft", "signOrder", "confirm"]) {
      expect(ends.has(s)).toBe(true);
    }
    const confirm = paths.find((p) => (p.state.value as string) === "confirm");
    const events = confirm!.steps.map((s) => s.event.type);
    for (const e of ["SELECT_ASSET", "SET_PRICE", "SET_EXPIRATION", "APPROVE", "SIGN"]) {
      expect(events).toContain(e);
    }
  });
});

describe("sellMachine \u{2014} telemetry events (happy path)", () => {
  it("select -> price -> expiration -> approve -> sign -> confirm -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(sellMachine, { input: inputFor(okCreate, track) }).start();

    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    expect(actor.getSnapshot().matches("setPrice")).toBe(true);
    actor.send({ type: "SET_PRICE", priceMana: 1500 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
    actor.send({ type: "SET_EXPIRATION", expiresAt: 1893456000000 });
    expect(actor.getSnapshot().matches("approveNft")).toBe(true);
    actor.send({ type: "APPROVE" });
    expect(actor.getSnapshot().matches("signOrder")).toBe(true);
    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      SELL_EVENTS.started,
      SELL_EVENTS.assetSelected,
      SELL_EVENTS.priceSet,
      SELL_EVENTS.expirationSet,
      SELL_EVENTS.approveReached,
      SELL_EVENTS.signReached,
      SELL_EVENTS.confirmReached,
      SELL_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(SELL_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(SELL_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === SELL_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "marketplace_sell_wizard",
      variant: "wizard",
    });
    const completed = track.mock.calls.find((c) => c[0] === SELL_EVENTS.completed);
    expect(completed?.[1]?.approval_tx_hash).toBe("0xtesttxhash");
    expect(completed?.[1]?.order_id).toBe("trade-1");
  });

  it("only positive finite prices advance (guardrail fires otherwise), and BACK steps return to the previous step", () => {
    expect(isValidPrice(1000)).toBe(true);
    expect(isValidPrice(0.01)).toBe(true);
    expect(isValidPrice(0)).toBe(false);
    expect(isValidPrice(-5)).toBe(false);
    expect(isValidPrice(NaN)).toBe(false);
    expect(isValidPrice(Infinity)).toBe(false);

    const track = vi.fn();
    const actor = createActor(sellMachine, { input: inputFor(okCreate, track) }).start();
    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    actor.send({ type: "SET_PRICE", priceMana: 0 });
    expect(actor.getSnapshot().matches("setPrice")).toBe(true);
    expect(names(track)).toContain(SELL_EVENTS.priceInvalid);
    expect(names(track)).not.toContain(SELL_EVENTS.priceSet);

    actor.send({ type: "SET_PRICE", priceMana: 250 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
    expect(names(track)).toContain(SELL_EVENTS.priceSet);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("setPrice")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("selectAsset")).toBe(true);
  });
});

describe("sellMachine \u{2014} order-create failure + retry", () => {
  it("create error -> RETRY recovers to success, firing mk_sell_failed then completed", async () => {
    const track = vi.fn();
    let calls = 0;
    const createOrder: CreateOrderFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("gateway unreachable");
      return okCreate(args);
    };
    const actor = createActor(sellMachine, { input: inputFor(createOrder, track) }).start();

    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    actor.send({ type: "SET_PRICE", priceMana: 500 });
    actor.send({ type: "SET_EXPIRATION", expiresAt: 1893456000000 });
    actor.send({ type: "APPROVE" });
    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("gateway unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(SELL_EVENTS.failed);
    expect(names(track)).toContain(SELL_EVENTS.completed);
  });
});

describe("failClosedCreate", () => {
  it("is the default the wizard ships when a route injects nothing, and fails closed instead of fabricating a tx hash", async () => {
    const trackCtx = inputFor(okCreate, () => {}).trackCtx;
    const actor = createActor(sellMachine, { input: { trackCtx, assets: ASSETS } }).start();
    expect(actor.getSnapshot().context.createOrder).toBe(failClosedCreate);
    const hydrated = resolveSellSnapshot({ step: "confirm", trackCtx, assets: ASSETS });
    expect(hydrated?.context.createOrder).toBe(failClosedCreate);

    const order = buildSellOrder({
      asset: ASSETS[0],
      priceMana: 1000,
      expiresAt: Date.now() + 1000,
    });
    await expect(failClosedCreate({ order })).rejects.toThrow(
      "listing unavailable: order relayer not configured",
    );
  });
});
