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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("checkout machine", () => {
  it("slugs are bijective with a review fallback, review needs no snapshot, and simulateFulfill resolves done", async () => {
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("garbage")).toBe("review");
    expect(FIRST_STEP_SLUG).toBe("review");

    const base = { totalCredits: "10", idempotencyKey: "k", trackCtx: TRACK_CTX };
    expect(resolveCheckoutSnapshot({ step: "review", ...base })).toBeUndefined();
    expect(resolveCheckoutSnapshot({ step: "fulfilling", ...base })).toBeDefined();

    const r: FulfillResult = await simulateFulfill({ idempotencyKey: "k" });
    expect(r.phase).toBe("done");
  });

  it("happy path review -> fulfilling -> done (one-step confirm)", async () => {
    const track = vi.fn();
    const actor = createActor(checkoutMachine, { input: inputFor(okRun, track) }).start();
    expect(actor.getSnapshot().value).toBe("review");
    actor.send({ type: "CONFIRM" });

    await waitFor(actor, (s) => s.status === "done");
    const snap = actor.getSnapshot();
    expect(snap.value).toBe("done");
    expect(snap.context.result?.checkoutId).toBe(42);
    const events = names(track);
    expect(events).toContain(CHECKOUT_EVENTS.started);
    expect(events).toContain(CHECKOUT_EVENTS.confirmReached);
    expect(events).toContain(CHECKOUT_EVENTS.succeeded);
  });

  it("refunded and thrown fulfilments route to failed (RETRY re-runs); a pending poll-timeout routes to processing, not failed", async () => {
    const refundedTrack = vi.fn();
    const refunded = createActor(checkoutMachine, {
      input: inputFor(failRun, refundedTrack),
    }).start();
    refunded.send({ type: "CONFIRM" });
    await waitFor(refunded, (s) => s.value === "failed");
    expect(refunded.getSnapshot().context.result?.status).toBe("refunded");
    expect(names(refundedTrack)).toContain(CHECKOUT_EVENTS.failed);
    refunded.send({ type: "RETRY" });
    expect(refunded.getSnapshot().value).toBe("fulfilling");

    const thrown = createActor(checkoutMachine, { input: inputFor(throwRun, vi.fn()) }).start();
    thrown.send({ type: "CONFIRM" });
    await waitFor(thrown, (s) => s.value === "failed");
    expect(thrown.getSnapshot().context.error).toMatch(/Invalid Auth Chain/);

    const pendingTrack = vi.fn();
    const pending = createActor(checkoutMachine, {
      input: inputFor(pendingRun, pendingTrack),
    }).start();
    pending.send({ type: "CONFIRM" });
    await waitFor(pending, (s) => s.value === "processing");
    const snap = pending.getSnapshot();
    expect(snap.context.result?.status).toBe("fulfilling");
    expect(snap.context.result?.checkoutId).toBe(44);
    expect(names(pendingTrack)).toContain(CHECKOUT_EVENTS.processing);
    expect(names(pendingTrack)).not.toContain(CHECKOUT_EVENTS.failed);
    pending.send({ type: "RETRY" });
    expect(pending.getSnapshot().value).toBe("processing");
  });
});
