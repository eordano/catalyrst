import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  rentMachine,
  RENT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveRentSnapshot,
  slugToState,
  stateToSlug,
  simulatePhase,
  type CommitPhaseFn,
  type SelectedPeriod,
  type TrackFn,
} from "./machine";

const PERIOD: SelectedPeriod = {
  index: 0,
  minDays: 1,
  maxDays: 6,
  pricePerDayMana: 100,
};

const okCommit: CommitPhaseFn = async () => {};
const failCommit: CommitPhaseFn = async () => {
  throw new Error("wallet rejected");
};

function inputFor(commit: CommitPhaseFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-rent-land",
      variant: "wizard",
      experimentKey: "marketplace_rent_wizard",
    },
    commit,
    track,
    rentalId: "rental-test-1",
    rentalContractAddress: "0x42f4ba48791e2de32f5fbf553441c2672864bb33",
  };
}

const EXPECTED_STATES = new Set([
  "review",
  "period",
  "price",
  "approve",
  "sign",
  "confirm",
  "success",
  "error",
]);

describe("rentMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(rentMachine.states));
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
      "review-land",
      "select-period",
      "set-price-or-accept",
      "approve-mana",
      "sign-rental",
      "confirm",
      "success",
      "error",
    ]);
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    expect(slugToState(null)).toBe("review");
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("")).toBe("review");
    expect(slugToState("nope")).toBe("review");
    expect(slugToState("select-period")).toBe("period");
    expect(slugToState("approve-mana")).toBe("approve");
    expect(slugToState("sign-rental")).toBe("sign");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("rentMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveRentSnapshot({
      step: "review",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveRentSnapshot({
      step: "sign",
      trackCtx: inputFor(commit, track).trackCtx,
      commit,
      track,
      period: PERIOD,
      days: 3,
    });
    const actor = createActor(rentMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("sign")).toBe(true);
    expect(actor.getSnapshot().context.period?.index).toBe(0);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("sign")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveRentSnapshot({
      step: "period",
      trackCtx: inputFor(okCommit, track).trackCtx,
      track,
    });
    const actor = createActor(rentMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("period")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    expect(actor.getSnapshot().matches("price")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(RENT_EVENTS.periodSelected);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "SELECT_PERIOD" as const, period: PERIOD },
  { type: "SET_DAYS" as const, days: 3 },
  { type: "ACCEPT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("rentMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(rentMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("period")).toBe(true);
    expect(ends.has("price")).toBe(true);
    expect(ends.has("approve")).toBe(true);
  });

  it("reaching approve passes through START, SELECT_PERIOD and ACCEPT", () => {
    const paths = getShortestPaths(rentMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const approve = paths.find((p) => (p.state.value as string) === "approve");
    expect(approve).toBeDefined();
    const events = approve!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events).toContain("SELECT_PERIOD");
    expect(events).toContain("ACCEPT");
  });
});

describe("rentMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> period -> price -> approve -> sign -> confirm -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(rentMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("period")).toBe(true);

    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    expect(actor.getSnapshot().matches("price")).toBe(true);

    actor.send({ type: "SET_DAYS", days: 4 });
    expect(actor.getSnapshot().context.days).toBe(4);
    actor.send({ type: "ACCEPT" });

    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RENT_EVENTS.started);
    expect(events).toContain(RENT_EVENTS.periodSelected);
    expect(events).toContain(RENT_EVENTS.priceSet);
    expect(events).toContain(RENT_EVENTS.manaApproved);
    expect(events).toContain(RENT_EVENTS.signReached);
    expect(events).toContain(RENT_EVENTS.signed);
    expect(events).toContain(RENT_EVENTS.completed);

    expect(events.indexOf(RENT_EVENTS.started)).toBeLessThan(
      events.indexOf(RENT_EVENTS.signReached),
    );
    expect(events.indexOf(RENT_EVENTS.signReached)).toBeLessThan(
      events.indexOf(RENT_EVENTS.completed),
    );

    expect(actor.getSnapshot().context.totalMana).toBe(400);

    const startedCall = track.mock.calls.find((c) => c[0] === RENT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "marketplace_rent_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result?.rentalId).toBe("rental-test-1");
    expect(actor.getSnapshot().context.result?.txHash).toContain("0xsimulated");
  });

  it("ACCEPT without SET_DAYS uses the period minimum and fires price_set", async () => {
    const track = vi.fn();
    const actor = createActor(rentMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("success"));

    expect(actor.getSnapshot().context.days).toBe(PERIOD.minDays);
    expect(actor.getSnapshot().context.totalMana).toBe(100);
    expect(track.mock.calls.map((c) => c[0])).toContain(RENT_EVENTS.priceSet);
  });

  it("SET_DAYS clamps to the selected period bounds", () => {
    const track = vi.fn();
    const actor = createActor(rentMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    actor.send({ type: "SET_DAYS", days: 999 });
    expect(actor.getSnapshot().context.days).toBe(PERIOD.maxDays);
    actor.send({ type: "SET_DAYS", days: -5 });
    expect(actor.getSnapshot().context.days).toBe(PERIOD.minDays);
  });

  it("abandoning the flow from review fires mk_rent_abandoned and does not start", () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const actor = createActor(rentMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RENT_EVENTS.abandoned);
    expect(events).not.toContain(RENT_EVENTS.started);
    expect(commit).not.toHaveBeenCalled();
  });
});

describe("rentMachine \u{2014} commit failure + retry", () => {
  it("commit error -> RETRY recovers to success and fires failed/retried", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitPhaseFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("wallet rejected");
      return okCommit(args);
    };

    const actor = createActor(rentMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("wallet rejected");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RENT_EVENTS.failed);
    expect(events).toContain(RENT_EVENTS.retried);
    expect(events).toContain(RENT_EVENTS.completed);
  });
});

describe("simulatePhase", () => {
  it("resolves for each phase (no network)", async () => {
    await expect(simulatePhase({ phase: "approve" })).resolves.toBeUndefined();
    await expect(simulatePhase({ phase: "sign" })).resolves.toBeUndefined();
    await expect(simulatePhase({ phase: "submit" })).resolves.toBeUndefined();
  });

  it("rejects when aborted", async () => {
    const ac = new AbortController();
    const p = simulatePhase({ phase: "approve", signal: ac.signal });
    ac.abort();
    await expect(p).rejects.toThrow("aborted");
  });
});
