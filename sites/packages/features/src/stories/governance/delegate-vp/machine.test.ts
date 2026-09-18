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

function seededInput(delegate: DelegateFn, track: TrackFn) {
  return {
    ...inputFor(delegate, track),
    candidateId: CANDIDATE.id,
    candidateAddress: CANDIDATE.address,
    candidateName: CANDIDATE.name,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "PICK_CANDIDATE" as const, ...CANDIDATE },
  { type: "CONFIRM" as const },
  { type: "SIGN" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("delegateMachine \u{2014} URL ?step slug map", () => {
  it("covers every state with the spec slugs, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(delegateMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(STATE_TO_SLUG).toMatchObject({
      browsing: "browse",
      candidate: "candidate",
      confirming: "confirm",
      signing: "signing",
      done: "done",
    });

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.browsing);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("browsing");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("delegateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; signing hydrates without telemetry or auto-sign; a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okDelegate, () => {}).trackCtx;
    expect(
      resolveDelegateSnapshot({ step: "browsing", trackCtx, space: "snapshot.dcl.eth", vp: 12480 }),
    ).toBeUndefined();

    const track = vi.fn();
    const delegate = vi.fn(okDelegate);
    const signing = createActor(delegateMachine, {
      input: seededInput(delegate, track),
      snapshot: resolveDelegateSnapshot({
        step: "signing",
        trackCtx,
        space: "snapshot.dcl.eth",
        vp: 12480,
        delegate,
        track,
        candidate: CANDIDATE,
      }),
    }).start();
    expect(signing.getSnapshot().matches("signing")).toBe(true);
    expect(signing.getSnapshot().context.candidateId).toBe("metahero");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(delegate).not.toHaveBeenCalled();
    expect(signing.getSnapshot().matches("signing")).toBe(true);

    const candidate = createActor(delegateMachine, {
      input: seededInput(okDelegate, track),
      snapshot: resolveDelegateSnapshot({
        step: "candidate",
        trackCtx,
        space: "snapshot.dcl.eth",
        vp: 12480,
        track,
        candidate: CANDIDATE,
      }),
    }).start();
    expect(candidate.getSnapshot().matches("candidate")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    candidate.send({ type: "CONFIRM" });
    expect(candidate.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(DELEGATE_EVENTS.confirmReached);
  });
});

describe("delegateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach candidate, confirming and signing, and signing passes through PICK_CANDIDATE, CONFIRM and SIGN", () => {
    const paths = getShortestPaths(delegateMachine, {
      input: inputFor(okDelegate, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["candidate", "confirming", "signing"]) {
      expect(ends.has(s)).toBe(true);
    }

    const signing = paths.find((p) => (p.state.value as string) === "signing");
    expect(signing).toBeDefined();
    expect(signing!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["PICK_CANDIDATE", "CONFIRM", "SIGN"]),
    );
  });
});

describe("delegateMachine \u{2014} telemetry events (happy path)", () => {
  it("BACK from candidate returns to browsing without confirm; browse -> candidate -> confirm -> sign -> done then fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(delegateMachine, {
      input: inputFor(okDelegate, track),
    }).start();

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    expect(actor.getSnapshot().matches("candidate")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("browsing")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(DELEGATE_EVENTS.started);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(DELEGATE_EVENTS.confirmReached);

    actor.send({ type: "PICK_CANDIDATE", ...CANDIDATE });
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        DELEGATE_EVENTS.started,
        DELEGATE_EVENTS.candidateViewed,
        DELEGATE_EVENTS.confirmReached,
        DELEGATE_EVENTS.signing,
        DELEGATE_EVENTS.completed,
      ]),
    );
    expect(events.indexOf(DELEGATE_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(DELEGATE_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === DELEGATE_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_delegate_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.receipt).toEqual(RECEIPT);
  });
});

describe("delegateMachine \u{2014} signature failure + retry", () => {
  it("sign error -> BACK returns to confirming without completing; a second failure -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const delegate: DelegateFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("wallet rejected");
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

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(DELEGATE_EVENTS.completed);

    actor.send({ type: "SIGN" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(DELEGATE_EVENTS.completed);
    expect(calls).toBe(3);
  });
});

describe("failClosedDelegate", () => {
  it("fails closed instead of fabricating an ECDSA-shaped signature", async () => {
    await expect(
      failClosedDelegate({ space: "snapshot.dcl.eth", delegate: "0xabc", vp: 100 }),
    ).rejects.toThrow(/delegation unavailable/i);
  });
});
