import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  moderateMachine,
  MODERATE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveModerateSnapshot,
  slugToState,
  stateToSlug,
  defaultSuspend,
  type SuspendFn,
  type TrackFn,
} from "./machine";

const okSuspend: SuspendFn = async ({ communityId, decision }) => ({
  ok: true,
  id: communityId,
  suspended: decision === "suspend",
});
const failSuspend: SuspendFn = async () => {
  throw new Error("admin bearer required");
};

function inputFor(suspend: SuspendFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "admin-communities-moderation",
      variant: "moderation_console",
      experimentKey: "admin_communities_moderation",
    },
    suspend,
    track,
    total: 6,
  };
}

const EXPECTED_STATES = new Set([
  "authGate",
  "list",
  "reviewCommunity",
  "decision",
  "submitting",
  "moderated",
]);

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "SET_FILTER" as const, status: "suspended" as const, total: 1 },
  { type: "OPEN" as const, communityId: "c-1" },
  { type: "DECIDE" as const, decision: "suspend" as const, reason: "spam" },
  { type: "CLOSE" as const },
  { type: "CANCEL" as const },
  { type: "CONFIRM" as const },
  { type: "RETRY" as const },
  { type: "CONTINUE" as const },
];

describe("adminCommunitiesModerate \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states with unique round-tripping slugs, and unknown/missing ?step falls back to the first step", () => {
    const machineStates = new Set(Object.keys(moderateMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    expect(
      Object.entries(STATE_TO_SLUG).filter(
        ([state, slug]) => SLUG_TO_STATE[slug] !== state || stateToSlug(state) !== slug,
      ),
    ).toEqual([]);

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    for (const missing of [null, undefined, "", "nope"]) {
      expect(slugToState(missing)).toBe("authGate");
    }
    expect(slugToState("list")).toBe("list");
    expect(slugToState("review-community")).toBe("reviewCommunity");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("adminCommunitiesModerate \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot, and hydrating `submitting` fires NO telemetry and does NOT auto-commit", async () => {
    expect(
      resolveModerateSnapshot({
        step: "authGate",
        trackCtx: inputFor(okSuspend, () => {}).trackCtx,
      }),
    ).toBeUndefined();

    const track = vi.fn();
    const suspend = vi.fn(okSuspend);
    const snapshot = resolveModerateSnapshot({
      step: "submitting",
      trackCtx: inputFor(suspend, track).trackCtx,
      suspend,
      track,
      communityId: "c-1",
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(suspend, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.communityId).toBe("c-1");
    expect(actor.getSnapshot().context.decision).toBe("suspend");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(suspend).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating `list` does NOT double-fire list_viewed, and real transitions after hydration still fire telemetry", () => {
    const listTrack = vi.fn();
    const list = createActor(moderateMachine, {
      input: inputFor(okSuspend, listTrack),
      snapshot: resolveModerateSnapshot({
        step: "list",
        trackCtx: inputFor(okSuspend, listTrack).trackCtx,
        track: listTrack,
        total: 6,
      }),
    }).start();
    expect(list.getSnapshot().matches("list")).toBe(true);
    expect(listTrack).not.toHaveBeenCalled();

    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okSuspend, track),
      snapshot: resolveModerateSnapshot({
        step: "reviewCommunity",
        trackCtx: inputFor(okSuspend, track).trackCtx,
        track,
        communityId: "c-1",
      }),
    }).start();
    expect(actor.getSnapshot().matches("reviewCommunity")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "DECIDE", decision: "unsuspend" });
    expect(actor.getSnapshot().matches("decision")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.decisionSelected);
  });
});

describe("adminCommunitiesModerate \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state, and reaching submitting passes through SIGN_IN, OPEN, DECIDE and CONFIRM", () => {
    const paths = getShortestPaths(moderateMachine, {
      input: inputFor(okSuspend, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    expect([...ends].filter((value) => !EXPECTED_STATES.has(value))).toEqual([]);
    for (const state of ["list", "reviewCommunity", "decision", "submitting"]) {
      expect(ends.has(state), state).toBe(true);
    }

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    for (const type of ["SIGN_IN", "OPEN", "DECIDE", "CONFIRM"]) {
      expect(events, type).toContain(type);
    }
  });
});

describe("adminCommunitiesModerate \u{2014} telemetry events", () => {
  it("sign-in -> list (+ SET_FILTER re-emits list_viewed) -> open -> decide -> confirm -> commit fires the full funnel; an unsuspend commit reports suspended=false and has_reason=false", async () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okSuspend, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("list")).toBe(true);
    actor.send({ type: "SET_FILTER", status: "suspended", total: 1 });
    const listViewed = track.mock.calls.filter((c) => c[0] === MODERATE_EVENTS.listViewed);
    expect(listViewed.length).toBe(2);
    expect(listViewed[1][1]).toMatchObject({ status_filter: "suspended", total: 1 });

    actor.send({ type: "OPEN", communityId: "c-1" });
    actor.send({ type: "DECIDE", decision: "suspend", reason: "harassment reports" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));

    const events = track.mock.calls.map((c) => c[0]);
    for (const name of [
      MODERATE_EVENTS.gateViewed,
      MODERATE_EVENTS.authenticated,
      MODERATE_EVENTS.listViewed,
      MODERATE_EVENTS.reviewed,
      MODERATE_EVENTS.decisionSelected,
      MODERATE_EVENTS.committed,
    ]) {
      expect(events, name).toContain(name);
    }
    expect(events.indexOf(MODERATE_EVENTS.reviewed)).toBeLessThan(
      events.indexOf(MODERATE_EVENTS.committed),
    );

    const committed = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.committed);
    expect(committed?.[1]).toMatchObject({ community_id: "c-1", suspended: true, has_reason: true });
    expect(committed?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "admin_communities_moderation",
      variant: "moderation_console",
    });

    const unsuspendTrack = vi.fn();
    const unsuspend = createActor(moderateMachine, {
      input: inputFor(okSuspend, unsuspendTrack),
    }).start();
    unsuspend.send({ type: "SIGN_IN" });
    unsuspend.send({ type: "OPEN", communityId: "c-9" });
    unsuspend.send({ type: "DECIDE", decision: "unsuspend" });
    unsuspend.send({ type: "CONFIRM" });
    await waitFor(unsuspend, (s) => s.matches("moderated"));
    const unsuspended = unsuspendTrack.mock.calls.find((c) => c[0] === MODERATE_EVENTS.committed);
    expect(unsuspended?.[1]).toMatchObject({ community_id: "c-9", suspended: false, has_reason: false });
  });

  it("a commit error keeps the wizard on decision with moderation_failed carrying the community, and CONFIRM again recovers to moderated", async () => {
    const track = vi.fn();
    let calls = 0;
    const suspend: SuspendFn = async (args) => {
      calls += 1;
      if (calls === 1) return failSuspend(args);
      return okSuspend(args);
    };

    const actor = createActor(moderateMachine, {
      input: inputFor(suspend, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "OPEN", communityId: "c-1" });
    actor.send({ type: "DECIDE", decision: "suspend", reason: "spam" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("decision"));
    expect(actor.getSnapshot().context.error).toBe("admin bearer required");
    const failed = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.failed);
    expect(failed?.[1]).toMatchObject({ community_id: "c-1" });
    expect(track.mock.calls.map((c) => c[0])).not.toContain(MODERATE_EVENTS.committed);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.committed);
  });
});

describe("defaultSuspend (real moderation write via the resource-route action)", () => {
  it("POSTs the decision to the moderation action and returns the live result, and throws the backend error on a failed write (no fabricated success)", async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response(JSON.stringify({ ok: true, id: "c-1", suspended: true }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    try {
      const r = await defaultSuspend({ communityId: "c-1", decision: "suspend", reason: "x" });
      expect(r).toMatchObject({ ok: true, id: "c-1", suspended: true });
      expect(fetchMock).toHaveBeenCalledOnce();
      const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
      expect(String(url)).toContain("/admin/community-suspension");
      expect(init.method).toBe("POST");
      expect(JSON.parse(String(init.body))).toMatchObject({
        communityId: "c-1",
        decision: "suspend",
        reason: "x",
      });

      fetchMock.mockImplementation(
        async () =>
          new Response(JSON.stringify({ error: "admin bearer required" }), {
            status: 403,
            headers: { "content-type": "application/json" },
          }),
      );
      await expect(
        defaultSuspend({ communityId: "c-1", decision: "suspend" }),
      ).rejects.toThrow("admin bearer required");
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
