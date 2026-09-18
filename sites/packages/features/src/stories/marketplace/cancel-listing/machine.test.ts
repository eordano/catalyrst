import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  cancelMachine,
  CANCEL_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCancelSnapshot,
  slugToState,
  stateToSlug,
  simulateCancel,
  type CancelFn,
  type CancelOrder,
  type CancelResult,
  type TrackFn,
} from "./machine";

const ORDER: CancelOrder = {
  orderId: "0x6ae4b880dad7bc413a256447d59eeac51ad8fa6225ea1e4722cb73c33fcc0cb1",
  owner: "0x00009dc8aac69accf38e87ab42a82a28be68f2a0",
  price: "1",
  name: "Listing #47828",
  network: "polygon",
};

const RESULT: CancelResult = {
  message: { order_signature_hash: ORDER.orderId, signed_at: 1700000000 },
  simulated: true,
};

const okCancel: CancelFn = async () => RESULT;

function inputFor(cancel: CancelFn, track: TrackFn, extra: Record<string, unknown> = {}) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-cancel-listing",
      variant: "wizard",
      experimentKey: "marketplace_cancel_wizard",
    },
    order: ORDER,
    cancel,
    track,
    ...extra,
  };
}

const EXPECTED_STATES = new Set([
  "reviewing",
  "connecting",
  "confirming",
  "submitting",
  "success",
  "notOwner",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "CONNECT_WALLET" as const },
  { type: "NOT_OWNER" as const },
  { type: "CONFIRM" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("cancelMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to review-listing", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(cancelMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(STATE_TO_SLUG).toMatchObject({
      reviewing: "review-listing",
      connecting: "connect-wallet",
      confirming: "confirm-cancel",
      submitting: "submit-tx",
      success: "success",
      notOwner: "not-owner",
    });
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.reviewing);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("reviewing");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("cancelMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots reviewing without a snapshot, hydrates submit-tx silently, and only real transitions track", async () => {
    const track = vi.fn();
    const cancel = vi.fn(okCancel);
    const trackCtx = inputFor(cancel, track).trackCtx;
    expect(resolveCancelSnapshot({ step: "reviewing", trackCtx, order: ORDER })).toBeUndefined();

    const submitting = createActor(cancelMachine, {
      input: inputFor(cancel, track),
      snapshot: resolveCancelSnapshot({ step: "submitting", trackCtx, order: ORDER, cancel, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(cancel).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const confirming = createActor(cancelMachine, {
      input: inputFor(okCancel, track),
      snapshot: resolveCancelSnapshot({ step: "confirming", trackCtx, order: ORDER, track }),
    }).start();
    expect(confirming.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    confirming.send({ type: "SUBMIT" });
    expect(confirming.getSnapshot().matches("submitting")).toBe(true);
    expect(names(track)).toContain(CANCEL_EVENTS.submitted);
  });
});

describe("cancelMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and confirming needs CONNECT_WALLET + CONFIRM", () => {
    const paths = getShortestPaths(cancelMachine, {
      input: inputFor(okCancel, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["notOwner", "confirming", "submitting"]) expect(ends.has(s)).toBe(true);
    const confirming = paths.find((p) => (p.state.value as string) === "confirming");
    const events = confirming!.steps.map((s) => s.event.type);
    expect(events).toContain("CONNECT_WALLET");
    expect(events).toContain("CONFIRM");
  });
});

describe("cancelMachine \u{2014} telemetry events", () => {
  it("review -> connect -> confirm -> submit -> success fires the full funnel; a non-owner never cancels", async () => {
    const track = vi.fn();
    const actor = createActor(cancelMachine, { input: inputFor(okCancel, track) }).start();

    actor.send({ type: "CONNECT_WALLET" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      CANCEL_EVENTS.started,
      CANCEL_EVENTS.walletConnected,
      CANCEL_EVENTS.confirmReached,
      CANCEL_EVENTS.submitted,
      CANCEL_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(CANCEL_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(CANCEL_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === CANCEL_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "marketplace_cancel_wizard",
      variant: "wizard",
    });
    const completedCall = track.mock.calls.find((c) => c[0] === CANCEL_EVENTS.completed);
    expect(completedCall?.[1]).toMatchObject({ stub: true, order_signature_hash: ORDER.orderId });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);

    const guardTrack = vi.fn();
    const cancel = vi.fn(okCancel);
    const other = createActor(cancelMachine, {
      input: inputFor(cancel, guardTrack, { ownership: "other" }),
    }).start();
    other.send({ type: "NOT_OWNER" });
    expect(other.getSnapshot().matches("notOwner")).toBe(true);
    expect(names(guardTrack)).toContain(CANCEL_EVENTS.notOwner);
    expect(names(guardTrack)).not.toContain(CANCEL_EVENTS.started);
    expect(names(guardTrack)).not.toContain(CANCEL_EVENTS.confirmReached);
    expect(cancel).not.toHaveBeenCalled();
  });
});

describe("cancelMachine \u{2014} cancel failure + retry", () => {
  it("submit error -> RETRY recovers to success, firing mk_cancel_failed then completed", async () => {
    const track = vi.fn();
    let calls = 0;
    const cancel: CancelFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okCancel(args);
    };
    const actor = createActor(cancelMachine, { input: inputFor(cancel, track) }).start();

    actor.send({ type: "CONNECT_WALLET" });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    const events = names(track);
    expect(events).toContain(CANCEL_EVENTS.failed);
    expect(events).toContain(CANCEL_EVENTS.completed);
    expect(events.indexOf(CANCEL_EVENTS.failed)).toBeLessThan(
      events.indexOf(CANCEL_EVENTS.completed),
    );
  });
});

describe("simulateCancel", () => {
  it("builds the OrderCancel message shape and never hits the network", async () => {
    const result = await simulateCancel({ order: ORDER });
    expect(result.simulated).toBe(true);
    expect(result.message.order_signature_hash).toBe(ORDER.orderId);
    expect(typeof result.message.signed_at).toBe("number");
  });
});
