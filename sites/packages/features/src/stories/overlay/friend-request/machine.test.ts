import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  friendMachine,
  FRIEND_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveFriendSnapshot,
  slugToState,
  stateToSlug,
  parseAction,
  transitionValid,
  simulateUpsert,
  type UpsertFn,
  type UpsertResult,
  type TrackFn,
} from "./machine";

const ADDR = "0x6e51000000000000000000000000000000008b63";
const RESULT: UpsertResult = { action: "request", address: ADDR };

const okUpsert: UpsertFn = async ({ action, address }) => ({ action, address });
const failUpsert: UpsertFn = async () => {
  throw new Error("InvalidFriendshipAction");
};

function inputFor(upsert: UpsertFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-friend-request",
      variant: "wizard",
      experimentKey: "ov_friend_request",
    },
    upsert,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "panel",
  "confirming",
  "blockPrompt",
  "submitting",
  "done",
  "failed",
]);

describe("friendMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(friendMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.panel);
    expect(slugToState(null)).toBe("panel");
    expect(slugToState(undefined)).toBe("panel");
    expect(slugToState("")).toBe("panel");
    expect(slugToState("nope")).toBe("panel");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("block")).toBe("blockPrompt");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("parseAction bridges the spec ?action vocabulary to (step, action)", () => {
    expect(parseAction("add")).toEqual({ step: "confirming", action: "request" });
    expect(parseAction("accept")).toEqual({ step: "confirming", action: "accept" });
    expect(parseAction("cancel")).toEqual({ step: "confirming", action: "cancel" });
    expect(parseAction("reject")).toEqual({ step: "confirming", action: "reject" });
    expect(parseAction("block")).toEqual({ step: "blockPrompt", action: "block" });
    expect(parseAction("done")).toEqual({ step: "done" });
    expect(parseAction(null)).toEqual({ step: "panel" });
    expect(parseAction("garbage")).toEqual({ step: "panel" });
  });
});

describe("transitionValid \u{2014} mirrors catalyrst-social-rpc service.rs", () => {
  it("request is valid only from none/cancel/reject", () => {
    expect(transitionValid(undefined, "request")).toBe(true);
    expect(transitionValid("cancel", "request")).toBe(true);
    expect(transitionValid("reject", "request")).toBe(true);
    expect(transitionValid("request", "request")).toBe(false);
    expect(transitionValid("accept", "request")).toBe(false);
  });

  it("accept/cancel/reject are valid only from a pending request", () => {
    for (const to of ["accept", "cancel", "reject"] as const) {
      expect(transitionValid("request", to)).toBe(true);
      expect(transitionValid(undefined, to)).toBe(false);
      expect(transitionValid("accept", to)).toBe(false);
    }
  });

  it("block is valid from none/request/cancel/reject/accept", () => {
    for (const from of [undefined, "request", "cancel", "reject", "accept"] as const) {
      expect(transitionValid(from, "block")).toBe(true);
    }
  });
});

describe("friendMachine \u{2014} deep-link hydration (no telemetry double-fire)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveFriendSnapshot({
      step: "panel",
      trackCtx: inputFor(okUpsert, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-run the RPC", async () => {
    const track = vi.fn();
    const upsert = vi.fn(okUpsert);
    const snapshot = resolveFriendSnapshot({
      step: "submitting",
      trackCtx: inputFor(upsert, track).trackCtx,
      upsert,
      track,
      action: "request",
      address: ADDR,
    });
    const actor = createActor(friendMachine, {
      input: inputFor(upsert, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(upsert).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating a confirm step seeds action/address and fires no telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveFriendSnapshot({
      step: "confirming",
      trackCtx: inputFor(okUpsert, track).trackCtx,
      track,
      action: "accept",
      address: ADDR,
    });
    const actor = createActor(friendMachine, {
      input: inputFor(okUpsert, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(actor.getSnapshot().context.action).toBe("accept");
    expect(actor.getSnapshot().context.address).toBe(ADDR);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveFriendSnapshot({
      step: "blockPrompt",
      trackCtx: inputFor(okUpsert, track).trackCtx,
      track,
      address: ADDR,
    });
    const actor = createActor(friendMachine, {
      input: inputFor(okUpsert, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("blockPrompt")).toBe(true);
    expect(actor.getSnapshot().context.action).toBe("block");
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(FRIEND_EVENTS.blockConfirmed);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "START" as const, action: "request" as const, address: ADDR },
  { type: "START" as const, action: "block" as const, address: ADDR },
  { type: "CONFIRM" as const },
  { type: "CANCEL" as const },
  { type: "RETRY" as const },
];

describe("friendMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(friendMachine, {
      input: inputFor(okUpsert, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("blockPrompt")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("a block reaches submitting via blockPrompt and CONFIRM", () => {
    const paths = getShortestPaths(friendMachine, {
      input: inputFor(okUpsert, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const types = submitting!.steps.map((s) => s.event.type);
    expect(types).toContain("START");
    expect(types).toContain("CONFIRM");
  });
});

describe("friendMachine \u{2014} telemetry events", () => {
  it("request -> confirm -> submit -> done fires the funnel", async () => {
    const track = vi.fn();
    const actor = createActor(friendMachine, {
      input: inputFor(okUpsert, track),
    }).start();

    actor.send({ type: "START", action: "request", address: ADDR });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(FRIEND_EVENTS.actionStarted);
    expect(events).toContain(FRIEND_EVENTS.actionCompleted);
    expect(events.indexOf(FRIEND_EVENTS.actionStarted)).toBeLessThan(
      events.indexOf(FRIEND_EVENTS.actionCompleted),
    );

    const completed = track.mock.calls.find((c) => c[0] === FRIEND_EVENTS.actionCompleted);
    expect(completed?.[1]).toMatchObject({ action: "request", address: ADDR, stub: true });
    expect(completed?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ov_friend_request",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("block path fires block_prompt then block_confirmed (guardrail) before completed", async () => {
    const track = vi.fn();
    const actor = createActor(friendMachine, {
      input: inputFor(okUpsert, track),
    }).start();

    actor.send({ type: "START", action: "block", address: ADDR });
    expect(actor.getSnapshot().matches("blockPrompt")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(FRIEND_EVENTS.blockPrompt);
    expect(events).toContain(FRIEND_EVENTS.blockConfirmed);
    expect(events).toContain(FRIEND_EVENTS.actionCompleted);
    expect(events.indexOf(FRIEND_EVENTS.blockPrompt)).toBeLessThan(
      events.indexOf(FRIEND_EVENTS.blockConfirmed),
    );
    expect(events).not.toContain(FRIEND_EVENTS.actionStarted);
  });

  it("CANCEL from a confirm step returns to panel and does not submit", () => {
    const track = vi.fn();
    const upsert = vi.fn(okUpsert);
    const actor = createActor(friendMachine, {
      input: inputFor(upsert, track),
    }).start();

    actor.send({ type: "START", action: "accept", address: ADDR });
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("panel")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).not.toContain(FRIEND_EVENTS.actionCompleted);
    expect(upsert).not.toHaveBeenCalled();
  });

  it("a rejected (invalid) upsert -> failed -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const upsert: UpsertFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("InvalidFriendshipAction");
      return okUpsert(args);
    };
    const actor = createActor(friendMachine, {
      input: inputFor(upsert, track),
    }).start();

    actor.send({ type: "START", action: "request", address: ADDR });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("failed"));
    expect(actor.getSnapshot().context.error).toBe("InvalidFriendshipAction");

    const failEvents = track.mock.calls.map((c) => c[0]);
    expect(failEvents).toContain(FRIEND_EVENTS.actionFailed);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(FRIEND_EVENTS.actionCompleted);
  });
});

describe("simulateUpsert", () => {
  it("echoes the action+address without touching network", async () => {
    const r = await simulateUpsert({ action: "accept", address: ADDR });
    expect(r).toEqual({ action: "accept", address: ADDR });
  });
});
