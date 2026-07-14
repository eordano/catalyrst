import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  communityJoinMachine,
  COMMUNITY_JOIN_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCommunityJoinSnapshot,
  slugToState,
  stateToSlug,
} from "./machine";
import type { CommitFn, CommitResult } from "@data/lib/catalyst/overlay/community-join";

const JOIN_RESULT: CommitResult = {
  ok: true,
  action: "join",
  communityId: "c-1",
  role: "member",
  signatureHash: "deadbeef",
  pending: false,
};
const REQUEST_RESULT: CommitResult = { ...JOIN_RESULT, action: "request", role: null, pending: true };

const okCommit: CommitFn = async ({ action, communityId }) =>
  action === "request"
    ? { ...REQUEST_RESULT, communityId }
    : { ...JOIN_RESULT, communityId };

function inputFor(commit: CommitFn, track: (...a: unknown[]) => void) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-community-join",
      variant: "guided",
      experimentKey: "cl_community_join",
    },
    commit,
    track: track as never,
  };
}

const EXPECTED_STATES = new Set([
  "browsing",
  "detail",
  "joining",
  "requesting",
  "confirming",
  "joined",
  "error",
]);

describe("communityJoinMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(communityJoinMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.browsing);
    expect(slugToState(null)).toBe("browsing");
    expect(slugToState(undefined)).toBe("browsing");
    expect(slugToState("")).toBe("browsing");
    expect(slugToState("nope")).toBe("browsing");
    expect(slugToState("join")).toBe("joining");
    expect(slugToState("request")).toBe("requesting");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("done")).toBe("joined");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("communityJoinMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCommunityJoinSnapshot({
      step: "browsing",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating ?step=confirm does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveCommunityJoinSnapshot({
      step: "confirming",
      trackCtx: inputFor(commit, track).trackCtx,
      commit,
      track,
      communityId: "c-1",
      action: "join",
    });
    const actor = createActor(communityJoinMachine, {
      input: { ...inputFor(commit, track), communityId: "c-1", action: "join" },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(actor.getSnapshot().context.communityId).toBe("c-1");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
  });

  it("hydrating ?step=request does NOT double-fire the request event on mount", async () => {
    const track = vi.fn();
    const snapshot = resolveCommunityJoinSnapshot({
      step: "requesting",
      trackCtx: inputFor(okCommit, track).trackCtx,
      track,
      communityId: "c-2",
      action: "request",
    });
    const actor = createActor(communityJoinMachine, {
      input: { ...inputFor(okCommit, track), communityId: "c-2", action: "request" },
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("requesting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("joined"));
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joined);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "SELECT" as const, communityId: "c-1", action: "join" as const },
  { type: "SELECT" as const, communityId: "c-2", action: "request" as const },
  { type: "START" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
  { type: "BROWSE_MORE" as const },
];

describe("communityJoinMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(communityJoinMachine, {
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
    expect(ends.has("joining")).toBe(true);
    expect(ends.has("requesting")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
  });

  it("reaching joining passes through SELECT(join) and START", () => {
    const paths = getShortestPaths(communityJoinMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const joining = paths.find((p) => (p.state.value as string) === "joining");
    expect(joining).toBeDefined();
    const events = joining!.steps.map((s) => s.event.type);
    expect(events).toContain("SELECT");
    expect(events).toContain("START");
  });
});

describe("communityJoinMachine \u{2014} public JOIN funnel", () => {
  it("select(public) -> start -> confirm -> joined fires the join funnel", async () => {
    const track = vi.fn();
    const actor = createActor(communityJoinMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "SELECT", communityId: "c-1", action: "join" });
    expect(actor.getSnapshot().matches("detail")).toBe(true);

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("joining")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("joined"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joinStarted);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joined);
    expect(events).not.toContain(COMMUNITY_JOIN_EVENTS.requestSubmitted);

    expect(events.indexOf(COMMUNITY_JOIN_EVENTS.joinStarted)).toBeLessThan(
      events.indexOf(COMMUNITY_JOIN_EVENTS.joined),
    );

    const snap = actor.getSnapshot();
    expect(snap.context.result?.role).toBe("member");
    expect(snap.context.result?.pending).toBe(false);

    const joinedCall = track.mock.calls.find((c) => c[0] === COMMUNITY_JOIN_EVENTS.joined);
    expect(joinedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_community_join",
      variant: "guided",
    });
    expect(joinedCall?.[1]).toMatchObject({ action: "join", pending: false, stub: true });
  });
});

describe("communityJoinMachine \u{2014} private REQUEST funnel", () => {
  it("select(private) -> start -> confirm -> joined submits a pending request", async () => {
    const track = vi.fn();
    const actor = createActor(communityJoinMachine, {
      input: inputFor(okCommit, track),
    }).start();

    actor.send({ type: "SELECT", communityId: "c-2", action: "request" });
    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("requesting")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("joined"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joinStarted);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.requestSubmitted);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joined);

    const snap = actor.getSnapshot();
    expect(snap.context.result?.pending).toBe(true);
    expect(snap.context.result?.role).toBeNull();
    const joinedCall = track.mock.calls.find((c) => c[0] === COMMUNITY_JOIN_EVENTS.joined);
    expect(joinedCall?.[1]).toMatchObject({ action: "request", pending: true, stub: true });
  });
});

describe("communityJoinMachine \u{2014} commit failure + retry", () => {
  it("commit error -> RETRY recovers to joined", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okCommit(args);
    };
    const actor = createActor(communityJoinMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "SELECT", communityId: "c-1", action: "join" });
    actor.send({ type: "START" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("joined"));
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COMMUNITY_JOIN_EVENTS.joined);
  });
});
