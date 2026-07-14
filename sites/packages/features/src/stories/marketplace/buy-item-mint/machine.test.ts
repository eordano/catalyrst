import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  buyMintMachine,
  MINT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveMintSnapshot,
  slugToState,
  stateToSlug,
  simulatePhase,
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

const EXPECTED_STATES = new Set([
  "review",
  "connecting",
  "approving",
  "confirming",
  "submitting",
  "success",
  "error",
]);

describe("buyMintMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(buyMintMachine.states));
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

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    expect(slugToState(null)).toBe("review");
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("")).toBe("review");
    expect(slugToState("nope")).toBe("review");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("submit")).toBe("submitting");
    expect(slugToState("success")).toBe("success");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("buyMintMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveMintSnapshot({
      step: "review",
      asset: ASSET,
      trackCtx: inputFor(okPhase, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-run a phase", async () => {
    const track = vi.fn();
    const simulate = vi.fn(okPhase);
    const snapshot = resolveMintSnapshot({
      step: "submitting",
      asset: ASSET,
      trackCtx: inputFor(simulate, track).trackCtx,
      simulate,
      track,
    });
    const actor = createActor(buyMintMachine, {
      input: inputFor(simulate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.asset.id).toBe("0xabc-0");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(simulate).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveMintSnapshot({
      step: "confirming",
      asset: ASSET,
      trackCtx: inputFor(okPhase, track).trackCtx,
      track,
    });
    const actor = createActor(buyMintMachine, {
      input: inputFor(okPhase, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "START_MINT" });
    expect(track.mock.calls.map((c) => c[0])).toContain(MINT_EVENTS.started);
  });

  it("deep-linked error step has a RETRY target (failedPhase seeded)", () => {
    const track = vi.fn();
    const snapshot = resolveMintSnapshot({
      step: "error",
      asset: ASSET,
      trackCtx: inputFor(okPhase, track).trackCtx,
      simulate: okPhase,
      track,
    });
    const actor = createActor(buyMintMachine, {
      input: inputFor(okPhase, track),
      snapshot,
    }).start();
    expect(actor.getSnapshot().matches("error")).toBe(true);
    actor.send({ type: "RETRY" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "START_MINT" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("buyMintMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(buyMintMachine, {
      input: inputFor(okPhase, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    for (const p of paths) {
      const value = p.state.value as string;
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
  });

  it("reaching confirming passes through START_MINT (via the simulated phases)", async () => {
    const track = vi.fn();
    const actor = createActor(buyMintMachine, { input: inputFor(okPhase, track) }).start();
    actor.send({ type: "START_MINT" });
    await waitFor(actor, (s) => s.matches("confirming"));
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MINT_EVENTS.started);
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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MINT_EVENTS.started);
    expect(events).toContain(MINT_EVENTS.reviewConfirmed);
    expect(events).toContain(MINT_EVENTS.walletConnected);
    expect(events).toContain(MINT_EVENTS.manaApproved);
    expect(events).toContain(MINT_EVENTS.confirmReached);
    expect(events).toContain(MINT_EVENTS.submitted);
    expect(events).toContain(MINT_EVENTS.completed);

    expect(events.indexOf(MINT_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(MINT_EVENTS.completed),
    );
    expect(events.indexOf(MINT_EVENTS.started)).toBeLessThan(
      events.indexOf(MINT_EVENTS.confirmReached),
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

  it("BACK from confirm returns to review and does NOT submit", () => {
    const track = vi.fn();
    const simulate = vi.fn(okPhase);
    const actor = createActor(buyMintMachine, { input: inputFor(simulate, track) }).start();

    const snapshot = resolveMintSnapshot({
      step: "confirming",
      asset: ASSET,
      trackCtx: inputFor(simulate, track).trackCtx,
      simulate,
      track,
    });
    const a2 = createActor(buyMintMachine, { input: inputFor(simulate, track), snapshot }).start();
    a2.send({ type: "BACK" });
    expect(a2.getSnapshot().matches("review")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(MINT_EVENTS.submitted);
    expect(simulate).not.toHaveBeenCalled();
  });
});

describe("buyMintMachine \u{2014} failure + retry", () => {
  it("submit error -> mk_mint_failed{step:submit} -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let submitCalls = 0;
    const simulate: SimulateFn = async (args) => {
      if (args.phase === "submit") {
        submitCalls += 1;
        if (submitCalls === 1) throw new Error("mint reverted");
      }
      return RESULT;
    };

    const actor = createActor(buyMintMachine, { input: inputFor(simulate, track) }).start();
    actor.send({ type: "START_MINT" });
    await waitFor(actor, (s) => s.matches("confirming"));
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));

    expect(actor.getSnapshot().context.failedPhase).toBe("submit");
    expect(actor.getSnapshot().context.error).toBe("mint reverted");
    const failedCall = track.mock.calls.find((c) => c[0] === MINT_EVENTS.failed);
    expect(failedCall?.[1]).toMatchObject({ step: "submit" });

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(MINT_EVENTS.completed);
  });

  it("connect error -> RETRY re-runs the connect phase (not submit)", async () => {
    const track = vi.fn();
    let connectCalls = 0;
    const simulate: SimulateFn = async (args) => {
      if (args.phase === "connect") {
        connectCalls += 1;
        if (connectCalls === 1) throw new Error("wallet rejected");
      }
      return RESULT;
    };
    const actor = createActor(buyMintMachine, { input: inputFor(simulate, track) }).start();
    actor.send({ type: "START_MINT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.failedPhase).toBe("connect");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("confirming"));
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
  });
});

describe("simulatePhase", () => {
  it("resolves a tx hash per phase (no network)", async () => {
    const connect = await simulatePhase({ phase: "connect", asset: ASSET });
    const submit = await simulatePhase({ phase: "submit", asset: ASSET });
    expect(connect.txHash).toContain("connect");
    expect(submit.txHash).toContain("submit");
  });
});
