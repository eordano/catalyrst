import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  bidMachine,
  BID_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBidSnapshot,
  slugToState,
  stateToSlug,
  simulateChain,
  type ChainFn,
  type TrackFn,
} from "./machine";

const okChain: ChainFn = async () => {};
const failChain: ChainFn = async () => {
  throw new Error("federation unreachable");
};

function inputFor(chain: ChainFn, track: TrackFn, manaBalance = 50000) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-bid",
      variant: "wizard",
      experimentKey: "marketplace_bid_wizard",
    },
    manaBalance,
    chain,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "asset",
  "setAmount",
  "setExpiration",
  "approveMana",
  "signing",
  "confirming",
  "success",
  "insufficient",
  "failed",
]);

const TRAVERSAL_EVENTS = [
  { type: "REVIEW" as const },
  { type: "SET_AMOUNT" as const, price: 1000 },
  { type: "SET_AMOUNT" as const, price: 999999 },
  { type: "SET_EXPIRATION" as const, expiration: "2026-07-20" },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("bidMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(bidMachine.states));
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

  it("the audit step slugs map to the right states", () => {
    expect(slugToState("asset")).toBe("asset");
    expect(slugToState("set-amount")).toBe("setAmount");
    expect(slugToState("set-expiration")).toBe("setExpiration");
    expect(slugToState("approve-mana")).toBe("approveMana");
    expect(slugToState("sign-bid")).toBe("signing");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("success")).toBe("success");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.asset);
    expect(slugToState(null)).toBe("asset");
    expect(slugToState(undefined)).toBe("asset");
    expect(slugToState("")).toBe("asset");
    expect(slugToState("nope")).toBe("asset");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveBidSnapshot({
      step: "asset",
      trackCtx: inputFor(okChain, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-run the chain", async () => {
    const track = vi.fn();
    const chain = vi.fn(okChain);
    const snapshot = resolveBidSnapshot({
      step: "signing",
      trackCtx: inputFor(chain, track).trackCtx,
      chain,
      track,
    });
    const actor = createActor(bidMachine, {
      input: inputFor(chain, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("signing")).toBe(true);
    expect(actor.getSnapshot().context.price).toBe(1000);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(chain).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("signing")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveBidSnapshot({
      step: "setAmount",
      trackCtx: inputFor(okChain, track).trackCtx,
      track,
    });
    const actor = createActor(bidMachine, {
      input: inputFor(okChain, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("setAmount")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SET_AMOUNT", price: 1000 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_EVENTS.amountSet);
  });
});

describe("bidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(bidMachine, {
      input: inputFor(okChain, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("setExpiration")).toBe(true);
    expect(ends.has("insufficient")).toBe(true);
    expect(ends.has("approveMana")).toBe(true);
  });

  it("reaching approveMana passes through REVIEW, SET_AMOUNT, SET_EXPIRATION", () => {
    const paths = getShortestPaths(bidMachine, {
      input: inputFor(okChain, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const approve = paths.find((p) => (p.state.value as string) === "approveMana");
    expect(approve).toBeDefined();
    const events = approve!.steps.map((s) => s.event.type);
    expect(events).toContain("REVIEW");
    expect(events).toContain("SET_AMOUNT");
    expect(events).toContain("SET_EXPIRATION");
  });
});

describe("bidMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> amount -> expiration -> approve -> sign -> confirm -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(okChain, track),
    }).start();

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("setAmount")).toBe(true);

    actor.send({ type: "SET_AMOUNT", price: 1000 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);

    actor.send({ type: "SET_EXPIRATION", expiration: "2026-07-20" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_EVENTS.started);
    expect(events).toContain(BID_EVENTS.amountSet);
    expect(events).toContain(BID_EVENTS.expirationSet);
    expect(events).toContain(BID_EVENTS.manaApproved);
    expect(events).toContain(BID_EVENTS.signReached);
    expect(events).toContain(BID_EVENTS.signed);
    expect(events).toContain(BID_EVENTS.confirmed);
    expect(events).toContain(BID_EVENTS.completed);

    expect(events.indexOf(BID_EVENTS.started)).toBeLessThan(
      events.indexOf(BID_EVENTS.signReached),
    );
    expect(events.indexOf(BID_EVENTS.signReached)).toBeLessThan(
      events.indexOf(BID_EVENTS.completed),
    );

    const amountCall = track.mock.calls.find((c) => c[0] === BID_EVENTS.amountSet);
    expect(amountCall?.[1]).toMatchObject({ price: 1000 });
    expect(amountCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "marketplace_bid_wizard",
      variant: "wizard",
    });
  });

  it("insufficient-MANA: a bid over balance branches to insufficient and never reaches sign", () => {
    const track = vi.fn();
    const chain = vi.fn(okChain);
    const actor = createActor(bidMachine, {
      input: inputFor(chain, track, 500),
    }).start();

    actor.send({ type: "REVIEW" });
    actor.send({ type: "SET_AMOUNT", price: 10000 });
    expect(actor.getSnapshot().matches("insufficient")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_EVENTS.insufficientMana);
    expect(events).not.toContain(BID_EVENTS.signReached);
    expect(chain).not.toHaveBeenCalled();

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("setAmount")).toBe(true);
    actor.send({ type: "SET_AMOUNT", price: 100 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
  });
});

describe("bidMachine \u{2014} chain failure + retry", () => {
  it("a failing approval -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const chain: ChainFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("federation unreachable");
      return okChain(args);
    };

    const actor = createActor(bidMachine, {
      input: inputFor(chain, track),
    }).start();

    actor.send({ type: "REVIEW" });
    actor.send({ type: "SET_AMOUNT", price: 1000 });
    actor.send({ type: "SET_EXPIRATION", expiration: "2026-07-20" });
    await waitFor(actor, (s) => s.matches("failed"));
    expect(actor.getSnapshot().context.error).toBe("federation unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_EVENTS.failed);
    expect(events).toContain(BID_EVENTS.completed);
  });
});

describe("simulateChain", () => {
  it("resolves for every phase (no network)", async () => {
    await expect(simulateChain({ phase: "approve" })).resolves.toBeUndefined();
    await expect(simulateChain({ phase: "sign" })).resolves.toBeUndefined();
    await expect(simulateChain({ phase: "place" })).resolves.toBeUndefined();
  });
});
