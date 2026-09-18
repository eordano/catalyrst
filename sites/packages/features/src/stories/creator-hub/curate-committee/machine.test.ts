import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  curationMachine,
  CURATION_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCurationSnapshot,
  slugToState,
  stateToSlug,
  simulateAssign,
  simulateDecide,
  type AssignFn,
  type DecideFn,
  type TrackFn,
} from "./machine";

const YOU = "0x9f3c4d1e7a2188cf90b3a6e7c4d5f6a7b8c9d0e1";

const okAssign: AssignFn = async ({ id, body }) => ({
  id,
  assignee: body.assignee,
  simulated: true,
});
const okDecide: DecideFn = async ({ id, body, comment }) => ({
  id,
  status: body.status,
  updated: 1,
  simulated: true,
  ...(comment && comment.raw.trim()
    ? { comment: { postId: 1, link: "https://forum.decentraland.org/t/1", raw: comment.raw.trim() } }
    : {}),
});

function inputFor(assign: AssignFn, decide: DecideFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-curate-committee",
      variant: "comments",
      experimentKey: "bd_curation_comments",
    },
    count: 7,
    youAddress: YOU,
    assign,
    decide,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "dashboard",
  "assigning",
  "reviewing",
  "commenting",
  "deciding",
  "decided",
]);

const TRAVERSAL_EVENTS = [
  { type: "FILTER" as const, filters: { status: "to_review" as const, type: "ALL_TYPES" as const, assignee: "all" } },
  { type: "ASSIGN" as const, id: "0xcol1" },
  { type: "OPEN_REVIEW" as const, id: "0xcol1", topicId: 50121 },
  { type: "DRAFT_DECISION" as const, status: "approved" as const },
  { type: "SUBMIT" as const, comment: "looks good" },
  { type: "BACK" as const },
];

describe("curateCommittee \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(curationMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.dashboard);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("dashboard");
    }
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("comment")).toBe("commenting");
    expect(slugToState("decide")).toBe("deciding");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("curateCommittee \u{2014} deep-link hydration", () => {
  it("first step boots from initial; commenting hydrates its draft without telemetry or auto-decide; real transitions still move", async () => {
    const track = vi.fn();
    const decide = vi.fn(okDecide);
    const input = inputFor(okAssign, decide, track);

    expect(
      resolveCurationSnapshot({ step: "dashboard", trackCtx: input.trackCtx, count: 7, youAddress: YOU }),
    ).toBeUndefined();

    const commenting = createActor(curationMachine, {
      input,
      snapshot: resolveCurationSnapshot({
        step: "commenting",
        trackCtx: input.trackCtx,
        count: 7,
        youAddress: YOU,
        activeId: "0xcol1",
        activeTopicId: 50121,
        decision: "rejected",
        comment: "draft text",
        decide,
        track,
      }),
    }).start();
    expect(commenting.getSnapshot().matches("commenting")).toBe(true);
    expect(commenting.getSnapshot().context.decision).toBe("rejected");
    expect(commenting.getSnapshot().context.comment).toBe("draft text");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(decide).not.toHaveBeenCalled();
    expect(commenting.getSnapshot().matches("commenting")).toBe(true);

    const reviewing = createActor(curationMachine, {
      input,
      snapshot: resolveCurationSnapshot({
        step: "reviewing",
        trackCtx: input.trackCtx,
        count: 7,
        youAddress: YOU,
        activeId: "0xcol1",
        track,
      }),
    }).start();
    expect(reviewing.getSnapshot().matches("reviewing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    reviewing.send({ type: "DRAFT_DECISION", status: "approved" });
    expect(reviewing.getSnapshot().matches("commenting")).toBe(true);
  });
});

describe("curateCommittee \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and commenting needs OPEN_REVIEW + DRAFT_DECISION", () => {
    const paths = getShortestPaths(curationMachine, {
      input: inputFor(okAssign, okDecide, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const v = p.state.value as string;
      ends.add(v);
      expect(EXPECTED_STATES.has(v)).toBe(true);
    }
    expect(ends.has("reviewing")).toBe(true);
    expect(ends.has("commenting")).toBe(true);

    const commenting = paths.find((p) => (p.state.value as string) === "commenting");
    expect(commenting).toBeDefined();
    const events = commenting!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["OPEN_REVIEW", "DRAFT_DECISION"]));
  });
});

describe("curateCommittee \u{2014} telemetry (review -> comment -> decide)", () => {
  it("approve WITH a comment fires comment_added before decided; deciding WITHOUT a comment skips it but still decides", async () => {
    const track = vi.fn();
    const decide = vi.fn(okDecide);
    const actor = createActor(curationMachine, {
      input: inputFor(okAssign, decide, track),
    }).start();

    actor.send({ type: "OPEN_REVIEW", id: "0xcol1", topicId: 50121 });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "DRAFT_DECISION", status: "approved" });
    expect(actor.getSnapshot().matches("commenting")).toBe(true);

    actor.send({ type: "SUBMIT", comment: "All items pass \u{2014} approving." });
    await waitFor(actor, (s) => s.matches("decided"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        CURATION_EVENTS.reviewOpened,
        CURATION_EVENTS.commentAdded,
        CURATION_EVENTS.decided,
      ]),
    );
    expect(events.indexOf(CURATION_EVENTS.commentAdded)).toBeLessThan(
      events.indexOf(CURATION_EVENTS.decided),
    );

    const call = decide.mock.calls[0]?.[0];
    expect(call?.body).toEqual({ status: "approved" });
    expect(call?.comment).toEqual({ raw: "All items pass \u{2014} approving.", topic_id: 50121 });

    const commentCall = track.mock.calls.find((c) => c[0] === CURATION_EVENTS.commentAdded);
    expect(commentCall?.[1]).toMatchObject({ id: "0xcol1", decision: "approved", has_comment: true });
    expect((commentCall?.[1] as { length: number }).length).toBeGreaterThan(0);
    expect(commentCall?.[2]).toMatchObject({ experimentKey: "bd_curation_comments", variant: "comments" });

    const silentTrack = vi.fn();
    const silentDecide = vi.fn(okDecide);
    const silent = createActor(curationMachine, {
      input: inputFor(okAssign, silentDecide, silentTrack),
    }).start();
    silent.send({ type: "OPEN_REVIEW", id: "0xcol1", topicId: 50121 });
    silent.send({ type: "DRAFT_DECISION", status: "rejected" });
    silent.send({ type: "SUBMIT", comment: "   " });
    await waitFor(silent, (s) => s.matches("decided"));

    const silentEvents = silentTrack.mock.calls.map((c) => c[0]);
    expect(silentEvents).toContain(CURATION_EVENTS.decided);
    expect(silentEvents).not.toContain(CURATION_EVENTS.commentAdded);
    expect(silentDecide.mock.calls[0]?.[0].comment).toBeUndefined();
    const decidedCall = silentTrack.mock.calls.find((c) => c[0] === CURATION_EVENTS.decided);
    expect(decidedCall?.[1]).toMatchObject({ status: "rejected", has_comment: false });
  });
});

describe("curateCommittee \u{2014} assign + filter", () => {
  it("FILTER fires bd_curation_filtered and stays on the dashboard; ASSIGN runs the PATCH and returns to dashboard", async () => {
    const track = vi.fn();
    const assign = vi.fn(okAssign);
    const actor = createActor(curationMachine, {
      input: inputFor(assign, okDecide, track),
    }).start();

    actor.send({
      type: "FILTER",
      filters: { status: "to_review", type: "ALL_TYPES", assignee: "all" },
    });
    expect(actor.getSnapshot().matches("dashboard")).toBe(true);
    expect(actor.getSnapshot().context.filters.status).toBe("to_review");
    expect(track.mock.calls.map((c) => c[0])).toContain(CURATION_EVENTS.filtered);

    actor.send({ type: "ASSIGN", id: "0xcol1" });
    await waitFor(actor, (s) => s.matches("dashboard") && !!s.context.assignResult);
    expect(assign.mock.calls[0]?.[0].body).toEqual({ assignee: YOU });
    expect(track.mock.calls.map((c) => c[0])).toContain(CURATION_EVENTS.assigned);
  });
});

describe("curateCommittee \u{2014} decide failure keeps the comment draft", () => {
  it("a failed decide returns to commenting WITH the draft, then retries to decided", async () => {
    const track = vi.fn();
    let calls = 0;
    const decide: DecideFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("forum unreachable");
      return okDecide(args);
    };
    const actor = createActor(curationMachine, {
      input: inputFor(okAssign, decide, track),
    }).start();

    actor.send({ type: "OPEN_REVIEW", id: "0xcol1", topicId: 50121 });
    actor.send({ type: "DRAFT_DECISION", status: "rejected" });
    actor.send({ type: "SUBMIT", comment: "Two emotes reuse frames." });
    await waitFor(actor, (s) => s.matches("commenting") && !!s.context.error);

    expect(actor.getSnapshot().context.comment).toBe("Two emotes reuse frames.");
    expect(actor.getSnapshot().context.error).toBe("forum unreachable");

    actor.send({ type: "SUBMIT", comment: "Two emotes reuse frames." });
    await waitFor(actor, (s) => s.matches("decided"));
    expect(track.mock.calls.map((c) => c[0])).toContain(CURATION_EVENTS.commentAdded);
  });
});

describe("simulateAssign / simulateDecide", () => {
  it("simulateDecide echoes status + a forum post, rejects an invalid status; simulateAssign echoes the assignee", async () => {
    const out = await simulateDecide({
      id: "0xcol1",
      body: { status: "approved" },
      comment: { raw: "ok", topic_id: 42 },
    });
    expect(out.status).toBe("approved");
    expect(out.simulated).toBe(true);
    expect(out.comment?.raw).toBe("ok");

    await expect(
      // @ts-expect-error -- deliberately invalid status
      simulateDecide({ id: "x", body: { status: "bogus" } }),
    ).rejects.toThrow(/Invalid Status/);

    const assigned = await simulateAssign({ id: "0xcol1", body: { assignee: YOU } });
    expect(assigned).toEqual({ id: "0xcol1", assignee: YOU, simulated: true });
  });
});
