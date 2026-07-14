import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  delegateMachine,
  DELEGATE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveDelegateSnapshot,
  slugToState,
  stateToSlug,
  type DelegateFn,
  type TrackFn,
} from "./machine";
import {
  failClosedDelegate,
  type DelegateReceipt,
} from "@data/lib/catalyst/governance/delegate-vp";

const RECEIPT: DelegateReceipt = {
  space: "snapshot.dcl.eth",
  delegate: "0xabc",
  vp: 12480,
  txHash: "0xdeadbeef",
  chainId: 1,
  status: "confirmed",
  blockNumber: 21_500_000,
};

const okDelegate: DelegateFn = async () => RECEIPT;

const CANDIDATE = {
  id: "metahero",
  address: "0x7c4f9b2e6d1a8c3f0b5e2d9a4c7f1e6b3a8d2c4e",
  name: "metahero.dcl",
};

function inputFor(delegate: DelegateFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-delegate-vp",
      variant: "wizard",
      experimentKey: "gv_delegate_wizard",
    },
    space: "snapshot.dcl.eth",
    vp: 12480,
    delegate,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "browsing",
  "candidate",
  "confirming",
  "signing",
  "done",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "PICK_CANDIDATE" as const, ...CANDIDATE },
  { type: "CONFIRM" as const },
  { type: "SIGN" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("delegateMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(delegateMachine.states));
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

  it("matches the spec ?step slugs", () => {
    expect(STATE_TO_SLUG.browsing).toBe("browse");
    expect(STATE_TO_SLUG.candidate).toBe("candidate");
    expect(STATE_TO_SLUG.confirming).toBe("confirm");
    expect(STATE_TO_SLUG.signing).toBe("signing");
    expect(STATE_TO_SLUG.done).toBe("done");
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.browsing);
    expect(slugToState(null)).toBe("browsing");
    expect(slugToState(undefined)).toBe("browsing");
    expect(slugToState("")).toBe("browsing");
    expect(slugToState("nope")).toBe("browsing");
    expect(slugToState("candidate")).toBe("candidate");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("signing")).toBe("signing");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("delegateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveDelegateSnapshot({
      step: "browsing",
      trackCtx: inputFor(okDelegate, () => {}).trackCtx,
      space: "snapshot.dcl.eth",
      vp: 12480,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating signing does NOT fire telemetry and does NOT auto-sign", async () => {
    const track = vi.fn();
    const delegate = vi.fn(okDelegate);
    const snapshot = resolveDelegateSnapshot({
      step: "signing",
      trackCtx: inputFor(delegate, track).trackCtx,
      space: "snapshot.dcl.eth",
      vp: 12480,
      delegate,
      track,
      candidate: CANDIDATE,
    });
    const actor = createActor(delegateMachine, {
      input: { ...inputFor(delegate, track), candidateId: CANDIDATE.id, candidateAddress: CANDIDATE.address, candidateName: CANDIDATE.name },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("signing")).toBe(true);
    expect(actor.getSnapshot().context.candidateId).toBe("metahero");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(delegate).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("signing")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveDelegateSnapshot({
      step: "candidate",
      trackCtx: inputFor(okDelegate, track).trackCtx,
      space: "snapshot.dcl.eth",
      vp: 12480,
      track,
      candidate: CANDIDATE,
    });
    const actor = createActor(delegateMachine, {
      input: { ...inputFor(okDelegate, track), candidateId: CANDIDATE.id, candidateAddress: CANDIDATE.address, candidateName: CANDIDATE.name },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("candidate")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      DELEGATE_EVENTS.confirmReached,
    );
  });
});

describe("delegateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(delegateMachine, {
      input: inputFor(okDelegate, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("candidate")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("signing")).toBe(true);
  });

  it("reaching signing passes through PICK_CANDIDATE, CONFIRM and SIGN", () => {
    const paths = getShortestPaths(delegateMachine, {
      input: inputFor(okDelegate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const signing = paths.find((p) => (p.state.value as string) === "signing");
    expect(signing).toBeDefined();
    const events = signing!.steps.map((s) => s.event.type);
    expect(events).toContain("PICK_CANDIDATE");
    expect(events).toContain("CONFIRM");
    expect(events).toContain("SIGN");
  });
});

describe("delegateMachine \u{2014} telemetry events (happy path)", () => {
  it("browse -> candidate -> confirm -> sign -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(delegateMachine, {
      input: inputFor(okDelegate, track),
    }).start();

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    expect(actor.getSnapshot().matches("candidate")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DELEGATE_EVENTS.started);
    expect(events).toContain(DELEGATE_EVENTS.candidateViewed);
    expect(events).toContain(DELEGATE_EVENTS.confirmReached);
    expect(events).toContain(DELEGATE_EVENTS.signing);
    expect(events).toContain(DELEGATE_EVENTS.completed);

    expect(events.indexOf(DELEGATE_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(DELEGATE_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find(
      (c) => c[0] === DELEGATE_EVENTS.started,
    );
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_delegate_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.receipt).toEqual(RECEIPT);
  });

  it("going back from candidate does not fire confirm and returns to browsing", () => {
    const track = vi.fn();
    const actor = createActor(delegateMachine, {
      input: inputFor(okDelegate, track),
    }).start();

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("browsing")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DELEGATE_EVENTS.started);
    expect(events).not.toContain(DELEGATE_EVENTS.confirmReached);
  });
});

describe("delegateMachine \u{2014} signature failure + retry", () => {
  it("sign error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const delegate: DelegateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("wallet rejected");
      return okDelegate(args);
    };

    const actor = createActor(delegateMachine, {
      input: inputFor(delegate, track),
    }).start();

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("wallet rejected");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DELEGATE_EVENTS.completed);
  });

  it("error -> BACK returns to confirming without completing", async () => {
    const track = vi.fn();
    const delegate: DelegateFn = async () => {
      throw new Error("nope");
    };
    const actor = createActor(delegateMachine, {
      input: inputFor(delegate, track),
    }).start();

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(
      DELEGATE_EVENTS.completed,
    );
  });
});

describe("failClosedDelegate", () => {
  it("fails closed instead of fabricating an ECDSA-shaped signature", async () => {
    await expect(
      failClosedDelegate({ space: "snapshot.dcl.eth", delegate: "0xabc", vp: 100 }),
    ).rejects.toThrow(
      "delegation unavailable: no wallet transaction path is wired for the Snapshot delegate registry",
    );
  });
});
