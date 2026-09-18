import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  buyMachine,
  BUY_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBuySnapshot,
  slugToState,
  stateToSlug,
  simulateTradeCommit,
  type BuyListing,
  type SimFn,
  type TradeResult,
  type TrackFn,
} from "./machine";

const LISTING: BuyListing = {
  assetId: "0xaaee4e0ea3de22dfc960a7f9c8bbd22f7081c5fa-1",
  contractAddress: "0xaaee4e0ea3de22dfc960a7f9c8bbd22f7081c5fa",
  tokenId: "105312291668557186697918027683670432318895095400549111254310978717",
  priceMana: "1",
  priceWei: "1000000000000000000",
  network: "polygon",
  marketplaceAddress: "0x480a0f4e360e8964e68858dd231c2922f1df45ef",
  chainId: 137,
  seller: "0x60047e2b1d7ab88389de4b1a2ed4fb4845cdd252",
};

const RESULT: TradeResult = { txHash: "0xdeadbeef" };

const okSim: SimFn = async () => RESULT;

function inputFor(sim: SimFn, track: TrackFn) {
  return {
    listing: LISTING,
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-buy-nft",
      variant: "wizard",
      experimentKey: "mk_buy_wizard",
    },
    connect: sim,
    approve: sim,
    commit: sim,
    track,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "CONFIRM" as const },
  { type: "CANCEL" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("buyMachine \u{2014} URL ?step slug map", () => {
  it("uses the shared-spec step ids, unique and round-tripping, falling back to review", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(buyMachine.states)));
    expect(STATE_TO_SLUG).toMatchObject({
      review: "review",
      connecting: "connect-wallet",
      approving: "approve-mana",
      confirming: "confirm-purchase",
      submitting: "submit-tx",
      success: "success",
    });
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("review");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("buyMachine \u{2014} deep-link hydration", () => {
  it("boots review without a snapshot, hydrates submit-tx/approve-mana silently, and only real transitions track", async () => {
    const track = vi.fn();
    const commit = vi.fn(okSim);
    const trackCtx = inputFor(commit, track).trackCtx;
    expect(resolveBuySnapshot({ step: "review", listing: LISTING, trackCtx })).toBeUndefined();

    const submitting = createActor(buyMachine, {
      input: inputFor(commit, track),
      snapshot: resolveBuySnapshot({ step: "submitting", listing: LISTING, trackCtx, commit, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const approving = createActor(buyMachine, {
      input: inputFor(okSim, track),
      snapshot: resolveBuySnapshot({ step: "approving", listing: LISTING, trackCtx, track }),
    }).start();
    expect(approving.getSnapshot().matches("approving")).toBe(true);
    expect(approving.getSnapshot().context.listing.assetId).toBe(LISTING.assetId);
    expect(track).not.toHaveBeenCalled();

    const confirming = createActor(buyMachine, {
      input: inputFor(okSim, track),
      snapshot: resolveBuySnapshot({ step: "confirming", listing: LISTING, trackCtx, track }),
    }).start();
    expect(confirming.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    confirming.send({ type: "CONFIRM" });
    expect(confirming.getSnapshot().matches("submitting")).toBe(true);
    expect(names(track)).toContain(BUY_EVENTS.confirmReached);
  });
});

describe("buyMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach review and connecting; connecting needs START and CANCEL at review is a no-op", () => {
    const paths = getShortestPaths(buyMachine, {
      input: inputFor(okSim, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    expect(ends.has("review")).toBe(true);
    expect(ends.has("connecting")).toBe(true);
    const connecting = paths.find((p) => (p.state.value as string) === "connecting");
    expect(connecting!.steps.map((s) => s.event.type)).toContain("START");

    const track = vi.fn();
    const actor = createActor(buyMachine, { input: inputFor(okSim, track) }).start();
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });
});

describe("buyMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> connect -> approve -> confirm -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(buyMachine, { input: inputFor(okSim, track) }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("confirming"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      BUY_EVENTS.started,
      BUY_EVENTS.walletConnected,
      BUY_EVENTS.manaApproved,
      BUY_EVENTS.confirmReached,
      BUY_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events).not.toContain(BUY_EVENTS.failed);
    expect(events.indexOf(BUY_EVENTS.started)).toBeLessThan(
      events.indexOf(BUY_EVENTS.confirmReached),
    );
    expect(events.indexOf(BUY_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(BUY_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === BUY_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "mk_buy_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("buyMachine \u{2014} failure + retry", () => {
  it("a failed phase routes to error and fires mk_buy_failed; RETRY after the transient failure recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const sim: SimFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("auth chain: Invalid Auth Chain");
      return okSim(args);
    };
    const actor = createActor(buyMachine, { input: inputFor(sim, track) }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("auth chain: Invalid Auth Chain");
    let events = names(track);
    expect(events).toContain(BUY_EVENTS.started);
    expect(events).toContain(BUY_EVENTS.failed);
    expect(events).not.toContain(BUY_EVENTS.completed);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("confirming"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));
    events = names(track);
    expect(events).toContain(BUY_EVENTS.completed);
    expect(events.indexOf(BUY_EVENTS.failed)).toBeLessThan(events.indexOf(BUY_EVENTS.completed));
  });
});

describe("simulateTradeCommit", () => {
  it("resolves a deterministic stub tx hash (no network, no signature)", async () => {
    const a = await simulateTradeCommit({ listing: LISTING });
    const b = await simulateTradeCommit({ listing: LISTING });
    expect(a.txHash).toMatch(/^0x[0-9a-f]{64}$/);
    expect(a.txHash).toBe(b.txHash);
  });
});
