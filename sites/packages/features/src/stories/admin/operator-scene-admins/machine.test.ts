import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  manageMachine,
  MANAGE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveManageSnapshot,
  slugToState,
  stateToSlug,
  simulateCommit,
  type CommitFn,
  type CommitResult,
  type TrackFn,
} from "./machine";

const OK: CommitResult = { ok: true };
const PLACE = "830d885b-52f3-4c91-9151-9c8ec40aab63";
const ADDR = "0x7e4b21d9f0a3c65e8b1d72f04a6c98e3b5d710a2";

const okCommit: CommitFn = async () => OK;

function inputFor(commit: CommitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "operator-scene-admins",
      variant: "wizard",
      experimentKey: "operator_scene_admins_wizard",
    },
    commit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "pickPlace",
  "admins",
  "grantOrRevoke",
  "confirm",
  "submitting",
  "done",
]);

const TRAVERSAL_EVENTS = [
  { type: "SELECT_PLACE" as const, placeId: PLACE },
  { type: "START_GRANT" as const, address: ADDR },
  { type: "START_REVOKE" as const, address: ADDR, canBeRemoved: true },
  { type: "REVIEW" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "DONE_BACK" as const },
  { type: "CHANGE_PLACE" as const },
];

describe("manageMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(manageMachine.states));
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
    expect(slugToState("admins")).toBe("admins");
    expect(slugToState("grant-or-revoke")).toBe("grantOrRevoke");
    expect(slugToState("confirm")).toBe("confirm");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("manageMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveManageSnapshot({
      step: "pickPlace",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveManageSnapshot({
      step: "submitting",
      trackCtx: inputFor(commit, track).trackCtx,
      placeId: PLACE,
      address: ADDR,
      commit,
      track,
    });
    const actor = createActor(manageMachine, {
      input: { ...inputFor(commit, track), placeId: PLACE },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.placeId).toBe(PLACE);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveManageSnapshot({
      step: "admins",
      trackCtx: inputFor(okCommit, track).trackCtx,
      placeId: PLACE,
      track,
    });
    const actor = createActor(manageMachine, {
      input: { ...inputFor(okCommit, track), placeId: PLACE },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("admins")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "START_GRANT", address: ADDR });
    expect(actor.getSnapshot().matches("grantOrRevoke")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MANAGE_EVENTS.grantStarted);
  });
});

describe("manageMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(manageMachine, {
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
    expect(ends.has("admins")).toBe(true);
    expect(ends.has("grantOrRevoke")).toBe(true);
    expect(ends.has("confirm")).toBe(true);
  });

  it("reaching confirm passes through SELECT_PLACE and an action start", () => {
    const paths = getShortestPaths(manageMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const confirm = paths.find((p) => (p.state.value as string) === "confirm");
    expect(confirm).toBeDefined();
    const events = confirm!.steps.map((s) => s.event.type);
    expect(events).toContain("SELECT_PLACE");
    expect(events.some((e) => e === "START_GRANT" || e === "START_REVOKE")).toBe(true);
    expect(events).toContain("REVIEW");
  });
});

describe("manageMachine \u{2014} add (grant) happy path", () => {
  it("pick -> grant -> review -> submit -> done fires grant_started + grant_committed", async () => {
    const track = vi.fn();
    const actor = createActor(manageMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "SELECT_PLACE", placeId: PLACE });
    expect(actor.getSnapshot().matches("admins")).toBe(true);

    actor.send({ type: "START_GRANT", address: ADDR });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("confirm")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MANAGE_EVENTS.grantStarted);
    expect(events).toContain(MANAGE_EVENTS.grantCommitted);
    expect(events).not.toContain(MANAGE_EVENTS.revokeCommitted);

    expect(events.indexOf(MANAGE_EVENTS.grantStarted)).toBeLessThan(
      events.indexOf(MANAGE_EVENTS.grantCommitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === MANAGE_EVENTS.grantStarted);
    expect(startedCall?.[1]).toMatchObject({ place_id: PLACE });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "operator_scene_admins_wizard",
      variant: "wizard",
    });
  });
});

describe("manageMachine \u{2014} revoke happy path", () => {
  it("revoke -> submit -> done fires revoke_committed with can_be_removed", async () => {
    const track = vi.fn();
    const actor = createActor(manageMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "SELECT_PLACE", placeId: PLACE });
    actor.send({ type: "START_REVOKE", address: ADDR, canBeRemoved: true });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MANAGE_EVENTS.revokeCommitted);
    expect(events).not.toContain(MANAGE_EVENTS.grantCommitted);

    const revokeCall = track.mock.calls.find((c) => c[0] === MANAGE_EVENTS.revokeCommitted);
    expect(revokeCall?.[1]).toMatchObject({ place_id: PLACE, can_be_removed: true });
  });
});

describe("manageMachine \u{2014} REVIEW guard (form readiness)", () => {
  it("an invalid add address cannot advance to confirm", () => {
    const track = vi.fn();
    const actor = createActor(manageMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "SELECT_PLACE", placeId: PLACE });
    actor.send({ type: "START_GRANT", address: "0xnot-an-address" });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("grantOrRevoke")).toBe(true);
  });
});

describe("manageMachine \u{2014} simulated commit failure", () => {
  it("commit error returns to confirm and fires action_failed", async () => {
    const track = vi.fn();
    const failCommit: CommitFn = async () => {
      throw new Error("403 forbidden (signed-fetch gated)");
    };
    const actor = createActor(manageMachine, {
      input: inputFor(failCommit, track),
    }).start();

    actor.send({ type: "SELECT_PLACE", placeId: PLACE });
    actor.send({ type: "START_GRANT", address: ADDR });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("confirm"));

    expect(actor.getSnapshot().context.error).toContain("403");
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MANAGE_EVENTS.actionFailed);
    expect(events).not.toContain(MANAGE_EVENTS.grantCommitted);

    const failCall = track.mock.calls.find((c) => c[0] === MANAGE_EVENTS.actionFailed);
    expect(failCall?.[1]).toMatchObject({ place_id: PLACE, action: "add" });
  });
});

describe("simulateCommit", () => {
  it("resolves ok (no network)", async () => {
    const r = await simulateCommit({ action: "add", placeId: PLACE, admin: ADDR });
    expect(r.ok).toBe(true);
  });
});
