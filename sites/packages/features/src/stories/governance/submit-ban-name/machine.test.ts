import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  banNameMachine,
  BAN_NAME_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBanNameSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";

const RESULT: SubmitResult = { proposalId: "sim-ban-name-test", stub: true };

const okSubmit: SubmitFn = async () => RESULT;

function inputFor(submit: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-ban-name",
      variant: "wizard",
      experimentKey: "gv_ban_name_wizard",
    },
    submit,
    track,
  };
}

const VALID_NAME = "Slur123";
const VALID_DESCRIPTION =
  "This name is offensive to a community and should be banned for safety reasons.";

const EXPECTED_STATES = new Set([
  "details",
  "description",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_NAME" as const, name: VALID_NAME },
  { type: "SUBMIT_NAME" as const, name: "!!" },
  { type: "SUBMIT_DESCRIPTION" as const, description: VALID_DESCRIPTION, coAuthors: [] },
  { type: "SUBMIT_DESCRIPTION" as const, description: "too short", coAuthors: [] },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("banNameMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(banNameMachine.states));
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

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.details);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("details");
    }
    for (const same of ["description", "review", "submitting", "success"]) {
      expect(slugToState(same)).toBe(same);
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("banNameMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; submitting hydrates without telemetry or auto-submit; a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okSubmit, () => {}).trackCtx;
    expect(resolveBanNameSnapshot({ step: "details", trackCtx })).toBeUndefined();

    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const submitting = createActor(banNameMachine, {
      input: inputFor(submit, track),
      snapshot: resolveBanNameSnapshot({
        step: "submitting",
        trackCtx,
        submit,
        track,
        draft: { name: VALID_NAME, description: VALID_DESCRIPTION, coAuthors: [] },
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.name).toBe(VALID_NAME);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const description = createActor(banNameMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveBanNameSnapshot({
        step: "description",
        trackCtx,
        track,
        draft: { name: VALID_NAME, description: "", coAuthors: [] },
      }),
    }).start();
    expect(description.getSnapshot().matches("description")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    description.send({ type: "SUBMIT_DESCRIPTION", description: VALID_DESCRIPTION, coAuthors: [] });
    expect(description.getSnapshot().matches("review")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BAN_NAME_EVENTS.descriptionSubmitted);
  });
});

describe("banNameMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and review passes through SUBMIT_NAME and SUBMIT_DESCRIPTION", () => {
    const paths = getShortestPaths(banNameMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["details", "description", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    expect(review!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["SUBMIT_NAME", "SUBMIT_DESCRIPTION"]),
    );
  });
});

describe("banNameMachine \u{2014} telemetry events (happy path + inline validation)", () => {
  it("invalid name and description stay put with inline errors; details -> description -> review -> submit -> success then fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(banNameMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(BAN_NAME_EVENTS.started);

    actor.send({ type: "SUBMIT_NAME", name: "bad name!" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(actor.getSnapshot().context.errors.name).toBeTruthy();
    let events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BAN_NAME_EVENTS.nameInvalid);
    expect(events).not.toContain(BAN_NAME_EVENTS.nameSubmitted);
    expect(events).not.toContain(BAN_NAME_EVENTS.reviewReached);

    actor.send({ type: "SUBMIT_NAME", name: VALID_NAME });
    expect(actor.getSnapshot().matches("description")).toBe(true);

    actor.send({ type: "SUBMIT_DESCRIPTION", description: "short", coAuthors: [] });
    expect(actor.getSnapshot().matches("description")).toBe(true);
    expect(actor.getSnapshot().context.errors.description).toBeTruthy();
    expect(track.mock.calls.map((c) => c[0])).not.toContain(BAN_NAME_EVENTS.reviewReached);

    actor.send({ type: "SUBMIT_DESCRIPTION", description: VALID_DESCRIPTION, coAuthors: [] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        BAN_NAME_EVENTS.started,
        BAN_NAME_EVENTS.nameSubmitted,
        BAN_NAME_EVENTS.descriptionSubmitted,
        BAN_NAME_EVENTS.reviewReached,
        BAN_NAME_EVENTS.submitting,
        BAN_NAME_EVENTS.submitted,
      ]),
    );
    expect(events.indexOf(BAN_NAME_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(BAN_NAME_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === BAN_NAME_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_ban_name_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("banNameMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success; simulateSubmit resolves a synthetic stub proposal id", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("simulated submit failed");
      return okSubmit(args);
    };

    const actor = createActor(banNameMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({ type: "SUBMIT_NAME", name: VALID_NAME });
    actor.send({ type: "SUBMIT_DESCRIPTION", description: VALID_DESCRIPTION, coAuthors: [] });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("simulated submit failed");
    expect(track.mock.calls.map((c) => c[0])).toContain(BAN_NAME_EVENTS.error);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(BAN_NAME_EVENTS.submitted);

    const res = await simulateSubmit({
      draft: { name: VALID_NAME, description: VALID_DESCRIPTION, coAuthors: [] },
    });
    expect(res.stub).toBe(true);
    expect(res.proposalId).toContain("sim-ban-name-");
  });
});
