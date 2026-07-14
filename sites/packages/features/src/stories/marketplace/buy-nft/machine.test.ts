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
const failSim: SimFn = async () => {
  throw new Error("auth chain: Invalid Auth Chain");
};

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

const EXPECTED_STATES = new Set([
  "review",
  "connecting",
  "approving",
  "confirming",
  "submitting",
  "success",
  "error",
]);

describe("buyMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(buyMachine.states));
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

  it("slugs match the SHARED SPEC step ids", () => {
    expect(STATE_TO_SLUG.review).toBe("review");
    expect(STATE_TO_SLUG.connecting).toBe("connect-wallet");
    expect(STATE_TO_SLUG.approving).toBe("approve-mana");
    expect(STATE_TO_SLUG.confirming).toBe("confirm-purchase");
    expect(STATE_TO_SLUG.submitting).toBe("submit-tx");
    expect(STATE_TO_SLUG.success).toBe("success");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    expect(slugToState(null)).toBe("review");
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("")).toBe("review");
    expect(slugToState("nope")).toBe("review");
    expect(slugToState("approve-mana")).toBe("approving");
    expect(slugToState("submit-tx")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("buyMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveBuySnapshot({
      step: "review",
      listing: LISTING,
      trackCtx: inputFor(okSim, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submit-tx fires NO telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okSim);
    const snapshot = resolveBuySnapshot({
      step: "submitting",
      listing: LISTING,
      trackCtx: inputFor(commit, track).trackCtx,
      commit,
      track,
    });
    const actor = createActor(buyMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating approve-mana does not auto-run and keeps the listing context", () => {
    const track = vi.fn();
    const snapshot = resolveBuySnapshot({
      step: "approving",
      listing: LISTING,
      trackCtx: inputFor(okSim, track).trackCtx,
      track,
    });
    const actor = createActor(buyMachine, {
      input: inputFor(okSim, track),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("approving")).toBe(true);
    expect(actor.getSnapshot().context.listing.assetId).toBe(LISTING.assetId);
    expect(track).not.toHaveBeenCalled();
  });

  it("a real transition from a hydrated confirming step still fires telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveBuySnapshot({
      step: "confirming",
      listing: LISTING,
      trackCtx: inputFor(okSim, track).trackCtx,
      track,
    });
    const actor = createActor(buyMachine, {
      input: inputFor(okSim, track),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BUY_EVENTS.confirmReached);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "CONFIRM" as const },
  { type: "CANCEL" as const },
  { type: "RETRY" as const },
];

describe("buyMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(buyMachine, {
      input: inputFor(okSim, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("review")).toBe(true);
    expect(ends.has("connecting")).toBe(true);
  });

  it("reaching connecting passes through START", () => {
    const paths = getShortestPaths(buyMachine, {
      input: inputFor(okSim, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const connecting = paths.find((p) => (p.state.value as string) === "connecting");
    expect(connecting).toBeDefined();
    expect(connecting!.steps.map((s) => s.event.type)).toContain("START");
  });
});

describe("buyMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> connect -> approve -> confirm -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(buyMachine, {
      input: inputFor(okSim, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("confirming"));

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BUY_EVENTS.started);
    expect(events).toContain(BUY_EVENTS.walletConnected);
    expect(events).toContain(BUY_EVENTS.manaApproved);
    expect(events).toContain(BUY_EVENTS.confirmReached);
    expect(events).toContain(BUY_EVENTS.completed);
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

  it("CANCEL at review does not start the funnel", () => {
    const track = vi.fn();
    const actor = createActor(buyMachine, {
      input: inputFor(okSim, track),
    }).start();
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });
});

describe("buyMachine \u{2014} failure + retry", () => {
  it("a failed sim phase routes to error and fires mk_buy_failed", async () => {
    const track = vi.fn();
    const actor = createActor(buyMachine, {
      input: inputFor(failSim, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("error"));

    expect(actor.getSnapshot().context.error).toBe("auth chain: Invalid Auth Chain");
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BUY_EVENTS.started);
    expect(events).toContain(BUY_EVENTS.failed);
    expect(events).not.toContain(BUY_EVENTS.completed);
  });

  it("RETRY after a transient failure recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const sim: SimFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("user rejected");
      return okSim(args);
    };
    const actor = createActor(buyMachine, {
      input: inputFor(sim, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("confirming"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    expect(track.mock.calls.map((c) => c[0])).toContain(BUY_EVENTS.completed);
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
