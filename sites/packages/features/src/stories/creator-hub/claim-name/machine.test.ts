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

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: "myWorld" },
  { type: "CONFIRM_MINT" as const },
  { type: "EDIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
  { type: "RETURN" as const },
];

describe("claimNameMachine \u{2014} URL ?step slug map", () => {
  it("covers every event-addressable state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(claimNameMachine.states));
    const mapped = new Set<string>([...Object.keys(STATE_TO_SLUG), "returned"]);
    expect(mapped).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.naming);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("naming");
    }
    expect(slugToState("availability")).toBe("checking");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("mint")).toBe("minting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("claimNameMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; review and mint hydrate without telemetry or auto-mint; real transitions still move", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);

    expect(
      resolveClaimSnapshot({ step: "naming", trackCtx: inputFor({ track }).trackCtx }),
    ).toBeUndefined();

    const reviewing = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot: resolveClaimSnapshot({
        step: "reviewing",
        trackCtx: inputFor({ track }).trackCtx,
        mint,
        track,
        name: "myWorld",
      }),
    }).start();
    expect(reviewing.getSnapshot().matches("reviewing")).toBe(true);
    expect(reviewing.getSnapshot().context.name).toBe("myWorld");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(mint).not.toHaveBeenCalled();
    expect(reviewing.getSnapshot().matches("reviewing")).toBe(true);

    const minting = createActor(claimNameMachine, {
      input: inputFor({ mint, track }),
      snapshot: resolveClaimSnapshot({
        step: "minting",
        trackCtx: inputFor({ track }).trackCtx,
        mint,
        track,
      }),
    }).start();
    await Promise.resolve();
    expect(mint).not.toHaveBeenCalled();
    expect(minting.getSnapshot().matches("minting")).toBe(true);

    expect(track).not.toHaveBeenCalled();
    reviewing.send({ type: "BACK" });
    expect(reviewing.getSnapshot().matches("naming")).toBe(true);
  });
});

describe("claimNameMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and checking needs SUBMIT_NAME", () => {
    const paths = getShortestPaths(claimNameMachine, {
      input: inputFor({ check: availableCheck, mint: okMint, track: () => {} }),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    expect(ends.has("naming")).toBe(true);
    expect(ends.has("checking")).toBe(true);

    const checking = paths.find((p) => (p.state.value as string) === "checking");
    expect(checking).toBeDefined();
    expect(checking!.steps.map((s) => s.event.type)).toContain("SUBMIT_NAME");
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
    expect(events).toEqual(
      expect.arrayContaining([
        CLAIM_EVENTS.started,
        CLAIM_EVENTS.available,
        CLAIM_EVENTS.reviewReached,
        CLAIM_EVENTS.mintSubmitted,
        CLAIM_EVENTS.completed,
        CLAIM_EVENTS.returned,
      ]),
    );
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

  it("a taken name (remote check or live taken set) lands on unavailable with no review and no mint; EDIT returns to naming", async () => {
    const track = vi.fn();
    const mint = vi.fn(okMint);
    const remote = createActor(claimNameMachine, {
      input: inputFor({ check: takenCheck, mint, track }),
    }).start();

    remote.send({ type: "SUBMIT_NAME", name: "decentraland" });
    await waitFor(remote, (s) => s.matches("unavailable"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CLAIM_EVENTS.unavailable);
    expect(events).not.toContain(CLAIM_EVENTS.reviewReached);
    expect(mint).not.toHaveBeenCalled();

    remote.send({ type: "EDIT" });
    expect(remote.getSnapshot().matches("naming")).toBe(true);

    const liveTrack = vi.fn();
    const live = createActor(claimNameMachine, {
      input: inputFor({ mint: okMint, track: liveTrack, takenNames: ["buterin"] }),
    }).start();
    live.send({ type: "SUBMIT_NAME", name: "Buterin" });
    await waitFor(live, (s) => s.matches("unavailable"));
    expect(liveTrack.mock.calls.map((c) => c[0])).toContain(CLAIM_EVENTS.unavailable);
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
  it("makeSimulateCheck flags seeded names taken and simulateMint resolves a fake tx hash + tokenId (no chain)", async () => {
    const check = makeSimulateCheck(new Set(["buterin"]));
    expect((await check({ name: "Buterin" })).available).toBe(false);
    expect((await check({ name: "myWorld" })).available).toBe(true);

    const res = await simulateMint({ name: "myWorld" });
    expect(res.txHash).toMatch(/^0x[0-9a-f]+$/);
    expect(res.tokenId).toMatch(/^\d+$/);
  });
});
