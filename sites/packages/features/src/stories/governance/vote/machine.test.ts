import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  voteMachine,
  VOTE_EVENTS,
  MAX_ERRORS_BEFORE_SNAPSHOT,
  type CastFn,
  type CastResult,
  type TrackFn,
} from "./machine";

const RECEIPT: CastResult = { receipt: "stub:p1:yes" };

const okCast: CastFn = async () => RECEIPT;
const failCast: CastFn = async () => {
  throw new Error("dApp cast failed");
};

function inputFor(guided: boolean, castVote: CastFn, track: TrackFn) {
  return {
    proposalId: "p1",
    choice: "Yes",
    totalVp: "12,480",
    trackCtx: {
      sid: "sid-xyz",
      story: "governance-vote",
      variant: guided ? "guided" : "control",
      experimentKey: "gv_vote_flow",
    },
    guided,
    castVote,
    track,
  };
}

const graphOpts = {
  serializeState: (s: { value: unknown }) => JSON.stringify(s.value),
};

describe("voteMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("control (guided:false): never visits reasoning or snapshotFallback", () => {
    const paths = getShortestPaths(voteMachine, {
      ...graphOpts,
      input: inputFor(false, okCast, () => {}),
    });

    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      const events = p.steps.map((s) => s.event.type);
      expect(events).not.toContain("REASON");
    }
    expect(ends.has("reasoning")).toBe(false);
    expect(ends.has("snapshotFallback")).toBe(false);
    expect(ends.has("registered") || ends.has("done")).toBe(true);
  });

  it("guided (guided:true): reaches reasoning and a completed terminal", () => {
    const paths = getShortestPaths(voteMachine, {
      ...graphOpts,
      input: inputFor(true, okCast, () => {}),
    });

    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);

    expect(ends.has("reasoning")).toBe(true);
    expect(ends.has("registered") || ends.has("done")).toBe(true);

    const registered = paths.find((p) => (p.state.value as string) === "registered");
    expect(registered).toBeDefined();
    const events = registered!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events).toContain("CAST");
  });
});

describe("voteMachine \u{2014} telemetry events (control)", () => {
  it("choosing -> casting -> registered fires started + completed (no reasoned)", async () => {
    const track = vi.fn();
    const actor = createActor(voteMachine, {
      input: inputFor(false, okCast, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("registered"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(VOTE_EVENTS.started);
    expect(events).toContain(VOTE_EVENTS.completed);
    expect(events).not.toContain(VOTE_EVENTS.reasoned);

    const startedCall = track.mock.calls.find((c) => c[0] === VOTE_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-xyz",
      experimentKey: "gv_vote_flow",
      variant: "control",
    });
    expect(actor.getSnapshot().context.receipt).toBe(RECEIPT.receipt);
  });

  it("SUBSCRIBE from registered terminates the flow (done)", async () => {
    const track = vi.fn();
    const actor = createActor(voteMachine, {
      input: inputFor(false, okCast, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("registered"));
    actor.send({ type: "SUBSCRIBE" });

    expect(actor.getSnapshot().matches("done")).toBe(true);
  });
});

describe("voteMachine \u{2014} telemetry events (guided)", () => {
  it("choosing -> reasoning -> casting -> registered fires started + reasoned + completed", async () => {
    const track = vi.fn();
    const actor = createActor(voteMachine, {
      input: inputFor(true, okCast, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("reasoning")).toBe(true);

    actor.send({ type: "REASON", reason: "I support this grant proposal." });
    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("registered"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual([
      VOTE_EVENTS.started,
      VOTE_EVENTS.reasoned,
      VOTE_EVENTS.completed,
    ]);
  });

  it("repeated cast failures route to snapshotFallback and fire snapshot_redirect", async () => {
    const track = vi.fn();
    const actor = createActor(voteMachine, {
      input: inputFor(true, failCast, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({ type: "CAST" });

    await waitFor(actor, (s) => s.matches("castError"), { timeout: 2000 });
    expect(actor.getSnapshot().context.attempts).toBe(1);

    while (!actor.getSnapshot().matches("snapshotFallback")) {
      actor.send({ type: "RETRY" });
      await waitFor(
        actor,
        (s) => s.matches("castError") || s.matches("snapshotFallback"),
        { timeout: 2000 },
      );
    }

    expect(actor.getSnapshot().context.attempts).toBeGreaterThanOrEqual(
      MAX_ERRORS_BEFORE_SNAPSHOT,
    );
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(VOTE_EVENTS.started);
    expect(events).toContain(VOTE_EVENTS.snapshotRedirect);
    expect(events).not.toContain(VOTE_EVENTS.completed);
  });

  it("CANCEL from reasoning returns to choosing without casting", () => {
    const track = vi.fn();
    const castVote = vi.fn(okCast);
    const actor = createActor(voteMachine, {
      input: inputFor(true, castVote, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("reasoning")).toBe(true);

    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);
    expect(castVote).not.toHaveBeenCalled();
  });
});
