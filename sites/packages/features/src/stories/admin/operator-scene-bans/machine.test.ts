import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  sceneBanMachine,
  SCENE_BAN_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveSceneBanSnapshot,
  slugToState,
  stateToSlug,
  simulateCommit,
  type CommitFn,
  type CommitResult,
  type TrackFn,
} from "./machine";

const ADDR = "0x4a1f7c2e90b35d8146a0c7e29f51d83b06a9e417";
const PLACE = "11111111-1111-4111-8111-111111111111";

const okResult: CommitResult = { action: "ban", address: ADDR };
const okCommit: CommitFn = async ({ action, address }) => ({ action, address });
const failCommit: CommitFn = async () => {
  throw new Error("forbidden: not a scene owner or admin");
};

function inputFor(commit: CommitFn, track: TrackFn, total = 7) {
  return {
    trackCtx: {
      sid: "sid-op",
      story: "operator-scene-bans",
      variant: "list",
      experimentKey: "operator_scene_bans",
    },
    placeId: PLACE,
    total,
    commit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "pickPlace",
  "bans",
  "banOrUnban",
  "confirm",
  "submitting",
  "done",
]);

const TRAVERSAL_EVENTS = [
  { type: "PICK_PLACE" as const },
  { type: "START_BAN" as const, address: ADDR },
  { type: "START_UNBAN" as const, address: ADDR },
  { type: "REVIEW" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RESET" as const },
];

describe("sceneBanMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(sceneBanMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.pickPlace);
    expect(slugToState(null)).toBe("pickPlace");
    expect(slugToState(undefined)).toBe("pickPlace");
    expect(slugToState("")).toBe("pickPlace");
    expect(slugToState("nope")).toBe("pickPlace");
    expect(slugToState("bans")).toBe("bans");
    expect(slugToState("ban-or-unban")).toBe("banOrUnban");
    expect(slugToState("confirm")).toBe("confirm");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("sceneBanMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveSceneBanSnapshot({
      step: "pickPlace",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
      placeId: PLACE,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveSceneBanSnapshot({
      step: "submitting",
      trackCtx: inputFor(commit, track).trackCtx,
      placeId: PLACE,
      total: 7,
      commit,
      track,
      action: "ban",
      address: ADDR,
    });
    const actor = createActor(sceneBanMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.action).toBe("ban");
    expect(actor.getSnapshot().context.address).toBe(ADDR);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("deep-linking the bans step does NOT fire `viewed` until a real transition", () => {
    const track = vi.fn();
    const snapshot = resolveSceneBanSnapshot({
      step: "bans",
      trackCtx: inputFor(okCommit, track).trackCtx,
      placeId: PLACE,
      total: 7,
      track,
    });
    const actor = createActor(sceneBanMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("bans")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "START_BAN", address: ADDR });
    expect(actor.getSnapshot().matches("banOrUnban")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(SCENE_BAN_EVENTS.started);
  });
});

describe("sceneBanMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(sceneBanMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("bans")).toBe(true);
    expect(ends.has("banOrUnban")).toBe(true);
    expect(ends.has("confirm")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through PICK_PLACE, a START_*, REVIEW and SUBMIT", () => {
    const paths = getShortestPaths(sceneBanMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("PICK_PLACE");
    expect(events.some((e) => e === "START_BAN" || e === "START_UNBAN")).toBe(true);
    expect(events).toContain("REVIEW");
    expect(events).toContain("SUBMIT");
  });
});

describe("sceneBanMachine \u{2014} ban happy path", () => {
  it("pick -> view -> ban -> review -> submit -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(sceneBanMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "PICK_PLACE" });
    expect(actor.getSnapshot().matches("bans")).toBe(true);

    actor.send({ type: "START_BAN", address: ADDR });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SCENE_BAN_EVENTS.viewed);
    expect(events).toContain(SCENE_BAN_EVENTS.started);
    expect(events).toContain(SCENE_BAN_EVENTS.banCommitted);
    expect(events).not.toContain(SCENE_BAN_EVENTS.unbanCommitted);

    expect(events.indexOf(SCENE_BAN_EVENTS.started)).toBeLessThan(
      events.indexOf(SCENE_BAN_EVENTS.banCommitted),
    );

    const viewed = track.mock.calls.find((c) => c[0] === SCENE_BAN_EVENTS.viewed);
    expect(viewed?.[1]).toMatchObject({ place_id: PLACE, total: 7 });
    expect(viewed?.[2]).toMatchObject({
      sid: "sid-op",
      experimentKey: "operator_scene_bans",
      variant: "list",
    });
    expect(actor.getSnapshot().context.result).toEqual(okResult);
  });
});

describe("sceneBanMachine \u{2014} unban path", () => {
  it("unban commits operator_scene_unban_committed (not the ban event)", async () => {
    const track = vi.fn();
    const actor = createActor(sceneBanMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "PICK_PLACE" });
    actor.send({ type: "START_UNBAN", address: ADDR });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SCENE_BAN_EVENTS.unbanCommitted);
    expect(events).not.toContain(SCENE_BAN_EVENTS.banCommitted);
    expect(actor.getSnapshot().context.action).toBe("unban");
  });
});

describe("sceneBanMachine \u{2014} invalid target is gated", () => {
  it("REVIEW is blocked when the ban address is malformed", () => {
    const track = vi.fn();
    const actor = createActor(sceneBanMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "PICK_PLACE" });
    actor.send({ type: "START_BAN", address: "not-an-address" });
    expect(actor.getSnapshot().matches("banOrUnban")).toBe(true);

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("banOrUnban")).toBe(true);
  });
});

describe("sceneBanMachine \u{2014} commit failure + retry", () => {
  it("submit error returns to confirm, fires failed, and retry reaches done", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("forbidden: not a scene owner or admin");
      return okCommit(args);
    };
    const actor = createActor(sceneBanMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "PICK_PLACE" });
    actor.send({ type: "START_BAN", address: ADDR });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("confirm") && !!s.context.error);
    expect(actor.getSnapshot().context.error).toContain("forbidden");

    const afterFail = track.mock.calls.map((c) => c[0]);
    expect(afterFail).toContain(SCENE_BAN_EVENTS.failed);
    const failed = track.mock.calls.find((c) => c[0] === SCENE_BAN_EVENTS.failed);
    expect(failed?.[1]).toMatchObject({ place_id: PLACE, action: "ban" });

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(SCENE_BAN_EVENTS.banCommitted);
  });
});

describe("sceneBanMachine \u{2014} RESET returns to the (updated) list", () => {
  it("done -> RESET clears the action and re-fires viewed", async () => {
    const track = vi.fn();
    const actor = createActor(sceneBanMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "PICK_PLACE" });
    actor.send({ type: "START_BAN", address: ADDR });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    track.mockClear();
    actor.send({ type: "RESET" });
    expect(actor.getSnapshot().matches("bans")).toBe(true);
    expect(actor.getSnapshot().context.action).toBeUndefined();
    expect(actor.getSnapshot().context.address).toBeUndefined();
    expect(track.mock.calls.map((c) => c[0])).toContain(SCENE_BAN_EVENTS.viewed);
  });
});

describe("simulateCommit", () => {
  it("resolves the action + address (no network)", async () => {
    const r = await simulateCommit({ placeId: PLACE, action: "unban", address: ADDR });
    expect(r).toEqual({ action: "unban", address: ADDR });
  });
});
