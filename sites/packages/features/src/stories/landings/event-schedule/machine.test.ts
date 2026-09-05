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

describe("scheduleMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(scheduleMachine.states));
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

  it("the audit-spec step ids are exactly the slugs", () => {
    expect(new Set(Object.values(STATE_TO_SLUG))).toEqual(
      new Set(["auth-gate", "basics", "dates", "review", "submitting", "created"]),
    );
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    expect(slugToState(null)).toBe("authGate");
    expect(slugToState(undefined)).toBe("authGate");
    expect(slugToState("")).toBe("authGate");
    expect(slugToState("nope")).toBe("authGate");
    expect(slugToState("basics")).toBe("basics");
    expect(slugToState("dates")).toBe("dates");
    expect(slugToState("review")).toBe("review");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("FORM_ORDER is the linear forward path", () => {
    expect(FORM_ORDER).toEqual(["basics", "dates", "review"]);
  });
});

describe("scheduleMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveScheduleSnapshot({
      step: "authGate",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating `submitting` does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolveScheduleSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      draft: validDraft(),
      submit,
      track,
    });
    const actor = createActor(scheduleMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating `review` is telemetry-silent despite its entry action", () => {
    const track = vi.fn();
    const snapshot = resolveScheduleSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      draft: validDraft(),
      track,
    });
    const actor = createActor(scheduleMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveScheduleSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      draft: validDraft(),
      track,
    });
    const actor = createActor(scheduleMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(track).not.toHaveBeenCalled();
    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(SCHEDULE_EVENTS.submitAttempted);
  });
});

describe("scheduleMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("basics")).toBe(true);
    expect(ends.has("dates")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through SIGN_IN and two forward NEXTs", () => {
    const paths = getShortestPaths(scheduleMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
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

    expect(track.mock.calls.map((c) => c[0])).toContain(SCHEDULE_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("basics")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("dates")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SCHEDULE_EVENTS.started);
    expect(events).toContain(SCHEDULE_EVENTS.reviewReached);
    expect(events).toContain(SCHEDULE_EVENTS.submitAttempted);
    expect(events).toContain(SCHEDULE_EVENTS.created);

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
    const track = vi.fn();
    const actor = createActor(scheduleMachine, {
      input: inputFor(okSubmit, track, emptyDraft()),
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
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("review") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(SCHEDULE_EVENTS.submitFailed);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("created"));
    expect(track.mock.calls.map((c) => c[0])).toContain(SCHEDULE_EVENTS.created);
  });
});

describe("schedule draft model", () => {
  it("basics validation requires a name and a background color", () => {
    expect(isStepValid("basics", emptyDraft())).toBe(false);
    expect(validateStep("basics", emptyDraft())).toHaveProperty("name");
    expect(isStepValid("basics", validDraft())).toBe(true);
  });

  it("dates validation requires start <= end", () => {
    const d = { ...validDraft(), activeSinceDate: "2026-08-01", activeUntilDate: "2026-07-01" };
    expect(isStepValid("dates", d)).toBe(false);
    expect(validateStep("dates", d)).toHaveProperty("activeUntilDate");
    expect(isStepValid("dates", validDraft())).toBe(true);
  });

  it("toUpsertBody derives epoch-ms timestamps and omits schedule_id on create", () => {
    const body = toUpsertBody(validDraft());
    expect(body.schedule_id).toBeUndefined();
    expect(body.name).toBe("Summer Sounds 2026");
    expect(typeof body.active_since).toBe("number");
    expect(body.active_until).toBeGreaterThan(body.active_since);
    expect(body.background).toEqual(["#00D6CE", "#0B6E99"]);
    expect(typeof body.signed_at).toBe("number");
  });

  it("toUpsertBody carries schedule_id when editing (PATCH)", () => {
    const body = toUpsertBody(validDraft(), "sample-mvfw-2026");
    expect(body.schedule_id).toBe("sample-mvfw-2026");
  });
});

describe("simulateSubmit", () => {
  it("resolves a local-sim id for a create (no network)", async () => {
    const res = await simulateSubmit({ draft: validDraft() });
    expect(res.id).toMatch(/^local-sim-/);
    expect(res.active).toBe(true);
  });

  it("preserves the schedule id for an edit", async () => {
    const res = await simulateSubmit({ draft: validDraft(), scheduleId: "sample-pride-2026" });
    expect(res.id).toBe("sample-pride-2026");
  });

  it("rejects a draft without a name", async () => {
    await expect(simulateSubmit({ draft: emptyDraft() })).rejects.toThrow(/name/);
  });
});
