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

const TRAVERSAL_EVENTS = [
  { type: "REVIEW" as const },
  { type: "SET_AMOUNT" as const, price: 1000 },
  { type: "SET_AMOUNT" as const, price: 999999 },
  { type: "SET_EXPIRATION" as const, expiration: "2026-07-20" },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("bidMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping audit slug and falls back to asset", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(bidMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState("set-amount")).toBe("setAmount");
    expect(slugToState("set-expiration")).toBe("setExpiration");
    expect(slugToState("approve-mana")).toBe("approveMana");
    expect(slugToState("sign-bid")).toBe("signing");
    expect(slugToState("confirm")).toBe("confirming");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.asset);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("asset");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots asset without a snapshot, hydrates signing silently, and only real transitions track", async () => {
    const track = vi.fn();
    const chain = vi.fn(okChain);
    const trackCtx = inputFor(chain, track).trackCtx;
    expect(resolveBidSnapshot({ step: "asset", trackCtx })).toBeUndefined();

    const signing = createActor(bidMachine, {
      input: inputFor(chain, track),
      snapshot: resolveBidSnapshot({ step: "signing", trackCtx, chain, track, price: 1234 }),
    }).start();
    expect(signing.getSnapshot().matches("signing")).toBe(true);
    expect(signing.getSnapshot().context.price).toBe(1234);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(chain).not.toHaveBeenCalled();
    expect(signing.getSnapshot().matches("signing")).toBe(true);

    const amount = createActor(bidMachine, {
      input: inputFor(okChain, track),
      snapshot: resolveBidSnapshot({ step: "setAmount", trackCtx, track }),
    }).start();
    expect(amount.getSnapshot().matches("setAmount")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    amount.send({ type: "SET_AMOUNT", price: 1000 });
    expect(amount.getSnapshot().matches("setExpiration")).toBe(true);
    expect(names(track)).toContain(BID_EVENTS.amountSet);
  });
});

describe("bidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach setExpiration, insufficient and approveMana, and approveMana needs REVIEW, SET_AMOUNT, SET_EXPIRATION", () => {
    const paths = getShortestPaths(bidMachine, {
      input: inputFor(okChain, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["setExpiration", "insufficient", "approveMana"]) expect(ends.has(s)).toBe(true);
    const approve = paths.find((p) => (p.state.value as string) === "approveMana");
    const events = approve!.steps.map((s) => s.event.type);
    for (const e of ["REVIEW", "SET_AMOUNT", "SET_EXPIRATION"]) expect(events).toContain(e);
  });
});

describe("bidMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> amount -> expiration -> approve -> sign -> confirm -> success fires the full funnel over the simulated chain", async () => {
    await expect(simulateChain({ phase: "approve" })).resolves.toBeUndefined();
    await expect(simulateChain({ phase: "sign" })).resolves.toBeUndefined();
    await expect(simulateChain({ phase: "place" })).resolves.toBeUndefined();

    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(simulateChain, track),
    }).start();

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("setAmount")).toBe(true);
    actor.send({ type: "SET_AMOUNT", price: 1000 });
    expect(actor.getSnapshot().matches("setExpiration")).toBe(true);
    actor.send({ type: "SET_EXPIRATION", expiration: "2026-07-20" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      BID_EVENTS.started,
      BID_EVENTS.amountSet,
      BID_EVENTS.expirationSet,
      BID_EVENTS.manaApproved,
      BID_EVENTS.signReached,
      BID_EVENTS.signed,
      BID_EVENTS.confirmed,
      BID_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
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
    expect(names(track)).toContain(BID_EVENTS.insufficientMana);
    expect(names(track)).not.toContain(BID_EVENTS.signReached);
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
    expect(names(track)).toContain(BID_EVENTS.failed);
    expect(names(track)).toContain(BID_EVENTS.completed);
  });
});
