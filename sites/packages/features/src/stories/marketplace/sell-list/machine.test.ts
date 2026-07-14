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

describe("sellMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(sellMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are unique and round-trip via SLUG_TO_STATE", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("slugs match the audit-spec step ids", () => {
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
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.selectAsset);
    expect(slugToState(null)).toBe("selectAsset");
    expect(slugToState(undefined)).toBe("selectAsset");
    expect(slugToState("")).toBe("selectAsset");
    expect(slugToState("nope")).toBe("selectAsset");
    expect(slugToState("set-price")).toBe("setPrice");
    expect(slugToState("sign-order")).toBe("signOrder");
    expect(slugToState("confirm")).toBe("confirm");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("isValidPrice accepts positive finite numbers only", () => {
    expect(isValidPrice(1000)).toBe(true);
    expect(isValidPrice(0.01)).toBe(true);
    expect(isValidPrice(0)).toBe(false);
    expect(isValidPrice(-5)).toBe(false);
    expect(isValidPrice(NaN)).toBe(false);
    expect(isValidPrice(Infinity)).toBe(false);
  });
});

describe("sell.ts \u{2014} MANA <-> wei + order build", () => {
  it("manaToWei / weiToMana round-trip whole amounts", () => {
    expect(manaToWei(1000)).toBe("1000000000000000000000");
    expect(weiToMana(manaToWei(1000))).toBe(1000);
    expect(manaToWei(0)).toBe("0");
    expect(manaToWei(-1)).toBe("0");
  });

  it("buildSellOrder projects inputs onto a schemas Order (open, MANA wei)", () => {
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
  });

  it("mints no order id: only the marketplace can name a listing", () => {
    const order = buildSellOrder({
      asset: ASSETS[0],
      priceMana: 1500,
      expiresAt: 1893456000000,
    });
    expect(order.id).toBe("");
    expect(order.marketplaceAddress).toBe(
      "0xa40b1d129b8906888720686f3a01921ddf37716f",
    );
  });
});

describe("sellMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveSellSnapshot({
      step: "selectAsset",
      trackCtx: inputFor(okCreate, () => {}).trackCtx,
      assets: ASSETS,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating confirm does NOT fire telemetry and does NOT auto-create the order", async () => {
    const track = vi.fn();
    const createOrder = vi.fn(okCreate);
    const snapshot = resolveSellSnapshot({
      step: "confirm",
      trackCtx: inputFor(createOrder, track).trackCtx,
      assets: ASSETS,
      createOrder,
      track,
    });
    const actor = createActor(sellMachine, {
      input: inputFor(createOrder, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirm")).toBe(true);
    expect(actor.getSnapshot().context.assetId).toBe("0xabc-101");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(createOrder).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("confirm")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveSellSnapshot({
      step: "setPrice",
      trackCtx: inputFor(okCreate, track).trackCtx,
      assets: ASSETS,
      track,
    });
    const actor = createActor(sellMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("setPrice")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SET_PRICE", priceMana: 0 });
    expect(actor.getSnapshot().matches("setPrice")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(SELL_EVENTS.priceInvalid);
  });
});

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

describe("sellMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("setPrice")).toBe(true);
    expect(ends.has("setExpiration")).toBe(true);
    expect(ends.has("approveNft")).toBe(true);
    expect(ends.has("signOrder")).toBe(true);
    expect(ends.has("confirm")).toBe(true);
  });

  it("reaching confirm passes through the full step funnel", () => {
    const paths = getShortestPaths(sellMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const confirm = paths.find((p) => (p.state.value as string) === "confirm");
    expect(confirm).toBeDefined();
    const events = confirm!.steps.map((s) => s.event.type);
    expect(events).toContain("SELECT_ASSET");
    expect(events).toContain("SET_PRICE");
    expect(events).toContain("SET_EXPIRATION");
    expect(events).toContain("APPROVE");
    expect(events).toContain("SIGN");
  });
});

describe("sellMachine \u{2014} telemetry events (happy path)", () => {
  it("select -> price -> expiration -> approve -> sign -> confirm -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(sellMachine, {
      input: inputFor(okCreate, track),
    }).start();

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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SELL_EVENTS.started);
    expect(events).toContain(SELL_EVENTS.assetSelected);
    expect(events).toContain(SELL_EVENTS.priceSet);
    expect(events).toContain(SELL_EVENTS.expirationSet);
    expect(events).toContain(SELL_EVENTS.approveReached);
    expect(events).toContain(SELL_EVENTS.signReached);
    expect(events).toContain(SELL_EVENTS.confirmReached);
    expect(events).toContain(SELL_EVENTS.completed);

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

  it("invalid price stays on setPrice and fires the guardrail (no price_set)", () => {
    const track = vi.fn();
    const actor = createActor(sellMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    actor.send({ type: "SET_PRICE", priceMana: 0 });
    expect(actor.getSnapshot().matches("setPrice")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SELL_EVENTS.priceInvalid);
    expect(events).not.toContain(SELL_EVENTS.priceSet);

    actor.send({ type: "SET_PRICE", priceMana: 250 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(SELL_EVENTS.priceSet);
  });

  it("BACK steps return to the previous step", () => {
    const track = vi.fn();
    const actor = createActor(sellMachine, {
      input: inputFor(okCreate, track),
    }).start();
    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    actor.send({ type: "SET_PRICE", priceMana: 100 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
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

    const actor = createActor(sellMachine, {
      input: inputFor(createOrder, track),
    }).start();

    actor.send({ type: "SELECT_ASSET", assetId: "0xabc-101" });
    actor.send({ type: "SET_PRICE", priceMana: 500 });
    actor.send({ type: "SET_EXPIRATION", expiresAt: 1893456000000 });
    actor.send({ type: "APPROVE" });
    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("gateway unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SELL_EVENTS.failed);
    expect(events).toContain(SELL_EVENTS.completed);
  });
});

describe("failClosedCreate", () => {
  it("is the default the wizard ships when a route injects nothing", async () => {
    const actor = createActor(sellMachine, {
      input: { trackCtx: inputFor(okCreate, () => {}).trackCtx, assets: ASSETS },
    }).start();
    expect(actor.getSnapshot().context.createOrder).toBe(failClosedCreate);

    const hydrated = resolveSellSnapshot({
      step: "confirm",
      trackCtx: inputFor(okCreate, () => {}).trackCtx,
      assets: ASSETS,
    });
    expect(hydrated?.context.createOrder).toBe(failClosedCreate);
  });

  it("fails closed instead of fabricating a tx hash", async () => {
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
