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

const EXPECTED_STATES = new Set([
  "entering",
  "checking",
  "unavailable",
  "approving",
  "confirming",
  "submitting",
  "success",
  "error",
]);

describe("claimNameMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(claimNameMachine.states));
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

  it("the audit-spec step ids are all addressable slugs", () => {
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
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.entering);
    expect(slugToState(null)).toBe("entering");
    expect(slugToState(undefined)).toBe("entering");
    expect(slugToState("")).toBe("entering");
    expect(slugToState("nope")).toBe("entering");
    expect(slugToState("approve-mana")).toBe("approving");
    expect(slugToState("confirm-mint")).toBe("confirming");
    expect(slugToState("submit-tx")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("claimNameMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveClaimSnapshot({
      step: "entering",
      trackCtx: inputFor({ track: () => {} }).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submit-tx does NOT fire telemetry and does NOT auto-mint", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const snapshot = resolveClaimSnapshot({
      step: "submitting",
      trackCtx: inputFor({ track }).trackCtx,
      mint,
      track,
      name: "myWorld",
    });
    const actor = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("myWorld");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating confirm-mint does NOT re-fire the confirm_reached entry event", async () => {
    const track = vi.fn();
    const snapshot = resolveClaimSnapshot({
      step: "confirming",
      trackCtx: inputFor({ track }).trackCtx,
      track,
    });
    const actor = createActor(claimNameMachine, {
      input: inputFor({ track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM_MINT" });
    expect(track.mock.calls.map((c) => c[0])).toContain(CLAIM_EVENTS.submitted);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "myWorld" },
  { type: "APPROVE_MANA" as const },
  { type: "CONFIRM_MINT" as const },
  { type: "EDIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("claimNameMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("entering")).toBe(true);
    expect(ends.has("checking")).toBe(true);
  });

  it("reaching checking passes through SUBMIT_NAME", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    const checking = paths.find((p) => (p.state.value as string) === "checking");
    expect(checking).toBeDefined();
    const events = checking!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_NAME");
  });
});

describe("claimNameMachine \u{2014} telemetry events (happy path)", () => {
  it("enter -> check(available) -> approve -> confirm -> submit -> success fires the full funnel", async () => {
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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.started);
    expect(events).toContain(CLAIM_EVENTS.available);
    expect(events).toContain(CLAIM_EVENTS.manaApproved);
    expect(events).toContain(CLAIM_EVENTS.confirmReached);
    expect(events).toContain(CLAIM_EVENTS.submitted);
    expect(events).toContain(CLAIM_EVENTS.completed);

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
  });

  it("taken name -> unavailable fires mk_claim_name_unavailable and does not mint", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: takenCheck, mint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "buterin" });
    await waitFor(actor, (s) => s.matches("unavailable"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.started);
    expect(events).toContain(CLAIM_EVENTS.unavailable);
    expect(events).not.toContain(CLAIM_EVENTS.confirmReached);
    expect(mint).not.toHaveBeenCalled();

    actor.send({ type: "EDIT" });
    expect(actor.getSnapshot().matches("entering")).toBe(true);
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

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.completed);
  });
});

describe("simulated actors (no network/chain)", () => {
  it("makeSimulateCheck flips on the seeded taken set", async () => {
    const check = makeSimulateCheck(new Set(["buterin"]));
    expect((await check({ name: "buterin" })).available).toBe(false);
    expect((await check({ name: "myWorld" })).available).toBe(true);
  });

  it("simulateMint resolves a fake tx hash + tokenId keyed by name", async () => {
    const a = await simulateMint({ name: "myWorld" });
    expect(a.txHash).toMatch(/^0x[0-9a-f]+$/);
    expect(a.tokenId).toMatch(/^\d+$/);
  });
});
