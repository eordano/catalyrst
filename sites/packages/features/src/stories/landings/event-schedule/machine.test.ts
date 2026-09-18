import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  scheduleMachine,
  SCHEDULE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  FORM_ORDER,
  resolveScheduleSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type TrackFn,
} from "./machine";
import {
  emptyDraft,
  toUpsertBody,
  validateStep,
  isStepValid,
  type ScheduleDraft,
  type SubmitResult,
} from "@data/lib/catalyst/landings/schedules";

const RESULT: SubmitResult = { id: "local-test-schedule", active: true };

const okSubmit: SubmitFn = async () => RESULT;

function validDraft(): ScheduleDraft {
  return {
    ...emptyDraft(),
    name: "Summer Sounds 2026",
    description: "A recurring summer concert series.",
    background: ["#00D6CE", "#0B6E99"],
    activeSinceDate: "2026-07-04",
    activeUntilDate: "2026-08-30",
    active: true,
  };
}

function inputFor(submit: SubmitFn, track: TrackFn, draft = validDraft()) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-event-schedule",
      variant: "builder",
      experimentKey: "lp_schedule_builder",
    },
    draft,
    submit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "authGate",
  "basics",
  "dates",
  "review",
  "submitting",
  "created",
]);

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("scheduleMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to auth-gate", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(scheduleMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(new Set(Object.values(STATE_TO_SLUG))).toEqual(
      new Set(["auth-gate", "basics", "dates", "review", "submitting", "created"]),
    );
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("authGate");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
    expect(FORM_ORDER).toEqual(["basics", "dates", "review"]);
  });
});

describe("scheduleMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots authGate without a snapshot, hydrates submitting/review silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(submit, track).trackCtx;
    expect(resolveScheduleSnapshot({ step: "authGate", trackCtx })).toBeUndefined();

    const submitting = createActor(scheduleMachine, {
      input: inputFor(submit, track),
      snapshot: resolveScheduleSnapshot({
        step: "submitting",
        trackCtx,
        draft: validDraft(),
        submit,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(scheduleMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveScheduleSnapshot({ step: "review", trackCtx, draft: validDraft(), track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    review.send({ type: "SUBMIT" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(names(track)).toContain(SCHEDULE_EVENTS.submitAttempted);
  });
});

describe("scheduleMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and review takes SIGN_IN plus two NEXTs", () => {
    const paths = getShortestPaths(scheduleMachine, {
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
    for (const s of ["basics", "dates", "review", "submitting"]) expect(ends.has(s)).toBe(true);
    const review = paths.find((p) => (p.state.value as string) === "review");
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events.filter((e) => e === "NEXT").length).toBe(2);
  });
});

describe("scheduleMachine \u{2014} telemetry events (happy path)", () => {
  it("gate -> basics -> dates -> review -> submit -> created fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(scheduleMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    expect(names(track)).toContain(SCHEDULE_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("dates")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = names(track);
    for (const e of [
      SCHEDULE_EVENTS.started,
      SCHEDULE_EVENTS.reviewReached,
      SCHEDULE_EVENTS.submitAttempted,
      SCHEDULE_EVENTS.created,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(SCHEDULE_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(SCHEDULE_EVENTS.created),
    );
    const startedCall = track.mock.calls.find((c) => c[0] === SCHEDULE_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "lp_schedule_builder",
      variant: "builder",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("NEXT is blocked when the current step is invalid (guard holds)", () => {
    const actor = createActor(scheduleMachine, {
      input: inputFor(okSubmit, vi.fn(), emptyDraft()),
    }).start();
    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);
  });
});

describe("scheduleMachine \u{2014} submit failure + retry", () => {
  it("submit error -> back to review -> RETRY recovers to created", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okSubmit(args);
    };
    const actor = createActor(scheduleMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");
    expect(names(track)).toContain(SCHEDULE_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(names(track)).toContain(SCHEDULE_EVENTS.created);
  });
});

describe("schedule draft model", () => {
  it("basics needs a name and background color; dates needs start <= end", () => {
    expect(isStepValid("basics", emptyDraft())).toBe(false);
    expect(validateStep("basics", emptyDraft())).toHaveProperty("name");
    expect(isStepValid("basics", validDraft())).toBe(true);
    const d = { ...validDraft(), activeSinceDate: "2026-08-01", activeUntilDate: "2026-07-01" };
    expect(isStepValid("dates", d)).toBe(false);
    expect(validateStep("dates", d)).toHaveProperty("activeUntilDate");
    expect(isStepValid("dates", validDraft())).toBe(true);
  });

  it("toUpsertBody derives epoch-ms timestamps, omits schedule_id on create and carries it on edit", () => {
    const body = toUpsertBody(validDraft());
    expect(body.schedule_id).toBeUndefined();
    expect(body.name).toBe("Summer Sounds 2026");
    expect(typeof body.active_since).toBe("number");
    expect(body.active_until).toBeGreaterThan(body.active_since);
    expect(body.background).toEqual(["#00D6CE", "#0B6E99"]);
    expect(typeof body.signed_at).toBe("number");
    expect(toUpsertBody(validDraft(), "sample-mvfw-2026").schedule_id).toBe("sample-mvfw-2026");
  });
});

describe("simulateSubmit", () => {
  it("mints a local-sim id on create, preserves the id on edit, and rejects a nameless draft", async () => {
    const created = await simulateSubmit({ draft: validDraft() });
    expect(created.id).toMatch(/^local-sim-/);
    expect(created.active).toBe(true);
    const edited = await simulateSubmit({ draft: validDraft(), scheduleId: "sample-pride-2026" });
    expect(edited.id).toBe("sample-pride-2026");
    await expect(simulateSubmit({ draft: emptyDraft() })).rejects.toThrow(/name/);
  });
});
