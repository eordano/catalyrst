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

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "SELECT_PERIOD" as const, period: PERIOD },
  { type: "SET_DAYS" as const, days: 3 },
  { type: "ACCEPT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("rentMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to review-land", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(rentMachine.states)));
    expect(STATE_TO_SLUG).toMatchObject({
      review: "review-land",
      period: "select-period",
      price: "set-price-or-accept",
      approve: "approve-mana",
      sign: "sign-rental",
      confirm: "confirm",
      success: "success",
    });
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

describe("rentMachine \u{2014} deep-link hydration", () => {
  it("boots review without a snapshot, hydrates sign silently, and only real transitions track", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const trackCtx = inputFor(commit, track).trackCtx;
    expect(resolveRentSnapshot({ step: "review", trackCtx })).toBeUndefined();

    const sign = createActor(rentMachine, {
      input: inputFor(commit, track),
      snapshot: resolveRentSnapshot({ step: "sign", trackCtx, commit, track, period: PERIOD, days: 3 }),
    }).start();
    expect(sign.getSnapshot().matches("sign")).toBe(true);
    expect(sign.getSnapshot().context.period?.index).toBe(0);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(sign.getSnapshot().matches("sign")).toBe(true);

    const period = createActor(rentMachine, {
      input: inputFor(okCommit, track),
      snapshot: resolveRentSnapshot({ step: "period", trackCtx, track }),
    }).start();
    expect(period.getSnapshot().matches("period")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    period.send({ type: "SELECT_PERIOD", period: PERIOD });
    expect(period.getSnapshot().matches("price")).toBe(true);
    expect(names(track)).toContain(RENT_EVENTS.periodSelected);
  });
});

describe("rentMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach period, price and approve, and approve needs START, SELECT_PERIOD, ACCEPT", () => {
    const paths = getShortestPaths(rentMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["period", "price", "approve"]) expect(ends.has(s)).toBe(true);
    const approve = paths.find((p) => (p.state.value as string) === "approve");
    const events = approve!.steps.map((s) => s.event.type);
    for (const e of ["START", "SELECT_PERIOD", "ACCEPT"]) expect(events).toContain(e);
  });
});

describe("rentMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> period -> price -> approve -> sign -> confirm -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(rentMachine, { input: inputFor(okCommit, track) }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("period")).toBe(true);
    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    expect(actor.getSnapshot().matches("price")).toBe(true);
    actor.send({ type: "SET_DAYS", days: 4 });
    expect(actor.getSnapshot().context.days).toBe(4);
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      RENT_EVENTS.started,
      RENT_EVENTS.periodSelected,
      RENT_EVENTS.priceSet,
      RENT_EVENTS.manaApproved,
      RENT_EVENTS.signReached,
      RENT_EVENTS.signed,
      RENT_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
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

  it("days default to the period minimum, clamp to its bounds, and BACK from review abandons without starting", async () => {
    const track = vi.fn();
    const defaulted = createActor(rentMachine, { input: inputFor(okCommit, track) }).start();
    defaulted.send({ type: "START" });
    defaulted.send({ type: "SELECT_PERIOD", period: PERIOD });
    defaulted.send({ type: "ACCEPT" });
    await waitFor(defaulted, (s) => s.matches("success"));
    expect(defaulted.getSnapshot().context.days).toBe(PERIOD.minDays);
    expect(defaulted.getSnapshot().context.totalMana).toBe(100);
    expect(names(track)).toContain(RENT_EVENTS.priceSet);

    const clamped = createActor(rentMachine, { input: inputFor(okCommit, vi.fn()) }).start();
    clamped.send({ type: "START" });
    clamped.send({ type: "SELECT_PERIOD", period: PERIOD });
    clamped.send({ type: "SET_DAYS", days: 999 });
    expect(clamped.getSnapshot().context.days).toBe(PERIOD.maxDays);
    clamped.send({ type: "SET_DAYS", days: -5 });
    expect(clamped.getSnapshot().context.days).toBe(PERIOD.minDays);

    const abandonTrack = vi.fn();
    const commit = vi.fn(okCommit);
    const abandoned = createActor(rentMachine, { input: inputFor(commit, abandonTrack) }).start();
    abandoned.send({ type: "BACK" });
    expect(abandoned.getSnapshot().matches("review")).toBe(true);
    expect(names(abandonTrack)).toContain(RENT_EVENTS.abandoned);
    expect(names(abandonTrack)).not.toContain(RENT_EVENTS.started);
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
    const actor = createActor(rentMachine, { input: inputFor(commit, track) }).start();

    actor.send({ type: "START" });
    actor.send({ type: "SELECT_PERIOD", period: PERIOD });
    actor.send({ type: "ACCEPT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("wallet rejected");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    const events = names(track);
    expect(events).toContain(RENT_EVENTS.failed);
    expect(events).toContain(RENT_EVENTS.retried);
    expect(events).toContain(RENT_EVENTS.completed);
  });
});

describe("simulatePhase", () => {
  it("resolves for each phase (no network) and rejects when aborted", async () => {
    await expect(simulatePhase({ phase: "approve" })).resolves.toBeUndefined();
    await expect(simulatePhase({ phase: "sign" })).resolves.toBeUndefined();
    await expect(simulatePhase({ phase: "submit" })).resolves.toBeUndefined();
    const ac = new AbortController();
    const p = simulatePhase({ phase: "approve", signal: ac.signal });
    ac.abort();
    await expect(p).rejects.toThrow("aborted");
  });
});
