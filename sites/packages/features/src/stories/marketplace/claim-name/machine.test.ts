import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  claimNameMachine,
  CLAIM_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveClaimSnapshot,
  slugToState,
  stateToSlug,
  simulateMint,
  makeSimulateCheck,
  type CheckAvailabilityFn,
  type MintFn,
  type MintResult,
  type TrackFn,
} from "./machine";

const MINT: MintResult = { txHash: "0xabc", tokenId: "42" };

const availableCheck: CheckAvailabilityFn = async () => ({ available: true });
const takenCheck: CheckAvailabilityFn = async () => ({ available: false });
const okMint: MintFn = async () => MINT;

function inputFor(overrides: {
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  track: TrackFn;
}) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "marketplace-claim-name",
      variant: "wizard",
      experimentKey: "mk_claim_name_wizard",
    },
    takenNames: ["buterin", "decentraland"],
    check: overrides.check ?? availableCheck,
    mint: overrides.mint ?? okMint,
    track: overrides.track,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "myWorld" },
  { type: "APPROVE_MANA" as const },
  { type: "CONFIRM_MINT" as const },
  { type: "EDIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("claimNameMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping audit slug and falls back to entering", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(claimNameMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    for (const step of [
      "enter-name",
      "check-availability",
      "approve-mana",
      "confirm-mint",
      "submit-tx",
      "success",
    ]) {
      expect(SLUG_TO_STATE[step as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }
    expect(slugToState("approve-mana")).toBe("approving");
    expect(slugToState("confirm-mint")).toBe("confirming");
    expect(slugToState("submit-tx")).toBe("submitting");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.entering);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("entering");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("claimNameMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots entering without a snapshot, hydrates submit-tx/confirm-mint silently, and only real transitions track", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const trackCtx = inputFor({ track }).trackCtx;
    expect(resolveClaimSnapshot({ step: "entering", trackCtx })).toBeUndefined();

    const submitting = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot: resolveClaimSnapshot({ step: "submitting", trackCtx, mint, track, name: "myWorld" }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.name).toBe("myWorld");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const confirming = createActor(claimNameMachine, {
      input: inputFor({ track }),
      snapshot: resolveClaimSnapshot({ step: "confirming", trackCtx, track }),
    }).start();
    expect(confirming.getSnapshot().matches("confirming")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    confirming.send({ type: "CONFIRM_MINT" });
    expect(names(track)).toContain(CLAIM_EVENTS.submitted);
  });
});

describe("claimNameMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and checking needs SUBMIT_NAME", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    expect(ends.has("entering")).toBe(true);
    expect(ends.has("checking")).toBe(true);
    const checking = paths.find((p) => (p.state.value as string) === "checking");
    expect(checking!.steps.map((s) => s.event.type)).toContain("SUBMIT_NAME");
  });
});

describe("claimNameMachine \u{2014} telemetry events", () => {
  it("an available name runs enter -> check -> approve -> confirm -> submit -> success; a taken name is unavailable and never mints", async () => {
    const track = vi.fn();
    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint: okMint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "myWorld" });
    await waitFor(actor, (s) => s.matches("approving"));
    actor.send({ type: "APPROVE_MANA" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    actor.send({ type: "CONFIRM_MINT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      CLAIM_EVENTS.started,
      CLAIM_EVENTS.available,
      CLAIM_EVENTS.manaApproved,
      CLAIM_EVENTS.confirmReached,
      CLAIM_EVENTS.submitted,
      CLAIM_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(CLAIM_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(CLAIM_EVENTS.submitted),
    );
    expect(events.indexOf(CLAIM_EVENTS.submitted)).toBeLessThan(
      events.indexOf(CLAIM_EVENTS.completed),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === CLAIM_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ name: "myWorld" });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "mk_claim_name_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(MINT);

    const takenTrack = vi.fn();
    const mint = vi.fn(okMint);
    const taken = createActor(claimNameMachine, {
      input: inputFor({ check: takenCheck, mint, track: takenTrack }),
    }).start();
    taken.send({ type: "SUBMIT_NAME", name: "buterin" });
    await waitFor(taken, (s) => s.matches("unavailable"));
    expect(names(takenTrack)).toContain(CLAIM_EVENTS.started);
    expect(names(takenTrack)).toContain(CLAIM_EVENTS.unavailable);
    expect(names(takenTrack)).not.toContain(CLAIM_EVENTS.confirmReached);
    expect(mint).not.toHaveBeenCalled();
    taken.send({ type: "EDIT" });
    expect(taken.getSnapshot().matches("entering")).toBe(true);
  });
});

describe("claimNameMachine \u{2014} mint failure + retry", () => {
  it("mint error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const mint: MintFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("registrar reverted");
      return okMint(args);
    };
    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "myWorld" });
    await waitFor(actor, (s) => s.matches("approving"));
    actor.send({ type: "APPROVE_MANA" });
    actor.send({ type: "CONFIRM_MINT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("registrar reverted");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(CLAIM_EVENTS.completed);
  });
});

describe("simulated actors (no network/chain)", () => {
  it("makeSimulateCheck flips on the seeded taken set and simulateMint resolves a fake tx hash + tokenId", async () => {
    const check = makeSimulateCheck(new Set(["buterin"]));
    expect((await check({ name: "buterin" })).available).toBe(false);
    expect((await check({ name: "myWorld" })).available).toBe(true);
    const a = await simulateMint({ name: "myWorld" });
    expect(a.txHash).toMatch(/^0x[0-9a-f]+$/);
    expect(a.tokenId).toMatch(/^\d+$/);
  });
});


describe("live NAME steps", () => {
  it("waits for approval, preserves the quote, and retries approval without minting", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    let finish!: () => void;
    const approve = vi.fn().mockRejectedValueOnce(new Error("Approval rejected")).mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
    const actor = createActor(claimNameMachine, { input: {
      ...inputFor({ track, mint, check: async () => ({ available: true, priceMana: "123.5" }) }), approve,
    } }).start();
    actor.send({ type: "SUBMIT_NAME", name: "Example" });
    await waitFor(actor, state => state.matches("approving"));
    actor.send({ type: "APPROVE_MANA" });
    await waitFor(actor, state => state.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("Approval rejected");
    actor.send({ type: "RETRY" });
    expect(actor.getSnapshot().matches("approvalPending")).toBe(true);
    actor.send({ type: "CONFIRM_MINT" });
    expect(mint).not.toHaveBeenCalled();
    finish();
    await waitFor(actor, state => state.matches("confirming"));
    expect(approve.mock.calls[1][0]).toMatchObject({ name: "Example", priceMana: "123.5" });
    actor.send({ type: "CONFIRM_MINT" });
    await waitFor(actor, state => state.matches("success"));
    expect(mint.mock.calls[0][0]).toMatchObject({ name: "Example", priceMana: "123.5" });
    expect(track.mock.calls.find(call => call[0] === CLAIM_EVENTS.completed)?.[1]).toMatchObject({ stub: false });
    actor.stop();
  });
  it("treats availability outages as retryable errors rather than taken names", async () => {
    const check = vi.fn().mockRejectedValueOnce(new Error("RPC offline")).mockResolvedValueOnce({ available: true });
    const mint = vi.fn(okMint);
    const actor = createActor(claimNameMachine, { input: inputFor({ check, mint, track: vi.fn() }) }).start();
    actor.send({ type: "SUBMIT_NAME", name: "Example" });
    await waitFor(actor, state => state.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("RPC offline");
    actor.send({ type: "RETRY" });
    await waitFor(actor, state => state.matches("approving"));
    expect(check).toHaveBeenCalledTimes(2);
    expect(mint).not.toHaveBeenCalled();
    actor.stop();
  });
});


it("resumes a previously submitted registration without repeating approval or confirmation", async () => {
  const mint = vi.fn(okMint);
  const approve = vi.fn();
  const actor = createActor(claimNameMachine, { input: {
    ...inputFor({ track: vi.fn(), mint, check: async () => ({ available: true, pendingRegistration: true, priceMana: "100" }) }), approve,
  } }).start();
  actor.send({ type: "SUBMIT_NAME", name: "Example" });
  await waitFor(actor, state => state.matches("success"));
  expect(approve).not.toHaveBeenCalled();
  expect(mint).toHaveBeenCalledTimes(1);
  actor.stop();
});
