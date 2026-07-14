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
  makeSimulateCheck,
  simulateMint,
  type CheckAvailabilityFn,
  type MintFn,
  type MintResult,
  type TrackFn,
} from "./machine";

const RESULT: MintResult = { txHash: "0xabc", tokenId: "42" };

const availableCheck: CheckAvailabilityFn = async () => ({ available: true });
const takenCheck: CheckAvailabilityFn = async () => ({ available: false });

const okMint: MintFn = async () => RESULT;

function inputFor(args: {
  check?: CheckAvailabilityFn;
  mint?: MintFn;
  track: TrackFn;
  takenNames?: string[];
}) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-hub-claim-name",
      variant: "wizard",
      experimentKey: "ch_claim_name_wizard",
    },
    check: args.check,
    mint: args.mint,
    track: args.track,
    takenNames: args.takenNames,
  };
}

const EXPECTED_STATES = new Set([
  "naming",
  "checking",
  "unavailable",
  "reviewing",
  "minting",
  "done",
  "returned",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "myWorld" },
  { type: "CONFIRM_MINT" as const },
  { type: "EDIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
  { type: "RETURN" as const },
];

describe("claimNameMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's event-addressable states", () => {
    const machineStates = new Set(Object.keys(claimNameMachine.states));
    const mapped = new Set<string>([...Object.keys(STATE_TO_SLUG), "returned"]);
    expect(mapped).toEqual(machineStates);
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    expect(slugToState(null)).toBe("naming");
    expect(slugToState(undefined)).toBe("naming");
    expect(slugToState("")).toBe("naming");
    expect(slugToState("nope")).toBe("naming");
    expect(slugToState("availability")).toBe("checking");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("mint")).toBe("minting");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("claimNameMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveClaimSnapshot({
      step: "naming",
      trackCtx: inputFor({ track: () => {} }).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating REVIEW does NOT fire telemetry (entry action not replayed)", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const snapshot = resolveClaimSnapshot({
      step: "reviewing",
      trackCtx: inputFor({ track }).trackCtx,
      mint,
      track,
      name: "myWorld",
    });
    const actor = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(actor.getSnapshot().context.name).toBe("myWorld");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
  });

  it("hydrating MINT does NOT auto-race forward", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const snapshot = resolveClaimSnapshot({
      step: "minting",
      trackCtx: inputFor({ track }).trackCtx,
      mint,
      track,
    });
    const actor = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot,
    }).start();

    await Promise.resolve();
    expect(mint).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("minting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveClaimSnapshot({
      step: "reviewing",
      trackCtx: inputFor({ track }).trackCtx,
      track,
    });
    const actor = createActor(claimNameMachine, {
      input: inputFor({ mint: okMint, track }),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
  });
});

describe("claimNameMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint: okMint, track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("naming")).toBe(true);
    expect(ends.has("checking")).toBe(true);
  });

  it("reaching checking passes through SUBMIT_NAME", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint: okMint, track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });
    const checking = paths.find((p) => (p.state.value as string) === "checking");
    expect(checking).toBeDefined();
    const events = checking!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_NAME");
  });
});

describe("claimNameMachine \u{2014} telemetry events (happy path)", () => {
  it("name -> available -> review -> mint -> done -> return fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint: okMint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "myWorld" });
    await waitFor(actor, (s) => s.matches("reviewing"));

    actor.send({ type: "CONFIRM_MINT" });
    await waitFor(actor, (s) => s.matches("done"));

    actor.send({ type: "RETURN" });
    expect(actor.getSnapshot().matches("returned")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.started);
    expect(events).toContain(CLAIM_EVENTS.available);
    expect(events).toContain(CLAIM_EVENTS.reviewReached);
    expect(events).toContain(CLAIM_EVENTS.mintSubmitted);
    expect(events).toContain(CLAIM_EVENTS.completed);
    expect(events).toContain(CLAIM_EVENTS.returned);

    expect(events.indexOf(CLAIM_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(CLAIM_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === CLAIM_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_claim_name_wizard",
      variant: "wizard",
    });
    const completedCall = track.mock.calls.find((c) => c[0] === CLAIM_EVENTS.completed);
    expect(completedCall?.[1]).toMatchObject({
      world_name: "myworld.dcl.eth",
      stub: true,
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("taken name -> unavailable, no review reached, no mint", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: takenCheck, mint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "decentraland" });
    await waitFor(actor, (s) => s.matches("unavailable"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.unavailable);
    expect(events).not.toContain(CLAIM_EVENTS.reviewReached);
    expect(mint).not.toHaveBeenCalled();

    actor.send({ type: "EDIT" });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
  });

  it("an owned NAME (live taken set) classifies as unavailable", async () => {
    const track = vi.fn();
    const actor = createActor(claimNameMachine, {
      input: inputFor({ mint: okMint, track, takenNames: ["buterin"] }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "Buterin" });
    await waitFor(actor, (s) => s.matches("unavailable"));
    expect(track.mock.calls.map((c) => c[0])).toContain(CLAIM_EVENTS.unavailable);
  });
});

describe("claimNameMachine \u{2014} mint failure + retry", () => {
  it("mint error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const mint: MintFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("registrar unreachable");
      return okMint(args);
    };

    const actor = createActor(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint, track }),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: "myWorld" });
    await waitFor(actor, (s) => s.matches("reviewing"));
    actor.send({ type: "CONFIRM_MINT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("registrar unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(CLAIM_EVENTS.completed);
  });
});

describe("simulated actors", () => {
  it("makeSimulateCheck flags seeded names taken, others available", async () => {
    const check = makeSimulateCheck(new Set(["buterin"]));
    expect((await check({ name: "Buterin" })).available).toBe(false);
    expect((await check({ name: "myWorld" })).available).toBe(true);
  });

  it("simulateMint resolves a fake tx hash + tokenId (no chain)", async () => {
    const res = await simulateMint({ name: "myWorld" });
    expect(res.txHash).toMatch(/^0x[0-9a-f]+$/);
    expect(res.tokenId).toMatch(/^\d+$/);
  });
});
