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
const failCancel: CancelFn = async () => {
  throw new Error("catalyst unreachable");
};

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

describe("cancelMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(cancelMachine.states));
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

  it("the audit-spec step ids are the slugs", () => {
    expect(STATE_TO_SLUG.reviewing).toBe("review-listing");
    expect(STATE_TO_SLUG.connecting).toBe("connect-wallet");
    expect(STATE_TO_SLUG.confirming).toBe("confirm-cancel");
    expect(STATE_TO_SLUG.submitting).toBe("submit-tx");
    expect(STATE_TO_SLUG.success).toBe("success");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.reviewing);
    expect(slugToState(null)).toBe("reviewing");
    expect(slugToState(undefined)).toBe("reviewing");
    expect(slugToState("")).toBe("reviewing");
    expect(slugToState("nope")).toBe("reviewing");
    expect(slugToState("confirm-cancel")).toBe("confirming");
    expect(slugToState("submit-tx")).toBe("submitting");
    expect(slugToState("not-owner")).toBe("notOwner");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("cancelMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCancelSnapshot({
      step: "reviewing",
      trackCtx: inputFor(okCancel, () => {}).trackCtx,
      order: ORDER,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submit-tx does NOT fire telemetry and does NOT auto-cancel", async () => {
    const track = vi.fn();
    const cancel = vi.fn(okCancel);
    const snapshot = resolveCancelSnapshot({
      step: "submitting",
      trackCtx: inputFor(cancel, track).trackCtx,
      order: ORDER,
      cancel,
      track,
    });
    const actor = createActor(cancelMachine, {
      input: inputFor(cancel, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(cancel).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCancelSnapshot({
      step: "confirming",
      trackCtx: inputFor(okCancel, track).trackCtx,
      order: ORDER,
      track,
    });
    const actor = createActor(cancelMachine, {
      input: inputFor(okCancel, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(CANCEL_EVENTS.submitted);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "CONNECT_WALLET" as const },
  { type: "NOT_OWNER" as const },
  { type: "CONFIRM" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("cancelMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("notOwner")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching confirming passes through CONNECT_WALLET and CONFIRM", () => {
    const paths = getShortestPaths(cancelMachine, {
      input: inputFor(okCancel, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const confirming = paths.find((p) => (p.state.value as string) === "confirming");
    expect(confirming).toBeDefined();
    const events = confirming!.steps.map((s) => s.event.type);
    expect(events).toContain("CONNECT_WALLET");
    expect(events).toContain("CONFIRM");
  });
});

describe("cancelMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> connect -> confirm -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(cancelMachine, {
      input: inputFor(okCancel, track),
    }).start();

    actor.send({ type: "CONNECT_WALLET" });
    expect(actor.getSnapshot().matches("connecting")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CANCEL_EVENTS.started);
    expect(events).toContain(CANCEL_EVENTS.walletConnected);
    expect(events).toContain(CANCEL_EVENTS.confirmReached);
    expect(events).toContain(CANCEL_EVENTS.submitted);
    expect(events).toContain(CANCEL_EVENTS.completed);

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
  });

  it("guard path fires mk_cancel_not_owner and never cancels", () => {
    const track = vi.fn();
    const cancel = vi.fn(okCancel);
    const actor = createActor(cancelMachine, {
      input: inputFor(cancel, track, { ownership: "other" }),
    }).start();

    actor.send({ type: "NOT_OWNER" });
    expect(actor.getSnapshot().matches("notOwner")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CANCEL_EVENTS.notOwner);
    expect(events).not.toContain(CANCEL_EVENTS.started);
    expect(events).not.toContain(CANCEL_EVENTS.confirmReached);
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

    const actor = createActor(cancelMachine, {
      input: inputFor(cancel, track),
    }).start();

    actor.send({ type: "CONNECT_WALLET" });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
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
