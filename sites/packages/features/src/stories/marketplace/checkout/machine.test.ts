import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";

import {
  checkoutMachine,
  CHECKOUT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  slugToState,
  stateToSlug,
  resolveCheckoutSnapshot,
  simulateFulfill,
  type FulfillFn,
  type FulfillResult,
  type TrackFn,
} from "./machine";
import type { TrackContext } from "@core/lib/telemetry/track";

const TRACK_CTX: TrackContext = { sid: "sid-test", story: "marketplace-checkout" };

function inputFor(run: FulfillFn, track: TrackFn) {
  return {
    totalCredits: "155",
    idempotencyKey: "idem-test-1",
    trackCtx: TRACK_CTX,
    run,
    track,
  };
}

const okRun: FulfillFn = async () => ({
  checkoutId: 42,
  status: "fulfilled",
  phase: "done",
});
const failRun: FulfillFn = async () => ({
  checkoutId: 43,
  status: "refunded",
  phase: "failed",
});
const pendingRun: FulfillFn = async () => ({
  checkoutId: 44,
  status: "fulfilling",
  phase: "pending",
});
const throwRun: FulfillFn = async () => {
  throw new Error("auth chain: Invalid Auth Chain");
};

describe("checkout machine", () => {
  it("slug mappings are bijective", () => {
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("garbage")).toBe("review");
    expect(FIRST_STEP_SLUG).toBe("review");
  });

  it("happy path review -> fulfilling -> done (one-step confirm)", async () => {
    const track = vi.fn();
    const actor = createActor(checkoutMachine, { input: inputFor(okRun, track) });
    actor.start();

    expect(actor.getSnapshot().value).toBe("review");
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.status === "done");
    const snap = actor.getSnapshot();
    expect(snap.value).toBe("done");
    expect(snap.context.result?.checkoutId).toBe(42);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CHECKOUT_EVENTS.started);
    expect(events).toContain(CHECKOUT_EVENTS.confirmReached);
    expect(events).toContain(CHECKOUT_EVENTS.succeeded);
  });

  it("failed terminal status routes to failed and can RETRY", async () => {
    const track = vi.fn();
    const actor = createActor(checkoutMachine, { input: inputFor(failRun, track) });
    actor.start();
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.value === "failed");
    expect(actor.getSnapshot().context.result?.status).toBe("refunded");
    expect(track.mock.calls.map((c) => c[0])).toContain(CHECKOUT_EVENTS.failed);

    actor.send({ type: "RETRY" });
    expect(actor.getSnapshot().value).toBe("fulfilling");
  });

  it("poll-timeout (phase pending) routes to processing, not failed", async () => {
    const track = vi.fn();
    const actor = createActor(checkoutMachine, {
      input: inputFor(pendingRun, track),
    });
    actor.start();
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.value === "processing");
    const snap = actor.getSnapshot();
    expect(snap.value).toBe("processing");
    expect(snap.context.result?.status).toBe("fulfilling");
    expect(snap.context.result?.checkoutId).toBe(44);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CHECKOUT_EVENTS.processing);
    expect(events).not.toContain(CHECKOUT_EVENTS.failed);

    actor.send({ type: "RETRY" });
    expect(actor.getSnapshot().value).toBe("processing");
  });

  it("thrown fulfilment error routes to failed with the message", async () => {
    const track = vi.fn();
    const actor = createActor(checkoutMachine, { input: inputFor(throwRun, track) });
    actor.start();
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.value === "failed");
    expect(actor.getSnapshot().context.error).toMatch(/Invalid Auth Chain/);
  });

  it("resolveCheckoutSnapshot returns undefined for review and a snapshot otherwise", () => {
    expect(
      resolveCheckoutSnapshot({
        step: "review",
        totalCredits: "10",
        idempotencyKey: "k",
        trackCtx: TRACK_CTX,
      }),
    ).toBeUndefined();
    const snap = resolveCheckoutSnapshot({
      step: "fulfilling",
      totalCredits: "10",
      idempotencyKey: "k",
      trackCtx: TRACK_CTX,
    });
    expect(snap).toBeDefined();
  });

  it("simulateFulfill resolves done", async () => {
    const r: FulfillResult = await simulateFulfill({ idempotencyKey: "k" });
    expect(r.phase).toBe("done");
  });
});
