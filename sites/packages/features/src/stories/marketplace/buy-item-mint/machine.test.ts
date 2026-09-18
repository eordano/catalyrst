import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";

import {
  buyMintMachine,
  MINT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveMintSnapshot,
  slugToState,
  stateToSlug,
  type SimulateFn,
  type MintResult,
  type TrackFn,
  type MintAssetInput,
} from "./machine";

const RESULT: MintResult = { txHash: "0xtest" };

const ASSET: MintAssetInput = {
  id: "0xabc-0",
  name: "Test Wearable",
  priceMana: "50",
  tradeId: "trade-123",
};

const okPhase: SimulateFn = async () => RESULT;

function inputFor(simulate: SimulateFn, track: TrackFn) {
  return {
    asset: ASSET,
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-buy-item-mint",
      variant: "wizard",
      experimentKey: "mk_mint_wizard",
    },
    simulate,
    track,
  };
}

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("buyMintMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to review", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(buyMintMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("submit")).toBe("submitting");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("review");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("buyMintMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots review without a snapshot, hydrates later steps silently, and only real transitions track", async () => {
    const track = vi.fn();
    const simulate = vi.fn(okPhase);
    const trackCtx = inputFor(simulate, track).trackCtx;
    expect(resolveMintSnapshot({ step: "review", asset: ASSET, trackCtx })).toBeUndefined();

    const submitting = createActor(buyMintMachine, {
      input: inputFor(simulate, track),
      snapshot: resolveMintSnapshot({ step: "submitting", asset: ASSET, trackCtx, simulate, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.asset.id).toBe("0xabc-0");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(simulate).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const confirming = createActor(buyMintMachine, {
      input: inputFor(simulate, track),
      snapshot: resolveMintSnapshot({ step: "confirming", asset: ASSET, trackCtx, simulate, track }),
    }).start();
    expect(confirming.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    confirming.send({ type: "BACK" });
    expect(confirming.getSnapshot().matches("review")).toBe(true);
    expect(names(track)).not.toContain(MINT_EVENTS.submitted);
    expect(simulate).not.toHaveBeenCalled();
    confirming.send({ type: "START_MINT" });
    expect(names(track)).toContain(MINT_EVENTS.started);

    const errored = createActor(buyMintMachine, {
      input: inputFor(okPhase, vi.fn()),
      snapshot: resolveMintSnapshot({
        step: "error",
        asset: ASSET,
        trackCtx,
        simulate: okPhase,
        track: vi.fn(),
      }),
    }).start();
    expect(errored.getSnapshot().matches("error")).toBe(true);
    errored.send({ type: "RETRY" });
    expect(errored.getSnapshot().matches("submitting")).toBe(true);
  });
});

describe("buyMintMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> connect -> approve -> confirm -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(buyMintMachine, { input: inputFor(okPhase, track) }).start();

    actor.send({ type: "START_MINT" });
    await waitFor(actor, (s) => s.matches("confirming"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      MINT_EVENTS.started,
      MINT_EVENTS.reviewConfirmed,
      MINT_EVENTS.walletConnected,
      MINT_EVENTS.manaApproved,
      MINT_EVENTS.confirmReached,
      MINT_EVENTS.submitted,
      MINT_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(MINT_EVENTS.started)).toBeLessThan(
      events.indexOf(MINT_EVENTS.confirmReached),
    );
    expect(events.indexOf(MINT_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(MINT_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === MINT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "mk_mint_wizard",
      variant: "wizard",
    });
    const completedCall = track.mock.calls.find((c) => c[0] === MINT_EVENTS.completed);
    expect(completedCall?.[1]).toMatchObject({ stub: true, item_id: "0xabc-0" });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("buyMintMachine \u{2014} failure + retry", () => {
  it("RETRY re-runs only the failed phase: a connect error resumes at connect, a submit error resumes at submit", async () => {
    const track = vi.fn();
    const calls = { connect: 0, submit: 0 };
    const simulate: SimulateFn = async (args) => {
      if (args.phase === "connect") {
        calls.connect += 1;
        if (calls.connect === 1) throw new Error("wallet rejected");
      }
      if (args.phase === "submit") {
        calls.submit += 1;
        if (calls.submit === 1) throw new Error("mint reverted");
      }
      return RESULT;
    };
    const actor = createActor(buyMintMachine, { input: inputFor(simulate, track) }).start();

    actor.send({ type: "START_MINT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.failedPhase).toBe("connect");
    expect(actor.getSnapshot().context.error).toBe("wallet rejected");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("confirming"));
    expect(calls.connect).toBe(2);
    expect(calls.submit).toBe(0);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.failedPhase).toBe("submit");
    expect(actor.getSnapshot().context.error).toBe("mint reverted");
    const failedCalls = track.mock.calls.filter((c) => c[0] === MINT_EVENTS.failed);
    expect(failedCalls.map((c) => c[1])).toEqual([
      expect.objectContaining({ step: "connect" }),
      expect.objectContaining({ step: "submit" }),
    ]);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(calls.connect).toBe(2);
    expect(calls.submit).toBe(2);
    expect(names(track)).toContain(MINT_EVENTS.completed);
  });
});
